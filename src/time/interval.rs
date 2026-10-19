use std::{
    future::poll_fn,
    pin::Pin,
    task::{Context, Poll, ready},
    time::{Duration, Instant},
};

use crate::{
    Handle,
    time::{Sleep, sleep_until},
};

pub fn interval(handle: &Handle, period: Duration) -> Interval {
    interval_at(handle, handle.timer_handle.now(), period)
}

pub fn interval_at(handle: &Handle, start: Instant, period: Duration) -> Interval {
    Interval::new(
        sleep_until(handle, start),
        period,
        MissedTickBehavior::default(),
    )
}

pub struct Interval {
    sleep: Sleep,
    period: Duration,
    missed_tick_behavior: MissedTickBehavior,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissedTickBehavior {
    #[default]
    Burst,
    Skip,
    Delay,
}

impl MissedTickBehavior {
    fn next_timeout(&self, timeout: Instant, now: Instant, period: Duration) -> Instant {
        match self {
            MissedTickBehavior::Burst => timeout + period,
            MissedTickBehavior::Delay => now + period,
            MissedTickBehavior::Skip => {
                let elapsed = now.duration_since(timeout);
                let remainder = Duration::from_nanos_u128(elapsed.as_nanos() % period.as_nanos());

                now + period - remainder
            }
        }
    }
}

impl Interval {
    const MISSED_TICK_GRACE_DELAY: Duration = Duration::from_millis(5);

    fn new(sleep: Sleep, period: Duration, behavior: MissedTickBehavior) -> Self {
        assert!(!period.is_zero(), "interval period should be non-zero");

        Self {
            sleep,
            period,
            missed_tick_behavior: behavior,
        }
    }

    pub async fn tick(&mut self) -> Instant {
        poll_fn(|cx| self.poll_tick(cx)).await
    }

    pub fn poll_tick(&mut self, cx: &mut Context<'_>) -> Poll<Instant> {
        ready!(Pin::new(&mut self.sleep).poll(cx));

        let timeout = self.sleep.deadline();
        let now = self.sleep.now();

        let next_timeout = if now > timeout + Self::MISSED_TICK_GRACE_DELAY {
            self.missed_tick_behavior
                .next_timeout(timeout, now, self.period)
        } else {
            timeout + self.period
        };

        self.sleep.reset(next_timeout);

        Poll::Ready(timeout)
    }

    pub fn period(&self) -> Duration {
        self.period
    }

    pub fn reset_at(&mut self, deadline: Instant) {
        self.sleep.reset(deadline);
    }

    pub fn reset_after(&mut self, after: Duration) {
        self.reset_at(self.sleep.now() + after);
    }

    pub fn reset(&mut self) {
        self.reset_at(self.sleep.now() + self.period);
    }

    pub fn reset_immediately(&mut self) {
        self.reset_at(self.sleep.now());
    }

    pub fn missed_tick_behavior(&self) -> MissedTickBehavior {
        self.missed_tick_behavior
    }

    pub fn set_missed_tick_behavior(&mut self, behavior: MissedTickBehavior) {
        self.missed_tick_behavior = behavior;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        pin::pin,
        task::{Context, Poll},
    };

    use crate::test_utils::{counting_waker, noop_context, paused_timer_driver};

    use super::*;

    fn new_interval(
        timer: crate::time::driver::TimerHandle,
        start: Instant,
        period: Duration,
    ) -> Interval {
        Interval::new(Sleep::new(timer, start), period, MissedTickBehavior::Burst)
    }

    #[test]
    fn first_tick_is_immediate_and_next_tick_waits_one_period() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let start = timer.now();
        let period = Duration::from_secs(5);
        let mut interval = new_interval(timer, start, period);

        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Ready(start));

        let (counter, waker) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        assert_eq!(interval.poll_tick(&mut cx), Poll::Pending);
        assert_eq!(driver.next_deadline(), Some(start + period));

        clock.advance(period);
        driver.process_expired();

