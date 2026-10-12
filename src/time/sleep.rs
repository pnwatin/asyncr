use std::{
    task::Poll,
    time::{Duration, Instant},
};

use crate::{
    Handle,
    time::driver::{TimerHandle, TimerId},
};

pub fn sleep(handle: &Handle, duration: Duration) -> Sleep {
    let timer = handle.timer_handle.clone();
    let deadline = timer.now() + duration;

    Sleep::new(timer, deadline)
}

pub fn sleep_until(handle: &Handle, instant: Instant) -> Sleep {
    Sleep::new(handle.timer_handle.clone(), instant)
}

pub struct Sleep {
    deadline: Instant,
    timer: TimerHandle,
    registration: Option<TimerId>,
}

impl Sleep {
    fn new(timer: TimerHandle, deadline: Instant) -> Self {
        Self {
            deadline,
            timer,
            registration: None,
        }
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn is_elapsed(&self) -> bool {
        self.timer.now() >= self.deadline()
    }
}

impl Future for Sleep {
    type Output = ();
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if self.is_elapsed() {
            if let Some(id) = self.registration.take() {
                self.timer.cancel(id);
            }

            return Poll::Ready(());
        }

        if let Some(id) = self.registration {
            self.timer.update_waker(id, cx.waker());

            Poll::Pending
        } else {
            let id = self.timer.register(self.deadline, cx.waker().clone());
            self.registration = Some(id);

            Poll::Pending
        }
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if let Some(id) = self.registration {
            self.timer.cancel(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        pin::pin,
        task::{Context, Waker},
    };

    use crate::test_utils::{counting_waker, get_wake_channel, paused_timer_driver};

    use super::*;

    #[test]
    fn elapsed_deadline_is_immediately_ready() {
        let (_clock, mut _driver, timer) = paused_timer_driver();
        let (counter, waker) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        let deadline = timer.now() - Duration::from_secs(5);
        let mut sleep = pin!(Sleep::new(timer, deadline));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Ready(()));
        assert_eq!(counter.count(), 0);
    }

    #[test]
    fn sleep_expired_before_first_poll_is_ready() {
        let (clock, mut _driver, timer) = paused_timer_driver();
        let (counter, waker) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        let deadline = timer.now() + Duration::from_secs(5);
        let mut sleep = pin!(Sleep::new(timer, deadline));

        clock.advance(Duration::from_secs(5));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Ready(()));
        assert_eq!(counter.count(), 0);
    }

    #[test]
    fn pending_sleep_wakes_at_deadline() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let (waker, wake_rx) = get_wake_channel();
        let mut cx = Context::from_waker(&waker);

        let deadline = timer.now() + Duration::from_secs(5);
        let mut sleep = pin!(Sleep::new(timer, deadline));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        wake_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("sleep should wake before the timeout");

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    #[test]
    fn deadline_wakes_most_recent_waker() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let (counter_a, waker_a) = counting_waker();
        let mut cx_a = Context::from_waker(&waker_a);

        let (waker_b, wake_rx) = get_wake_channel();
        let mut cx_b = Context::from_waker(&waker_b);

        let deadline = timer.now() + Duration::from_secs(5);
        let mut sleep = pin!(Sleep::new(timer, deadline));

        assert_eq!(sleep.as_mut().poll(&mut cx_a), Poll::Pending);
        assert_eq!(sleep.as_mut().poll(&mut cx_b), Poll::Pending);

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        wake_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("sleep should wake before the timeout");

        assert_eq!(counter_a.count(), 0);
    }

    #[test]
    fn repeated_polls_produce_one_deadline_wakeup() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let (counter, waker) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        let deadline = timer.now() + Duration::from_secs(5);
        let mut sleep = pin!(Sleep::new(timer, deadline));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        assert_eq!(counter.count(), 1);
    }

    #[test]
    fn dropping_pending_sleep_cancel_registration() {
        let (_clock, mut driver, timer) = paused_timer_driver();

        let mut cx = Context::from_waker(Waker::noop());

        let deadline = timer.now() + Duration::from_secs(5);
        let mut sleep = Box::pin(Sleep::new(timer, deadline));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);

        assert_eq!(driver.next_deadline(), Some(deadline));
        drop(sleep);
        assert_eq!(driver.next_deadline(), None);
    }
}
