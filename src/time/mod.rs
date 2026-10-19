pub(crate) mod clock;
pub(crate) mod driver;
mod interval;
mod sleep;
mod timeout;

use std::time::{Duration, Instant};

pub use interval::{Interval, MissedTickBehavior, interval, interval_at};
pub use sleep::{Sleep, sleep, sleep_until};
pub use timeout::{Elapsed, Timeout, timeout, timeout_at};

pub(crate) fn deadline_after(instant: Instant, duration: Duration) -> Instant {
    instant
        .checked_add(duration)
        .expect("timer duration exceeds the supported range")
}
