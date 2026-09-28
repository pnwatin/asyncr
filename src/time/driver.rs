use std::{
    cell::RefCell,
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, hash_map::Entry},
    rc::Rc,
    task::Waker,
    time::Instant,
};

use crate::time::clock::Clock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct TimerId(u64);

impl TimerId {
    fn new(id: impl Into<u64>) -> Self {
        Self(id.into())
    }
}

#[derive(Default)]
struct TimerState {
    next_id: u64,
    deadlines: BinaryHeap<Reverse<(Instant, TimerId)>>,
    registrations: HashMap<TimerId, TimerRegistration>,
}

#[derive(Clone)]
pub(crate) struct TimerHandle {
    state: Rc<RefCell<TimerState>>,
    clock: Clock,
}

pub(crate) struct TimerDriver {
    state: Rc<RefCell<TimerState>>,
    clock: Clock,
}

impl TimerHandle {
    pub(crate) fn register(&self, deadline: Instant, waker: Waker) -> TimerId {
        let id = self.next_id();

        let mut state = self.state.borrow_mut();

        state.deadlines.push(Reverse((deadline, id)));
        state
            .registrations
            .insert(id, TimerRegistration { deadline, waker });

        id
    }

    pub(crate) fn update_waker(&self, id: TimerId, waker: &Waker) {
        let mut state = self.state.borrow_mut();

        let registration = state
            .registrations
            .get_mut(&id)
            .expect("pending timer should have registration");

        if !registration.waker.will_wake(waker) {
            registration.waker = waker.clone();
        }
    }

    pub(crate) fn cancel(&self, id: TimerId) {
        self.state.borrow_mut().registrations.remove(&id);
    }

    pub(crate) fn reschedule(&self, id: TimerId, new_deadline: Instant) {
        let mut state = self.state.borrow_mut();

        let registration = state
            .registrations
            .get_mut(&id)
            .expect("cannot reschedule an inactive timer");

        registration.deadline = new_deadline;
        state.deadlines.push(Reverse((new_deadline, id)));
    }

    pub(crate) fn now(&self) -> Instant {
        self.clock.now()
    }

    fn next_id(&self) -> TimerId {
        let mut state = self.state.borrow_mut();
        let id = TimerId::new(state.next_id);

        state.next_id = state.next_id.wrapping_add(1);

        if state.registrations.contains_key(&id) {
            panic!("timer id exhausted");
        }

        id
    }
}

impl TimerDriver {
    pub(crate) fn new(clock: Clock) -> (Self, TimerHandle) {
        let state = Rc::new(RefCell::new(TimerState::default()));

        let handle = TimerHandle {
            state: Rc::clone(&state),
            clock: clock.clone(),
        };
        let driver = TimerDriver { state, clock };

        (driver, handle)
    }

    pub(crate) fn process_expired(&mut self) {
        let mut wakers = Vec::new();

        let now = self.clock.now();

        {
            let mut state = self.state.borrow_mut();

            while let Some(&Reverse((deadline, id))) = state.deadlines.peek() {
                if deadline > now {
                    break;
                }

                state.deadlines.pop();

                if let Entry::Occupied(entry) = state.registrations.entry(id)
                    && deadline == entry.get().deadline
                {
                    let (_, registration) = entry.remove_entry();
                    wakers.push(registration.waker);
                }
            }
        }

        for waker in wakers {
            waker.wake();
        }
    }

    pub(crate) fn next_deadline(&mut self) -> Option<Instant> {
        let mut state = self.state.borrow_mut();

        loop {
            let &Reverse((deadline, id)) = state.deadlines.peek()?;
            let is_current = state
                .registrations
                .get(&id)
                .is_some_and(|registration| registration.deadline == deadline);

            if is_current {
                return Some(deadline);
            }

            state.deadlines.pop();
        }
    }
}

struct TimerRegistration {
    deadline: Instant,
    waker: Waker,
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use crate::test_utils::WakeCounter;

    use super::*;

