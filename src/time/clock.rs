#[cfg(test)]
use std::{cell::RefCell, rc::Rc, time::Duration};

use std::time::Instant;

#[cfg(not(test))]
#[derive(Clone)]
pub(crate) struct Clock;

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct Clock {
    state: Rc<RefCell<ClockState>>,
}

#[cfg(test)]
struct ClockState {
    base: Instant,
    unpaused_at: Option<Instant>,
}

#[cfg(not(test))]
impl Clock {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn now(&self) -> Instant {
        Instant::now()
    }
}

#[cfg(test)]
impl Clock {
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        Self {
            state: Rc::new(RefCell::new(ClockState {
                base: now,
                unpaused_at: Some(now),
            })),
        }
    }

    pub(crate) fn new_paused() -> Self {
        let now = Instant::now();
        Self {
            state: Rc::new(RefCell::new(ClockState {
                base: now,
                unpaused_at: None,
            })),
        }
    }

    pub(crate) fn now(&self) -> Instant {
        let state = self.state.borrow();
        match state.unpaused_at {
            Some(instant) => state.base + Instant::now().duration_since(instant),
            None => state.base,
        }
    }

    pub(crate) fn pause(&self) {
        let mut state = self.state.borrow_mut();

        let unpaused_at = state.unpaused_at.take().expect("clock is already paused");

        state.base += Instant::now().duration_since(unpaused_at);
    }

    pub(crate) fn resume(&self) {
        let mut state = self.state.borrow_mut();

        assert!(state.unpaused_at.is_none(), "clock is already running");
        state.unpaused_at = Some(Instant::now());
    }

    pub(crate) fn advance(&self, duration: Duration) {
        let mut state = self.state.borrow_mut();

        if state.unpaused_at.is_some() {
            panic!("clock should be paused before advancing it");
        }

        state.base = state
            .base
            .checked_add(duration)
            .expect("clock advance exceeds Instant's range");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_clock_advances_by_requested_duration() {
        let clock = Clock::new_paused();
        let start = clock.now();

        clock.advance(Duration::from_secs(5));

        assert_eq!(clock.now(), start + Duration::from_secs(5));
    }

    #[test]
    fn clones_observe_the_same_time() {
        let clock = Clock::new_paused();
        let other = clock.clone();
        let start = clock.now();

        clock.advance(Duration::from_secs(5));

        assert_eq!(other.now(), start + Duration::from_secs(5));
    }

    #[test]
    fn running_clock_can_be_paused_and_advanced() {
        let clock = Clock::new();

        clock.pause();
        let paused_at = clock.now();

        clock.advance(Duration::from_secs(5));

        assert_eq!(clock.now(), paused_at + Duration::from_secs(5));
    }

    #[test]
    fn resume_preserves_virtual_time() {
        let clock = Clock::new_paused();
        let start = clock.now();

        clock.advance(Duration::from_secs(60 * 60));
        let advanced = clock.now();

        clock.resume();
        clock.pause();

        assert!(clock.now() >= advanced);
        assert!(clock.now() >= start + Duration::from_secs(60 * 60));
    }

    #[test]
    #[should_panic]
    fn running_clock_cannot_advance() {
        Clock::new().advance(Duration::from_secs(1));
    }
}
