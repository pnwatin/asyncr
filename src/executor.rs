use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    num::NonZeroU64,
    pin::pin,
    rc::Rc,
    sync::mpsc::{self, RecvError, channel},
    task::{Context, Poll, Waker},
};

use crate::{
    scheduling::{TaskPollOutcome, TaskSchedule},
    task::{Id, LocalTask, ROOT_TASK_ID},
};

pub(crate) struct Executor {
    tasks: HashMap<Id, LocalTask>,
    spawn_state: Rc<SpawnState>,
    ready_rx: mpsc::Receiver<Id>,
}

impl Executor {
    pub(crate) fn new() -> (Self, Rc<SpawnState>) {
        let (ready_tx, ready_rx) = channel();
        let spawn_state = Rc::new(SpawnState {
            ready_tx,
            pending_spawns: Default::default(),
            next_id: Cell::new(1),
            closed: Cell::new(false),
        });

        let executor = Self {
            tasks: Default::default(),
            spawn_state: Rc::clone(&spawn_state),
            ready_rx,
        };

        (executor, spawn_state)
    }

    pub(crate) fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        let mut future = pin!(future);

        let schedule = TaskSchedule::new(ROOT_TASK_ID, self.spawn_state.ready_tx.clone());

        let waker = Waker::from(schedule.clone());
        let mut context = Context::from_waker(&waker);

        schedule.request_schedule();

        loop {
            self.drain_pending_spawns();

            match self.ready_rx.recv() {
                Ok(ROOT_TASK_ID) => {
                    schedule.begin_poll();

                    match future.as_mut().poll(&mut context) {
                        Poll::Pending => {
                            schedule.finish_poll(Poll::Pending);
                        }
                        Poll::Ready(output) => {
                            schedule.finish_poll(Poll::Ready(()));
                            return output;
                        }
                    }
                }
                Ok(id) => {
                    self.poll_task(id);
                }
                Err(RecvError) => {
                    unreachable!(
                        "the executor's spawn state owns a sender while the executor is alive"
                    );
                }
            }
        }
    }

    fn poll_task(&mut self, id: Id) {
        let task = self
            .tasks
            .get_mut(&id)
            .expect("ready queue contained a task that was not admitted to the executor");

        let schedule = task.schedule.clone();

        schedule.begin_poll();

        let waker = schedule.waker();
        let mut cx = Context::from_waker(&waker);
        let result = task.future.as_mut().poll(&mut cx);

        if let TaskPollOutcome::Complete = schedule.finish_poll(result) {
            self.tasks.remove(&id);
        }
    }

    fn drain_pending_spawns(&mut self) {
        let mut pending = self.spawn_state.pending_spawns.borrow_mut();

        self.tasks.extend(pending.drain(..).map(|t| (t.id, t)));
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.spawn_state.closed.set(true);
        self.spawn_state.pending_spawns.borrow_mut().clear();
    }
}

pub(crate) struct SpawnState {
    pending_spawns: RefCell<VecDeque<LocalTask>>,
    ready_tx: mpsc::Sender<Id>,
    next_id: Cell<u64>,
    closed: Cell<bool>,
}

impl SpawnState {
    pub(crate) fn submit<F>(&self, future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        if self.closed.get() {
            return;
        }

        let task_id = self.next_id();
        let schedule = TaskSchedule::new(task_id, self.ready_tx.clone());

        let local_task = LocalTask {
            id: task_id,
            future: Box::pin(future),
            schedule: schedule.clone(),
        };

        self.pending_spawns.borrow_mut().push_back(local_task);

        schedule.request_schedule();
    }

