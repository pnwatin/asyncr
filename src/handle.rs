use std::{marker::PhantomData, rc::Rc};

use crate::{
    catch::CatchUnwind,
    context,
    executor::SpawnState,
    join::{JoinError, JoinHandle, join_pair},
    time::driver::TimerHandle,
};

#[derive(Clone)]
pub struct Handle {
    pub(crate) spawn_state: Rc<SpawnState>,
    pub(crate) timer_handle: TimerHandle,
}

impl Handle {
    pub fn try_current() -> Option<Self> {
        context::try_current()
    }

    #[track_caller]
    pub fn current() -> Self {
        context::current()
    }

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
    {
        let (sender, handle) = join_pair();

        let task_future = async move {
            let result = match CatchUnwind::new(future).await {
                Ok(output) => Ok(output),
                Err(_panic) => Err(JoinError::Panic),
            };

            sender.publish(result);
        };

        self.spawn_state.submit(task_future);

        handle
    }

    pub fn enter(&self) -> EnterGuard {
        context::try_set_current(self).expect(
            "cannot enter an asyncr runtime while another runtime is active on this thread",
        );

        EnterGuard {
            _not_send: PhantomData,
        }
    }
}

#[must_use = "the runtime context remains entered only while this guard is alive"]
pub struct EnterGuard {
    _not_send: PhantomData<Rc<()>>,
}

impl Drop for EnterGuard {
    fn drop(&mut self) {
        context::clear_current();
    }
}