        assert_eq!(counter.count(), 1);
        assert_eq!(interval.poll_tick(&mut cx), Poll::Ready(start + period));
    }

    #[test]
    fn burst_behavior_returns_each_missed_tick() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let start = timer.now();
        let period = Duration::from_secs(10);
        let mut interval = new_interval(timer, start, period);

        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Ready(start));

        clock.advance(Duration::from_secs(35));

        assert_eq!(
            interval.poll_tick(&mut noop_context()),
            Poll::Ready(start + Duration::from_secs(10))
        );
        assert_eq!(
            interval.poll_tick(&mut noop_context()),
            Poll::Ready(start + Duration::from_secs(20))
        );
        assert_eq!(
            interval.poll_tick(&mut noop_context()),
            Poll::Ready(start + Duration::from_secs(30))
        );
        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Pending);
        assert_eq!(
            driver.next_deadline(),
            Some(start + Duration::from_secs(40))
        );
    }

    #[test]
    fn delay_behavior_schedules_from_when_missed_tick_is_observed() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let start = timer.now();
        let period = Duration::from_secs(10);
        let mut interval = new_interval(timer, start, period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Ready(start));

        clock.advance(Duration::from_secs(15));

        assert_eq!(
            interval.poll_tick(&mut noop_context()),
            Poll::Ready(start + period)
        );
        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Pending);
        assert_eq!(
            driver.next_deadline(),
            Some(start + Duration::from_secs(25))
        );
    }

    #[test]
    fn skip_behavior_advances_to_next_scheduled_tick() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let start = timer.now();
        let period = Duration::from_secs(10);
        let mut interval = new_interval(timer, start, period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Ready(start));

        clock.advance(Duration::from_secs(35));

        assert_eq!(
            interval.poll_tick(&mut noop_context()),
            Poll::Ready(start + period)
        );
        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Pending);
        assert_eq!(
            driver.next_deadline(),
            Some(start + Duration::from_secs(40))
        );
    }

    #[test]
    fn reset_at_reschedules_pending_tick() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let start = timer.now();
        let period = Duration::from_secs(10);
        let reset_deadline = start + Duration::from_secs(5);
        let mut interval = new_interval(timer, start, period);

        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Ready(start));
        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Pending);

        interval.reset_at(reset_deadline);

        assert_eq!(driver.next_deadline(), Some(reset_deadline));

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        assert_eq!(
            interval.poll_tick(&mut noop_context()),
            Poll::Ready(reset_deadline)
        );
    }

    #[test]
    fn dropping_pending_tick_preserves_interval_and_updates_waker() {
        let (clock, mut driver, timer) = paused_timer_driver();
        let start = timer.now();
        let period = Duration::from_secs(5);
        let mut interval = new_interval(timer, start + period, period);
        let (counter_a, waker_a) = counting_waker();
        let (counter_b, waker_b) = counting_waker();
        let mut cx_a = Context::from_waker(&waker_a);
        let mut cx_b = Context::from_waker(&waker_b);

        {
            let mut tick = pin!(interval.tick());
            assert_eq!(tick.as_mut().poll(&mut cx_a), Poll::Pending);
        }

        {
            let mut tick = pin!(interval.tick());
            assert_eq!(tick.as_mut().poll(&mut cx_b), Poll::Pending);
        }

        clock.advance(period);
        driver.process_expired();

        assert_eq!(counter_a.count(), 0);
        assert_eq!(counter_b.count(), 1);
        assert_eq!(interval.poll_tick(&mut cx_b), Poll::Ready(start + period));
    }

    #[test]
    fn dropping_interval_cancels_pending_tick() {
        let (_clock, mut driver, timer) = paused_timer_driver();
        let start = timer.now();
        let period = Duration::from_secs(5);
        let deadline = start + period;
        let mut interval = new_interval(timer, deadline, period);

        assert_eq!(interval.poll_tick(&mut noop_context()), Poll::Pending);
        assert_eq!(driver.next_deadline(), Some(deadline));

        drop(interval);

        assert_eq!(driver.next_deadline(), None);
    }

    #[test]
    #[should_panic(expected = "interval period should be non-zero")]
    fn zero_period_is_rejected() {
        let (_clock, _driver, timer) = paused_timer_driver();
        let start = timer.now();

        let _interval = new_interval(timer, start, Duration::ZERO);
    }
}