    fn next_id(&self) -> Id {
        loop {
            let id = self.next_id.replace(self.next_id.get().wrapping_add(1));

            if let Some(id) = NonZeroU64::new(id) {
                return Id::new(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        future::poll_fn,
        pin::pin,
        rc::Rc,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc::TryRecvError,
        },
        task::Poll,
    };

    use crate::{Handle, JoinError, executor::Executor, test_utils::noop_context};

    #[test]
    fn poll_one_ready_task_returns_false_when_idle() {
        let (mut executor, _) = Executor::new();

        assert!(!executor.poll_one_ready_task());
    }

    #[test]
    fn poll_one_ready_task_runs_and_removes_completed_task() {
        let (mut executor, spawn_state) = Executor::new();
        let has_run = Arc::new(AtomicBool::new(false));

        let future = {
            let has_run = Arc::clone(&has_run);

            async move {
                has_run.store(true, Ordering::SeqCst);
            }
        };

        spawn_state.submit(future);

        assert!(executor.poll_one_ready_task());
        assert!(has_run.load(Ordering::SeqCst));
        assert!(executor.tasks.is_empty());

        assert!(!executor.poll_one_ready_task());
    }

    #[test]
    fn pending_task_is_not_polled_again_without_wake() {
        let (mut executor, spawn_state) = Executor::new();
        let polls = Arc::new(AtomicUsize::new(0));

        let future = {
            let polls = Arc::clone(&polls);

            poll_fn(move |_| {
                polls.fetch_add(1, Ordering::SeqCst);

                Poll::<()>::Pending
            })
        };

        spawn_state.submit(future);

        assert!(executor.poll_one_ready_task());
        assert_eq!(polls.load(Ordering::SeqCst), 1);

        assert!(!executor.poll_one_ready_task());
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(!executor.tasks.is_empty());
    }

    #[test]
    fn wake_during_poll_schedules_another_turn() {
        let (mut executor, spawn_state) = Executor::new();
        let polls = Arc::new(AtomicUsize::new(0));

        let future = {
            let polls = Arc::clone(&polls);

            poll_fn(move |cx| {
                if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        };

        spawn_state.submit(future);

        assert!(executor.poll_one_ready_task());
        assert!(executor.poll_one_ready_task());
        assert!(!executor.poll_one_ready_task());

        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn spawned_task_result_reaches_join_handle() {
        let (mut executor, spawn_state) = Executor::new();
        let handle = Handle { spawn_state };

        let mut join = pin!(handle.spawn(async { 42 }));

        assert!(join.as_mut().poll(&mut noop_context()).is_pending());

        assert!(executor.poll_one_ready_task());
        assert_eq!(join.as_mut().poll(&mut noop_context()), Poll::Ready(Ok(42)));
    }

    #[test]
    fn spawned_task_panic_becomes_join_error() {
        let (mut executor, spawn_state) = Executor::new();
        let handle = Handle { spawn_state };

        let mut join = pin!(handle.spawn(async {
            panic!("oups");
        }));

        assert!(join.as_mut().poll(&mut noop_context()).is_pending());

        assert!(executor.poll_one_ready_task());
        assert_eq!(
            join.as_mut().poll(&mut noop_context()),
            Poll::Ready(Err(JoinError::Panic))
        );
    }

    #[test]
    fn dropping_join_handle_does_not_cancel_task() {
        let (mut executor, spawn_state) = Executor::new();
        let handle = Handle { spawn_state };
        let has_run = Arc::new(AtomicBool::new(false));

        let future = {
            let has_run = Arc::clone(&has_run);

            async move {
                has_run.store(true, Ordering::SeqCst);
            }
        };

        handle.spawn(future);

        executor.run_until_stalled();

        assert!(has_run.load(Ordering::SeqCst));
    }

    #[test]
    fn dropping_executor_cancels_active_spawned_task() {
        let (mut executor, spawn_state) = Executor::new();
        let handle = Handle { spawn_state };
        let mut join = pin!(handle.spawn(poll_fn(|_| Poll::<()>::Pending)));

        assert!(executor.poll_one_ready_task());
        assert!(join.as_mut().poll(&mut noop_context()).is_pending());

        drop(executor);

        assert_eq!(
            join.as_mut().poll(&mut noop_context()),
            Poll::Ready(Err(JoinError::Cancelled))
        );
    }

    #[test]
    fn spawn_accepts_non_send_future() {
        let (mut executor, spawn_state) = Executor::new();
        let handle = Handle { spawn_state };
        let not_send_data = Rc::new(Cell::new(true));

        let mut join = pin!(handle.spawn(async move { not_send_data }));

        executor.run_until_stalled();
        let result = join.as_mut().poll(&mut noop_context());

        let Poll::Ready(result) = result else {
            panic!("join should be ready");
        };

        assert!(result.unwrap().take());
    }

    #[test]
    fn running_task_can_spawn_and_join_child() {
        let (mut executor, spawn_state) = Executor::new();
        let handle = Handle { spawn_state };
        let shared_trace = Arc::new(Mutex::new(Vec::new()));

        let parent = {
            let handle = handle.clone();
            let shared_trace = Arc::clone(&shared_trace);

            async move {
                shared_trace.lock().unwrap().push("parent:start");
                let join = handle.spawn({
                    let shared_trace = Arc::clone(&shared_trace);

                    async move {
                        shared_trace.lock().unwrap().push("child");
                        42
                    }
                });

                let x = join.await.unwrap();

                shared_trace.lock().unwrap().push("parent:end");

                x
            }
        };

        let mut join = pin!(handle.spawn(parent));

        executor.run_until_stalled();

        assert_eq!(join.as_mut().poll(&mut noop_context()), Poll::Ready(Ok(42)));
        assert_eq!(
            shared_trace.lock().unwrap().as_slice(),
            &["parent:start", "child", "parent:end"]
        );
    }

    impl Executor {
        fn poll_one_ready_task(&mut self) -> bool {
            self.drain_pending_spawns();

            match self.ready_rx.try_recv() {
                Ok(id) => {
                    self.poll_task(id);

                    true
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => {
                    unreachable!(
                        "the executor's spawn state owns a sender while the executor is alive"
                    );
                }
            }
        }

        fn run_until_stalled(&mut self) -> u32 {
            let mut counter = 0;

            while self.poll_one_ready_task() {
                counter += 1;
            }

            counter
        }
    }

    #[test]
    fn pending_task_is_cancelled_on_executor_drop() {
        let (executor, spawn_state) = Executor::new();
        let handle = Handle { spawn_state };

        let mut join = pin!(handle.spawn(async {}));

        drop(executor);

        assert_eq!(
            join.as_mut().poll(&mut noop_context()),
            Poll::Ready(Err(JoinError::Cancelled))
        );
    }

    #[test]
    fn spawn_after_shutdown_is_immediately_cancelled() {
        let (executor, spawn_state) = Executor::new();
        let handle = Handle { spawn_state };

        drop(executor);

        let mut join = pin!(handle.spawn(async {}));
        assert_eq!(
            join.as_mut().poll(&mut noop_context()),
            Poll::Ready(Err(JoinError::Cancelled))
        );
    }
}
