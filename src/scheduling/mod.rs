mod state;

use std::{
    sync::{Arc, Mutex, mpsc},
    task::{Poll, Wake, Waker},
};

use crate::task::TaskId;
use state::{ScheduleAction, ScheduleState};

pub(super) struct TaskSchedule {
    id: TaskId,
    state: Mutex<ScheduleState>,
    ready_tx: mpsc::Sender<TaskId>,
}

impl TaskSchedule {
    pub(super) fn new(id: TaskId, ready_tx: mpsc::Sender<TaskId>) -> Arc<Self> {
        Arc::new(Self {
            id,
            state: Mutex::new(ScheduleState::Idle),
            ready_tx,
        })
    }

    pub(super) fn request_schedule(&self) {
        let mut state = self.state.lock().unwrap();
        let action = state.wake();
        drop(state);

        if let ScheduleAction::Enqueue = action {
            let _ = self.ready_tx.send(self.id);
        }
    }

    pub(super) fn begin_poll(&self) {
        self.state.lock().unwrap().begin_poll();
    }

    pub(super) fn finish_poll(&self, poll_result: Poll<()>) -> TaskPollOutcome {
        let mut state = self.state.lock().unwrap();

        let action = state.finish_poll(poll_result);
        drop(state);

        match action {
            ScheduleAction::None => TaskPollOutcome::Pending,
            ScheduleAction::Enqueue => {
                let _ = self.ready_tx.send(self.id);
                TaskPollOutcome::Pending
            }
            ScheduleAction::Complete => TaskPollOutcome::Complete,
        }
    }

    pub(super) fn waker(self: &Arc<Self>) -> Waker {
        Waker::from(self.clone())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum TaskPollOutcome {
    Pending,
    Complete,
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
    use std::sync::mpsc::{TryRecvError, channel};

    use super::*;

    #[test]
    fn new_schedule_is_initially_idle() {
        let (ready_tx, ready_rx) = channel();
        let task_id = TaskId::new();

        let _schedule = TaskSchedule::new(task_id, ready_tx);

        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn request_schedule_enqueues_idle_task_once() {
        let (ready_tx, ready_rx) = channel();
        let task_id = TaskId::new();

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
        let task_id = TaskId::new();

        let schedule = TaskSchedule::new(task_id, ready_tx);

        schedule.request_schedule();

        assert_eq!(ready_rx.try_recv(), Ok(task_id));

        schedule.begin_poll();
        let outcome = schedule.finish_poll(Poll::Pending);

        assert_eq!(outcome, TaskPollOutcome::Pending);
        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));

        schedule.waker().wake();

        assert_eq!(ready_rx.try_recv(), Ok(task_id));
    }

    #[test]
    fn wake_during_poll_requeues_pending_task_once() {
        let (ready_tx, ready_rx) = channel();
        let task_id = TaskId::new();

        let schedule = TaskSchedule::new(task_id, ready_tx);

        schedule.request_schedule();

        assert_eq!(ready_rx.try_recv(), Ok(task_id));

        schedule.begin_poll();
        let waker = schedule.waker();

        waker.wake_by_ref();
        waker.wake_by_ref();
        waker.wake_by_ref();

        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));

        let outcome = schedule.finish_poll(Poll::Pending);
        assert_eq!(outcome, TaskPollOutcome::Pending);
        assert_eq!(ready_rx.try_recv(), Ok(task_id));
        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn completed_task_is_not_requeued() {
        let (ready_tx, ready_rx) = channel();
        let task_id = TaskId::new();

        let schedule = TaskSchedule::new(task_id, ready_tx);

        schedule.request_schedule();

        assert_eq!(ready_rx.try_recv(), Ok(task_id));

        schedule.begin_poll();
        let waker = schedule.waker();

        waker.wake_by_ref();

        let outcome = schedule.finish_poll(Poll::Ready(()));

        assert_eq!(outcome, TaskPollOutcome::Complete);
        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));

        waker.wake_by_ref();
        assert_eq!(ready_rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn wake_after_ready_queue_is_dropped_is_harmless() {
        let (ready_tx, ready_rx) = channel();
        let task_id = TaskId::new();

        let schedule = TaskSchedule::new(task_id, ready_tx);
        let waker = schedule.waker();

        drop(ready_rx);

        waker.wake();
    }
}
