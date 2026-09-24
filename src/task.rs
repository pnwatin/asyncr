use std::{pin::Pin, sync::Arc};

use uuid::Uuid;

use crate::scheduling::TaskSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct TaskId(Uuid);

pub(crate) const ROOT_TASK_ID: TaskId = TaskId(Uuid::nil());

impl TaskId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

pub(crate) type TaskFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

pub(crate) struct LocalTask {
    pub(crate) id: TaskId,
    pub(crate) future: TaskFuture,
    pub(crate) schedule: Arc<TaskSchedule>,
}
