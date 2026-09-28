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
pub(crate) struct TimerDriver {
    next_id: u64,
    deadlines: BinaryHeap<Reverse<(Instant, TimerId)>>,
    registrations: HashMap<TimerId, TimerRegistration>,
}

impl TimerDriver {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&mut self, deadline: Instant, waker: Waker) -> TimerId {
        let id = self.next_id();

        self.deadlines.push(Reverse((deadline, id)));
        self.registrations
            .insert(id, TimerRegistration { deadline, waker });

        id
    }

    pub(crate) fn update_waker(&mut self, id: TimerId, waker: &Waker) {
        let registration = self
            .registrations
            .get_mut(&id)
            .expect("pending timer should have registration");

        if !registration.waker.will_wake(waker) {
            registration.waker = waker.clone();
        }
    }

    pub(crate) fn cancel(&mut self, id: TimerId) {
        self.registrations.remove(&id);
    }

    pub(crate) fn reschedule(&mut self, id: TimerId, new_deadline: Instant) {
        let registration = self
            .registrations
            .get_mut(&id)
            .expect("cannot reschedule an inactive timer");

        registration.deadline = new_deadline;
        self.deadlines.push(Reverse((new_deadline, id)));
    }

    pub(crate) fn process_expired(&mut self, now: Instant) -> Vec<Waker> {
        let mut wakers = Vec::new();

        while let Some(&Reverse((deadline, id))) = self.deadlines.peek() {
            if deadline > now {
                break;
            }

            self.deadlines.pop();

            if let Entry::Occupied(entry) = self.registrations.entry(id)
                && deadline == entry.get().deadline
            {
                let (_, registration) = entry.remove_entry();
                wakers.push(registration.waker);
            }
        }

        wakers
    }

    pub(crate) fn next_deadline(&mut self) -> Option<Instant> {
        loop {
            let &Reverse((deadline, id)) = self.deadlines.peek()?;
            let is_current = self
                .registrations
                .get(&id)
                .is_some_and(|registration| registration.deadline == deadline);

            if is_current {
                return Some(deadline);
            }

            self.deadlines.pop();
        }
    }

    fn next_id(&mut self) -> TimerId {
        let id = TimerId::new(self.next_id);

        self.next_id = self.next_id.wrapping_add(1);

        if self.registrations.contains_key(&id) {
            panic!("timer id exhausted");
        }

        id
    }
}

struct TimerRegistration {
    deadline: Instant,
    waker: Waker,
}
