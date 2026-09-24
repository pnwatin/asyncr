use std::rc::Rc;

use crate::{
    catch::CatchUnwind,
    executor::SpawnState,
    join::{JoinError, JoinHandle, join_pair},
};

#[derive(Clone)]
pub struct Handle {
    pub(crate) spawn_state: Rc<SpawnState>,
}

impl Handle {
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
}
