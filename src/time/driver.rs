use std::{
    cell::RefCell,
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, hash_map::Entry},
    rc::Rc,
    task::Waker,
    time::Instant,
};

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
}

pub(crate) struct TimerDriver {
    state: Rc<RefCell<TimerState>>,
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
    pub(crate) fn new() -> (Self, TimerHandle) {
        let state = Rc::new(RefCell::new(TimerState::default()));

        let handle = TimerHandle {
            state: Rc::clone(&state),
        };
        let driver = TimerDriver { state };

        (driver, handle)
    }

    pub(crate) fn process_expired(&mut self, now: Instant) {
        let mut wakers = Vec::new();

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