    #[test]
    fn timer_expires_when_shared_clock_reaches_deadline() {
        let clock = Clock::new_paused();
        let (mut driver, handle) = TimerDriver::new(clock.clone());

        let counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&counter));

        let deadline = handle.now() + Duration::from_secs(5);
        handle.register(deadline, waker);

        driver.process_expired();
        assert_eq!(counter.count(), 0);

        clock.advance(Duration::from_secs(5));

        driver.process_expired();
        assert_eq!(counter.count(), 1);

        driver.process_expired();
        assert_eq!(counter.count(), 1);

        assert_eq!(driver.next_deadline(), None);
    }

    #[test]
    fn cancellation_ignores_stale_heap_entries() {
        let clock = Clock::new_paused();
        let (mut driver, handle) = TimerDriver::new(clock.clone());

        let counter_a = Arc::new(WakeCounter::default());
        let waker_a = Waker::from(Arc::clone(&counter_a));

        let counter_b = Arc::new(WakeCounter::default());
        let waker_b = Waker::from(Arc::clone(&counter_b));

        let deadline_a = handle.now() + Duration::from_secs(5);
        let deadline_b = handle.now() + Duration::from_secs(10);
        let id_a = handle.register(deadline_a, waker_a);
        handle.register(deadline_b, waker_b);

        handle.cancel(id_a);

        assert_eq!(driver.next_deadline(), Some(deadline_b));

        clock.advance(Duration::from_secs(15));
        driver.process_expired();

        assert_eq!(counter_a.count(), 0);
        assert_eq!(counter_b.count(), 1);
    }

    #[test]
    fn rescheduling_later_does_not_fire_at_the_old_deadline() {
        let clock = Clock::new_paused();
        let (mut driver, handle) = TimerDriver::new(clock.clone());

        let counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&counter));

        let now = handle.now();

        let deadline = now + Duration::from_secs(5);
        let id = handle.register(deadline, waker);

        handle.reschedule(id, now + Duration::from_secs(10));

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        assert_eq!(counter.count(), 0);
        assert_eq!(driver.next_deadline(), Some(now + Duration::from_secs(10)));

        clock.advance(Duration::from_secs(5));
        driver.process_expired();
        assert_eq!(counter.count(), 1);
    }

    #[test]
    fn updating_the_waker_replaces_the_old_one() {
        let clock = Clock::new_paused();
        let (mut driver, handle) = TimerDriver::new(clock.clone());

        let counter_a = Arc::new(WakeCounter::default());
        let waker_a = Waker::from(Arc::clone(&counter_a));

        let counter_b = Arc::new(WakeCounter::default());
        let waker_b = Waker::from(Arc::clone(&counter_b));

        let deadline = handle.now() + Duration::from_secs(5);
        let id = handle.register(deadline, waker_a);
        handle.update_waker(id, &waker_b);

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        assert_eq!(counter_a.count(), 0);
        assert_eq!(counter_b.count(), 1);
    }

    #[test]
    fn timers_expire_in_deadline_order() {
        let clock = Clock::new_paused();
        let (mut driver, handle) = TimerDriver::new(clock.clone());

        let counter_a = Arc::new(WakeCounter::default());
        let waker_a = Waker::from(Arc::clone(&counter_a));
        let deadline_a = handle.now() + Duration::from_secs(10);

        let counter_b = Arc::new(WakeCounter::default());
        let waker_b = Waker::from(Arc::clone(&counter_b));
        let deadline_b = handle.now() + Duration::from_secs(5);

        let counter_c = Arc::new(WakeCounter::default());
        let waker_c = Waker::from(Arc::clone(&counter_c));
        let deadline_c = handle.now() + Duration::from_secs(7);

        handle.register(deadline_a, waker_a);
        handle.register(deadline_b, waker_b);
        handle.register(deadline_c, waker_c);

        assert_eq!(driver.next_deadline(), Some(deadline_b));

        clock.advance(Duration::from_secs(5));
        driver.process_expired();

        assert_eq!(counter_a.count(), 0);
        assert_eq!(counter_b.count(), 1);
        assert_eq!(counter_c.count(), 0);
        assert_eq!(driver.next_deadline(), Some(deadline_c));

        clock.advance(Duration::from_secs(2));
        driver.process_expired();

        assert_eq!(counter_a.count(), 0);
        assert_eq!(counter_b.count(), 1);
        assert_eq!(counter_c.count(), 1);
        assert_eq!(driver.next_deadline(), Some(deadline_a));

        clock.advance(Duration::from_secs(3));
        driver.process_expired();
        assert_eq!(counter_a.count(), 1);
        assert_eq!(counter_b.count(), 1);
        assert_eq!(counter_c.count(), 1);
        assert_eq!(driver.next_deadline(), None);
    }
}
