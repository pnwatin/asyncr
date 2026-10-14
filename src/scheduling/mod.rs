mod state;

use std::{
    pin::Pin,
    sync::{Arc, Mutex, mpsc},
    task::{Context, Poll, Wake, Waker},
};

use crate::task::Id;
use state::{ScheduleAction, ScheduleState};

pub(super) struct TaskSchedule {
    id: Id,
    state: Mutex<ScheduleState>,
    ready_tx: mpsc::Sender<Id>,
}

impl TaskSchedule {
    pub(super) fn new(id: Id, ready_tx: mpsc::Sender<Id>) -> Arc<Self> {
        Arc::new(Self {
            id,
            state: Mutex::new(ScheduleState::Idle),
            ready_tx,
        })
    }

    pub(super) fn poll<F>(self: &Arc<Self>, future: Pin<&mut F>) -> Poll<F::Output>
    where
        F: Future + ?Sized,
    {
        self.state.lock().unwrap().begin_poll();

        let waker = self.waker();
        let mut cx = Context::from_waker(&waker);

        let result = future.poll(&mut cx);

        let outcome = match &result {
            Poll::Ready(_) => Poll::Ready(()),
            Poll::Pending => Poll::Pending,
        };

        let mut state = self.state.lock().unwrap();
        let action = state.finish_poll(outcome);
        drop(state);

        if let ScheduleAction::Enqueue = action {
            let _ = self.ready_tx.send(self.id);
        }

        result
    }

    pub(super) fn request_schedule(&self) {
        let mut state = self.state.lock().unwrap();
        let action = state.wake();
        drop(state);

        if let ScheduleAction::Enqueue = action {
            let _ = self.ready_tx.send(self.id);
        }
    }

    fn waker(self: &Arc<Self>) -> Waker {
        Waker::from(self.clone())
    }
}

impl Wake for TaskSchedule {
    fn wake(self: Arc<Self>) {
        self.request_schedule();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.request_schedule();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::poll_fn,
        num::NonZeroU64,
        pin::pin,
        sync::mpsc::{TryRecvError, channel},
    };

    use super::*;

    #[test]
    fn new_schedule_is_initially_idle() {
        let (ready_tx, ready_rx) = channel();
        let task_id = id();

        let _schedule = TaskSchedule::new(task_id, ready_tx);

        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn request_schedule_enqueues_idle_task_once() {
        let (ready_tx, ready_rx) = channel();
        let task_id = id();

        let schedule = TaskSchedule::new(task_id, ready_tx);

        schedule.request_schedule();

        assert_eq!(ready_rx.try_recv(), Ok(task_id));

        schedule.request_schedule();
        schedule.request_schedule();
        schedule.request_schedule();

        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn wake_requeues_pending_idle_task() {
        let (ready_tx, ready_rx) = channel();
        let task_id = id();

        let schedule = TaskSchedule::new(task_id, ready_tx);

        schedule.request_schedule();

        assert_eq!(ready_rx.try_recv(), Ok(task_id));

        let mut future = pin!(poll_fn(|_| Poll::<()>::Pending));
        assert_eq!(schedule.poll(future.as_mut()), Poll::Pending);

        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));

        schedule.waker().wake();

        assert_eq!(ready_rx.try_recv(), Ok(task_id));
    }

    #[test]
    fn wake_during_poll_requeues_pending_task_once() {
        let (ready_tx, ready_rx) = channel();
        let task_id = id();

        let schedule = TaskSchedule::new(task_id, ready_tx);

        schedule.request_schedule();

        assert_eq!(ready_rx.try_recv(), Ok(task_id));

        let mut future = pin!(poll_fn(|cx| {
            cx.waker().wake_by_ref();
            cx.waker().wake_by_ref();
            cx.waker().wake_by_ref();

            assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));

            Poll::<()>::Pending
        }));
        assert_eq!(schedule.poll(future.as_mut()), Poll::Pending);
        assert_eq!(ready_rx.try_recv(), Ok(task_id));
        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn completed_task_is_not_requeued() {
        let (ready_tx, ready_rx) = channel();
        let task_id = id();

        let schedule = TaskSchedule::new(task_id, ready_tx);

        schedule.request_schedule();

        assert_eq!(ready_rx.try_recv(), Ok(task_id));

        let waker = schedule.waker();
        let mut future = pin!(poll_fn(|cx| {
            cx.waker().wake_by_ref();

            Poll::Ready(42)
        }));
        assert_eq!(schedule.poll(future.as_mut()), Poll::Ready(42));

        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));

        waker.wake_by_ref();
        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn wake_after_ready_queue_is_dropped_is_harmless() {
        let (ready_tx, ready_rx) = channel();

        let schedule = TaskSchedule::new(id(), ready_tx);
        let waker = schedule.waker();

        drop(ready_rx);

        waker.wake();
    }

    fn id() -> Id {
        Id::new(NonZeroU64::new(1).unwrap())
    }
}
