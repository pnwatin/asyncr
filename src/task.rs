use std::{num::NonZeroU64, pin::Pin, sync::Arc};

use crate::{Handle, JoinHandle, scheduling::TaskSchedule};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Id(u64);

pub(crate) const ROOT_TASK_ID: Id = Id(0);

impl Id {
    pub(crate) fn new(integer: impl Into<NonZeroU64>) -> Self {
        Self(integer.into().into())
    }
}

pub(crate) type TaskFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

pub(crate) struct LocalTask {
    pub(crate) id: Id,
    pub(crate) future: TaskFuture,
    pub(crate) schedule: Arc<TaskSchedule>,
}

#[track_caller]
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
{
    let handle = Handle::current();
    handle.spawn(future)
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use crate::Runtime;

    use super::*;

    #[test]
    fn spawn_uses_current_runtime_and_returns_task_output() {
        let mut runtime = Runtime::new();

        let output = runtime.block_on(async { spawn(async { 42 }).await.unwrap() });

        assert_eq!(output, 42);
    }

    #[test]
    #[should_panic(expected = "no asyncr runtime is currently entered on this thread")]
    fn spawn_requires_an_entered_runtime() {
        spawn(async {});
    }

    #[test]
    fn spawn_accepts_non_send_future_and_output() {
        let mut runtime = Runtime::new();

        let output = runtime.block_on(async {
            let value = Rc::new(Cell::new(42));

            spawn(async move { value }).await.unwrap()
        });

        assert_eq!(output.get(), 42);
    }
}
