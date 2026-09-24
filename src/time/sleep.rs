use std::{
    sync::{Arc, Mutex},
    task::{Poll, Waker},
    thread,
    time::{Duration, Instant},
};

pub fn sleep(duration: Duration) -> Sleep {
    let deadline = Instant::now() + duration;

    Sleep {
        deadline,
        inner: Default::default(),
    }
}

pub fn sleep_until(instant: Instant) -> Sleep {
    Sleep {
        deadline: instant,
        inner: Default::default(),
    }
}

pub struct Sleep {
    deadline: Instant,
    inner: Arc<Mutex<SleepInner>>,
}

#[derive(Default)]
struct SleepInner {
    elapsed: bool,
    waker: Option<Waker>,
}

impl Sleep {
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn is_elapsed(&self) -> bool {
        Instant::now() >= self.deadline()
    }
}

impl Future for Sleep {
    type Output = ();
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut inner = self.inner.lock().unwrap();

        if inner.elapsed || Instant::now() >= self.deadline {
            inner.elapsed = true;
            inner.waker.take();

            return Poll::Ready(());
        }

        if let Some(ref mut waker) = inner.waker {
            if !waker.will_wake(cx.waker()) {
                *waker = cx.waker().clone();
            }
        } else {
            inner.waker = Some(cx.waker().clone());

            drop(inner);

            thread::spawn({
                let deadline = self.deadline;
                let inner = Arc::clone(&self.inner);

                move || {
                    let now = Instant::now();

                    if now < deadline {
                        thread::sleep(deadline - now);
                    }

                    let mut inner = inner.lock().unwrap();
                    inner.elapsed = true;
                    let waker = inner.waker.take();

                    drop(inner);

                    if let Some(waker) = waker {
                        waker.wake();
                    }
                }
            });
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use std::{pin::pin, task::Context};

    use crate::test_utils::{WakeCounter, get_wake_channel};

    use super::*;

    #[test]
    fn elapsed_deadline_is_ready_on_first_poll() {
        let counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&counter));
        let mut cx = Context::from_waker(&waker);

        let mut sleep = pin!(sleep_until(Instant::now() - Duration::from_mins(60)));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Ready(()));
        assert_eq!(counter.count(), 0);
    }

    #[test]
    fn sleep_expired_before_first_poll_is_ready() {
        let counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&counter));
        let mut cx = Context::from_waker(&waker);

        let mut sleep = pin!(sleep(Duration::from_millis(10)));

        std::thread::sleep(Duration::from_secs(1));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Ready(()));
        assert_eq!(counter.count(), 0);
    }

    #[test]
    fn pending_sleep_wakes_registered_waker_at_deadline() {
        let (wake, wake_rx) = get_wake_channel();

        let waker = Waker::from(Arc::new(wake));
        let mut cx = Context::from_waker(&waker);

        let mut sleep = pin!(sleep(Duration::from_millis(10)));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);

        wake_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("sleep should wake before the timeout");

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    #[test]
    fn deadline_wakes_most_recent_waker() {
        let counter_a = Arc::new(WakeCounter::default());
        let waker_a = Waker::from(Arc::clone(&counter_a));
        let mut cx_a = Context::from_waker(&waker_a);

        let (wake, wake_rx) = get_wake_channel();
        let waker_b = Waker::from(Arc::new(wake));
        let mut cx_b = Context::from_waker(&waker_b);

        let mut sleep = pin!(sleep(Duration::from_millis(10)));

        assert_eq!(sleep.as_mut().poll(&mut cx_a), Poll::Pending);
        assert_eq!(sleep.as_mut().poll(&mut cx_b), Poll::Pending);

        wake_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("sleep should wake before the timeout");

        assert_eq!(counter_a.count(), 0);
    }

    #[test]
    fn repeated_polls_produce_one_deadline_wakeup() {
        let counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&counter));
        let mut cx = Context::from_waker(&waker);

        let mut sleep = pin!(sleep(Duration::from_millis(10)));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);

        std::thread::sleep(Duration::from_secs(1));

        assert_eq!(counter.count(), 1);
    }

    #[test]
    fn dropping_sleep_does_not_cancel_helper() {
        let (wake, wake_rx) = get_wake_channel();

        let waker = Waker::from(Arc::new(wake));
        let mut cx = Context::from_waker(&waker);

        let mut sleep = Box::pin(sleep(Duration::from_millis(10)));

        assert_eq!(sleep.as_mut().poll(&mut cx), Poll::Pending);

        drop(sleep);

        wake_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("helper should have invoked retained waker");
    }
}
