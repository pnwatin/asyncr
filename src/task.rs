use std::{num::NonZeroU64, pin::Pin, sync::Arc};

use crate::scheduling::TaskSchedule;

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
