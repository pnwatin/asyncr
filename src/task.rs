use std::{
    pin::Pin,
    sync::{Arc, Mutex, mpsc},
    task::{Context, Poll, Wake, Waker},
};

type TaskFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

pub(crate) struct Task {
    state: TaskStateCell,
    ready_tx: mpsc::Sender<Arc<Task>>,
}

struct TaskStateCell {
    inner: Mutex<Option<TaskState>>,
}

enum TaskState {
    Idle(TaskFuture),
    Queued(TaskFuture),
    Running(RunningState),
    Complete,
}

enum RunningState {
    Unnotified,
    Notified,
}

enum Action {
    None,
    Enqueue,
}

impl TaskState {
    fn wake(self) -> (Self, Action) {
        match self {
            Self::Idle(future) => (TaskState::Queued(future), Action::Enqueue),
            Self::Running(RunningState::Unnotified) => {
                (TaskState::Running(RunningState::Notified), Action::None)
            }
            state => (state, Action::None),
        }
    }

    fn begin_poll(self) -> (Self, TaskFuture) {
        match self {
            TaskState::Queued(future) => (TaskState::Running(RunningState::Unnotified), future),
            _ => unreachable!("only a queued task may be polled"),
        }
    }

    fn finish_poll(self, future: TaskFuture, result: Poll<()>) -> (Self, Action) {
        match (self, result) {
            (TaskState::Running(_), Poll::Ready(())) => (TaskState::Complete, Action::None),

            (TaskState::Running(RunningState::Unnotified), Poll::Pending) => {
                (TaskState::Idle(future), Action::None)
            }

            (TaskState::Running(RunningState::Notified), Poll::Pending) => {
                (TaskState::Queued(future), Action::Enqueue)
            }

            _ => unreachable!("task must be running when poll finishes"),
        }
    }
}

impl TaskStateCell {
    fn new(state: TaskState) -> Self {
        Self {
            inner: Mutex::new(Some(state)),
        }
    }

    fn transition<R>(&self, f: impl FnOnce(TaskState) -> (TaskState, R)) -> R {
        let mut slot = self.inner.lock().unwrap();

        let state = slot
            .take()
            .expect("task state must exist while mutex is locked");

        let (state, result) = f(state);

        *slot = Some(state);

        result
    }
}

impl Task {
    pub(crate) fn spawn<F>(fut: F, sender: &mpsc::Sender<Arc<Task>>)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task = Arc::new(Task {
            state: TaskStateCell::new(TaskState::Idle(Box::pin(fut))),
            ready_tx: sender.clone(),
        });

        task.schedule();
    }

    pub(crate) fn poll_once(self: Arc<Self>) {
        let mut future = self.state.transition(TaskState::begin_poll);

        let waker = Waker::from(self.clone());
        let mut cx = Context::from_waker(&waker);

        let res = future.as_mut().poll(&mut cx);

        let action = self
            .state
            .transition(|state| state.finish_poll(future, res));

        self.perform_action(action);
    }

    fn schedule(self: &Arc<Self>) {
        let action = self.state.transition(TaskState::wake);

        self.perform_action(action);
    }

    fn perform_action(self: &Arc<Self>, action: Action) {
        match action {
            Action::None => {}

            Action::Enqueue => {
                let _ = self.ready_tx.send(Arc::clone(self));
            }
        }
    }
}

impl Wake for Task {
    fn wake_by_ref(self: &Arc<Self>) {
        self.schedule();
    }

    fn wake(self: Arc<Self>) {
        self.schedule();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_future() -> TaskFuture {
        Box::pin(async {})
    }

    #[test]
    fn waking_idle_task_queues_it() {
        let state = TaskState::Idle(dummy_future());

        let (state, action) = state.wake();

        assert!(matches!(state, TaskState::Queued(_)));
        assert!(matches!(action, Action::Enqueue));
    }

    #[test]
    fn waking_queued_task_is_a_noop() {
        let state = TaskState::Queued(dummy_future());

        let (state, action) = state.wake();

        assert!(matches!(state, TaskState::Queued(_)));
        assert!(matches!(action, Action::None));
    }

    #[test]
    fn waking_running_task_marks_it_notified() {
        let state = TaskState::Running(RunningState::Unnotified);

        let (state, action) = state.wake();

        assert!(matches!(state, TaskState::Running(RunningState::Notified)));
        assert!(matches!(action, Action::None));
    }

    #[test]
    fn repeated_wake_while_running_is_coalesced() {
        let state = TaskState::Running(RunningState::Notified);

        let (state, action) = state.wake();

        assert!(matches!(state, TaskState::Running(RunningState::Notified)));
        assert!(matches!(action, Action::None));
    }

    #[test]
    fn polling_queued_task_moves_it_to_running() {
        let state = TaskState::Queued(dummy_future());

        let (state, _future) = state.begin_poll();

        assert!(matches!(
            state,
            TaskState::Running(RunningState::Unnotified)
        ));
    }

    #[test]
    fn pending_unnotified_task_becomes_idle() {
        let state = TaskState::Running(RunningState::Unnotified);

        let (state, action) = state.finish_poll(dummy_future(), Poll::Pending);

        assert!(matches!(state, TaskState::Idle(_)));
        assert!(matches!(action, Action::None));
    }

    #[test]
    fn pending_notified_task_is_requeued() {
        let state = TaskState::Running(RunningState::Notified);

        let (state, action) = state.finish_poll(dummy_future(), Poll::Pending);

        assert!(matches!(state, TaskState::Queued(_)));
        assert!(matches!(action, Action::Enqueue));
    }

    #[test]
    fn ready_task_becomes_complete() {
        let state = TaskState::Running(RunningState::Unnotified);

        let (state, action) = state.finish_poll(dummy_future(), Poll::Ready(()));

        assert!(matches!(state, TaskState::Complete));
        assert!(matches!(action, Action::None));
    }
}
