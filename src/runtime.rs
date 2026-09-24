use crate::{executor::Executor, handle::Handle};

pub struct Runtime {
    executor: Executor,
    handle: Handle,
}

impl Runtime {
    pub fn new() -> Self {
        let (executor, spawn_state) = Executor::new();
        let handle = Handle { spawn_state };

        Self { executor, handle }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        self.executor.block_on(future)
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::poll_fn,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc::channel,
        },
        task::Poll,
        thread,
    };

    use super::*;

    #[test]
    fn block_on_returns_root_output() {
        let mut runtime = Runtime::default();

        let result = runtime.block_on(async { 42 });

        assert_eq!(result, 42);
    }

    #[test]
    fn block_on_repolls_root_after_wake_during_poll() {
        let polls = Arc::new(AtomicUsize::new(0));
        let mut runtime = Runtime::default();

        let root = {
            let polls = Arc::clone(&polls);

            poll_fn(move |cx| {
                if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(42)
                }
            })
        };

        let result = runtime.block_on(root);

        assert_eq!(result, 42);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn block_on_waits_for_external_root_wake() {
        let (waker_tx, waker_rx) = channel();
        let polls = Arc::new(AtomicUsize::new(0));
        let mut runtime = Runtime::default();

        let root = {
            let polls = Arc::clone(&polls);

            poll_fn(move |cx| {
                if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                    let _ = waker_tx.send(cx.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(42)
                }
            })
        };

        let helper = thread::spawn(move || match waker_rx.recv() {
            Ok(waker) => waker.wake(),
            _ => {
                unreachable!("waker_tx should not be dropped at this point");
            }
        });

        let result = runtime.block_on(root);

        helper.join().unwrap();

        assert_eq!(result, 42);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn block_on_drives_spawned_task_joined_by_root() {
        let mut runtime = Runtime::default();
        let handle = runtime.handle();
        let child = handle.spawn(async { 42 });

        let result = runtime.block_on(async { child.await.unwrap() });

        assert_eq!(result, 42);
    }

    #[test]
    fn block_on_root_can_spawn_and_join_child() {
        let mut runtime = Runtime::default();
        let shared_trace = Arc::new(Mutex::new(Vec::new()));

        let root = {
            let handle = runtime.handle();
            let shared_trace = Arc::clone(&shared_trace);

            async move {
                shared_trace.lock().unwrap().push("parent:start");
                let child = handle.spawn({
                    let shared_trace = Arc::clone(&shared_trace);

                    async move {
                        shared_trace.lock().unwrap().push("child");
                        42
                    }
                });

                let x = child.await.unwrap();
                shared_trace.lock().unwrap().push("parent:end");

                x
            }
        };

        let result = runtime.block_on(root);

        assert_eq!(result, 42);
        assert_eq!(
            shared_trace.lock().unwrap().as_slice(),
            &["parent:start", "child", "parent:end"]
        );
    }

    #[test]
    fn block_on_accepts_borrowing_root_future() {
        let mut runtime = Runtime::default();
        let mut counter = 0;
        let s = String::from("hello");

        let result = runtime.block_on(async {
            counter += 1;

            &s
        });

        assert_eq!(result, "hello");
        assert_eq!(counter, 1);
    }

    #[test]
    fn runtime_can_block_on_more_than_once() {
        let mut runtime = Runtime::default();

        assert_eq!(runtime.block_on(async { 42 }), 42);
        assert_eq!(runtime.block_on(async { 67 }), 67);
    }

    #[test]
    fn detached_task_resumes_on_later_block_on() {
        let spawned_complete = Arc::new(AtomicBool::new(false));
        let mut runtime = Runtime::default();

        let root = {
            let handle = runtime.handle();
            let spawned_complete = Arc::clone(&spawned_complete);

            async move {
                handle.spawn({
                    async move {
                        spawned_complete.store(true, Ordering::SeqCst);
                    }
                });

                42
            }
        };

        let result = runtime.block_on(root);

        assert_eq!(result, 42);
        assert!(!spawned_complete.load(Ordering::SeqCst));

        runtime.block_on(async {});

        assert!(spawned_complete.load(Ordering::SeqCst));
    }
}
