use std::time::Instant;

#[cfg(test)]
use std::{cell::RefCell, rc::Rc, time::Duration};

#[cfg(test)]
enum ClockMode {
    Real { started_at: Instant, base: Instant },
    Paused { now: Instant },
}

#[cfg(not(test))]
#[derive(Clone)]
pub(crate) struct Clock;

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct Clock {
    state: Rc<RefCell<ClockMode>>,
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
            state: Rc::new(RefCell::new(ClockMode::Real {
                started_at: now,
                base: now,
            })),
        }
    }

    pub(crate) fn new_paused() -> Self {
        Self {
            state: Rc::new(RefCell::new(ClockMode::Paused {
                now: Instant::now(),
            })),
        }
    }

    pub(crate) fn now(&self) -> Instant {
        let state = self.state.borrow();
        match *state {
            ClockMode::Real { started_at, base } => {
                base + Instant::now().duration_since(started_at)
            }
            ClockMode::Paused { now } => now,
        }
    }

    pub(crate) fn pause(&self) {
        let mut state = self.state.borrow_mut();

        match *state {
            ClockMode::Real { started_at, base } => {
                *state = ClockMode::Paused {
                    now: base + Instant::now().duration_since(started_at),
                }
            }
            ClockMode::Paused { .. } => {
                panic!("clock is already paused");
            }
        }
    }

    pub(crate) fn resume(&self) {
        let mut state = self.state.borrow_mut();

        match *state {
            ClockMode::Paused { now } => {
                *state = ClockMode::Real {
                    started_at: Instant::now(),
                    base: now,
                }
            }
            ClockMode::Real { .. } => {
                panic!("clock is already running");
            }
        }
    }

    pub(crate) fn advance(&self, duration: Duration) {
        let mut state = self.state.borrow_mut();

        match &mut *state {
            ClockMode::Paused { now } => {
                *now = now
                    .checked_add(duration)
                    .expect("clock advance exceeds the supported range")
            }
            ClockMode::Real { .. } => {
                panic!("clock should be paused before advancing it");
            }
        }
    }

    pub(crate) fn advance_to(&self, instant: Instant) {
        let mut state = self.state.borrow_mut();

        match &mut *state {
            ClockMode::Paused { now } => {
                assert!(instant >= *now, "cannot move a paused clock backwards");
                *now = instant
            }
            ClockMode::Real { .. } => {
                panic!("clock should be paused before advancing it");
            }
        }
    }

    pub(crate) fn is_paused(&self) -> bool {
        let state = self.state.borrow();

        matches!(*state, ClockMode::Paused { .. })
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
