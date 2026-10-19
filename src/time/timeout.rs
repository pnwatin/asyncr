use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    pin::Pin,
    task::Poll,
    time::{Duration, Instant},
};

use crate::{
    Handle,
    time::{Sleep, sleep_until},
};

pub fn timeout<F>(handle: &Handle, duration: Duration, future: F) -> Timeout<F>
where
    F: Future,
{
    let deadline = handle.timer_handle.now() + duration;

    timeout_at(handle, deadline, future)
}

pub fn timeout_at<F>(handle: &Handle, deadline: Instant, future: F) -> Timeout<F>
where
    F: Future,
{
    Timeout::new(sleep_until(handle, deadline), future)
}

pub struct Timeout<F> {
    future: Option<Pin<Box<F>>>,
    sleep: Option<Sleep>,
}

impl<F> Timeout<F> {
    fn new(sleep: Sleep, future: F) -> Self {
        Self {
            sleep: Some(sleep),
            future: Some(Box::pin(future)),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Elapsed;

impl Display for Elapsed {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("deadline has elapsed")
    }
}

impl Error for Elapsed {}

impl<F> Future for Timeout<F>
where
    F: Future,
{
    type Output = Result<F::Output, Elapsed>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let fut_result = self
            .future
            .as_mut()
            .expect("Timeout polled after completion")
            .as_mut()
            .poll(cx);

        if let Poll::Ready(result) = fut_result {
            self.future.take();
            self.sleep.take();

            return Poll::Ready(Ok(result));
        }

        let sleep = self
            .sleep
            .as_mut()
            .expect("Timeout polled after completion");

        if Pin::new(sleep).poll(cx).is_ready() {
            self.future.take();
            self.sleep.take();

            return Poll::Ready(Err(Elapsed));
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        future::{pending, poll_fn},
        pin::pin,
        rc::Rc,
        task::{Context, Poll},
        time::Duration,
    };

    use crate::test_utils::{counting_waker, noop_context, paused_timer_driver};

    use super::*;

    struct DropFlag(Rc<Cell<bool>>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    #[test]
    fn immediately_ready_future_wins_when_deadline_is_elapsed() {
        let (_clock, _driver, timer) = paused_timer_driver();
        let deadline = timer.now() - Duration::from_secs(1);
        let sleep = Sleep::new(timer, deadline);
        let mut timeout = pin!(Timeout::new(sleep, async { 42 }));

        let result = timeout.as_mut().poll(&mut noop_context());

        assert_eq!(result, Poll::Ready(Ok(42)));
    }

    #[test]
    fn pending_future_times_out_at_deadline() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let (counter, waker) = counting_waker();
        let mut cx = Context::from_waker(&waker);
        let deadline = timer.now() + Duration::from_secs(5);
        let sleep = Sleep::new(timer, deadline);
        let mut timeout = pin!(Timeout::new(sleep, poll_fn(|_| Poll::<()>::Pending)));

        assert_eq!(timeout.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(driver.next_deadline(), Some(deadline));

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        assert_eq!(counter.count(), 1);
        assert_eq!(timeout.as_mut().poll(&mut cx), Poll::Ready(Err(Elapsed)));
        assert_eq!(driver.next_deadline(), None);
    }

    #[test]
    fn future_completes_before_deadline() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let (counter, waker) = counting_waker();
        let mut cx = Context::from_waker(&waker);
        let deadline = timer.now() + Duration::from_secs(10);
        let sleep = Sleep::new(timer.clone(), deadline);
        let deadline_inner = timer.now() + Duration::from_secs(5);
        let sleep_inner = Sleep::new(timer, deadline_inner);
        let mut timeout = pin!(Timeout::new(sleep, async {
            sleep_inner.await;
            42
        }));

        assert_eq!(timeout.as_mut().poll(&mut cx), Poll::Pending);

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        assert_eq!(counter.count(), 1);
        assert_eq!(timeout.as_mut().poll(&mut cx), Poll::Ready(Ok(42)));
    }

    #[test]
    fn timing_out_drops_wrapped_future() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let deadline = timer.now() + Duration::from_secs(5);
        let sleep = Sleep::new(timer, deadline);
        let dropped = Rc::new(Cell::new(false));
        let drop_flag = DropFlag(Rc::clone(&dropped));
        let future = async move {
            let _drop_flag = drop_flag;
            pending::<()>().await;
        };
        let mut timeout = pin!(Timeout::new(sleep, future));

        assert_eq!(timeout.as_mut().poll(&mut noop_context()), Poll::Pending);
        assert!(!dropped.get());

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        assert_eq!(
            timeout.as_mut().poll(&mut noop_context()),
            Poll::Ready(Err(Elapsed))
        );
        assert!(dropped.get());
    }

    #[test]
    fn successful_future_cancels_timeout_registration() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let (counter, waker) = counting_waker();
        let mut cx = Context::from_waker(&waker);
        let deadline = timer.now() + Duration::from_secs(5);
        let sleep = Sleep::new(timer, deadline);
        let ready = Rc::new(Cell::new(false));
        let future_ready = Rc::clone(&ready);
        let future = poll_fn(move |_| {
            if future_ready.get() {
                Poll::Ready(42)
            } else {
                Poll::Pending
            }
        });
        let mut timeout = pin!(Timeout::new(sleep, future));

        assert_eq!(timeout.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(driver.next_deadline(), Some(deadline));

        ready.set(true);

        assert_eq!(timeout.as_mut().poll(&mut cx), Poll::Ready(Ok(42)));
        assert_eq!(driver.next_deadline(), None);

        clock.advance(Duration::from_secs(5));
        driver.process_expired();
        assert_eq!(counter.count(), 0);
    }

    #[test]
    fn dropping_pending_timeout_drops_future_and_cancels_registration() {
        let (_clock, mut driver, timer) = paused_timer_driver();
        let deadline = timer.now() + Duration::from_secs(5);
        let sleep = Sleep::new(timer, deadline);
        let dropped = Rc::new(Cell::new(false));
        let drop_flag = DropFlag(Rc::clone(&dropped));
        let future = async move {
            let _drop_flag = drop_flag;
            pending::<()>().await;
        };
        let mut timeout = Box::pin(Timeout::new(sleep, future));

        assert_eq!(timeout.as_mut().poll(&mut noop_context()), Poll::Pending);
        assert_eq!(driver.next_deadline(), Some(deadline));
        assert!(!dropped.get());

        drop(timeout);

        assert!(dropped.get());
        assert_eq!(driver.next_deadline(), None);
    }
}
