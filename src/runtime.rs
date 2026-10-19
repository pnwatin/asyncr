use crate::{EnterGuard, executor::Executor, handle::Handle};

#[cfg(test)]
use crate::time::clock::Clock;

pub struct Runtime {
    executor: Executor,
    handle: Handle,
}

impl Runtime {
    pub fn new() -> Self {
        let (executor, handle) = Executor::new();

        Self { executor, handle }
    }

    #[cfg(test)]
    pub(crate) fn with_clock(clock: Clock) -> Self {
        let (executor, handle) = Executor::with_clock(clock);

        Self { executor, handle }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        let _guard = self.enter();
        self.executor.block_on(future)
    }

    pub fn enter(&self) -> EnterGuard {
        self.handle.enter()
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
        cell::{Cell, RefCell},
        future::{pending, poll_fn},
        panic::{AssertUnwindSafe, catch_unwind},
        pin::pin,
        rc::Rc,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc::channel,
        },
        task::Poll,
        thread,
        time::Duration,
    };

    use crate::time::{Elapsed, interval, sleep, timeout};

    use super::*;

    #[test]
    fn enter_makes_runtime_handle_current_until_guard_is_dropped() {
        let runtime = Runtime::new();
        let expected = runtime.handle();

        let guard = runtime.enter();

        let current = Handle::current();
        assert!(Rc::ptr_eq(&current.spawn_state, &expected.spawn_state));

        drop(guard);

        assert!(Handle::try_current().is_none());
    }

    #[test]
    fn block_on_enters_runtime_context_for_root_future() {
        let mut runtime = Runtime::new();
        let expected = runtime.handle();

        assert!(Handle::try_current().is_none());

        let saw_expected_runtime = runtime.block_on(async {
            let current = Handle::current();
            Rc::ptr_eq(&current.spawn_state, &expected.spawn_state)
        });

        assert!(saw_expected_runtime);
        assert!(Handle::try_current().is_none());
    }

    #[test]
    fn block_on_enters_runtime_context_for_spawned_tasks() {
        let mut runtime = Runtime::new();
        let handle = runtime.handle();
        let expected = handle.clone();

        let saw_expected_runtime = runtime.block_on(async move {
            handle
                .spawn(async move {
                    let current = Handle::current();
                    Rc::ptr_eq(&current.spawn_state, &expected.spawn_state)
                })
                .await
                .unwrap()
        });

        assert!(saw_expected_runtime);
    }

    #[test]
    fn block_on_clears_runtime_context_after_root_panic() {
        let mut runtime = Runtime::new();

        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.block_on(async {
                assert!(Handle::try_current().is_some());
                panic!("root future panicked");
            });
        }));

        assert!(result.is_err());
        assert!(Handle::try_current().is_none());
        assert_eq!(runtime.block_on(async { 42 }), 42);
    }

    #[test]
    fn block_on_rejects_an_already_entered_context_without_replacing_it() {
        let outer_runtime = Runtime::new();
        let outer_handle = outer_runtime.handle();
        let _outer_guard = outer_runtime.enter();
        let mut inner_runtime = Runtime::new();

        let result = catch_unwind(AssertUnwindSafe(|| inner_runtime.block_on(async {})));

        assert!(result.is_err());
        let current = Handle::current();
        assert!(Rc::ptr_eq(&current.spawn_state, &outer_handle.spawn_state));
    }

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
    fn paused_runtime_auto_advances_for_root_sleep() {
        let clock = Clock::new_paused();
        let start = clock.now();
        let mut runtime = Runtime::with_clock(clock.clone());
        let polls = Cell::new(0);
        let guard = runtime.enter();
        let mut sleep = pin!(sleep(Duration::from_secs(60 * 60)));
        drop(guard);

        let output = runtime.block_on(poll_fn(|cx| {
            polls.set(polls.get() + 1);
            sleep.as_mut().poll(cx).map(|()| 42)
        }));

        assert_eq!(output, 42);
        assert_eq!(polls.get(), 2);
        assert_eq!(clock.now(), start + Duration::from_secs(60 * 60));
    }

    #[test]
    fn paused_runtime_drives_sleeping_spawned_task() {
        let clock = Clock::new_paused();
        let start = clock.now();
        let mut runtime = Runtime::with_clock(clock.clone());
        let handle = runtime.handle();

        let child = handle.spawn(async move {
            sleep(Duration::from_secs(5)).await;
            42
        });

        let output = runtime.block_on(async { child.await.unwrap() });

        assert_eq!(output, 42);
        assert_eq!(clock.now(), start + Duration::from_secs(5));
    }

    #[test]
    fn paused_runtime_drives_timeout_through_public_api() {
        let clock = Clock::new_paused();
        let start = clock.now();
        let mut runtime = Runtime::with_clock(clock.clone());

        let result =
            runtime.block_on(async { timeout(Duration::from_secs(5), pending::<()>()).await });

        assert_eq!(result, Err(Elapsed));
        assert_eq!(clock.now(), start + Duration::from_secs(5));
    }

    #[test]
    fn paused_runtime_drives_interval_through_public_api() {
        let clock = Clock::new_paused();
        let start = clock.now();
        let mut runtime = Runtime::with_clock(clock.clone());

        let ticks = runtime.block_on(async {
            let mut interval = interval(Duration::from_secs(5));
            let first = interval.tick().await;
            let second = interval.tick().await;

            (first, second)
        });

        assert_eq!(ticks, (start, start + Duration::from_secs(5)));
        assert_eq!(clock.now(), start + Duration::from_secs(5));
    }

    #[test]
    fn running_runtime_drives_sleep_through_recv_timeout() {
        let mut runtime = Runtime::new();

        runtime.block_on(async {
            sleep(Duration::from_millis(1)).await;
        });
    }

    #[test]
    fn ready_task_can_insert_earlier_deadline_before_auto_advance() {
        let clock = Clock::new_paused();
        let start = clock.now();
        let mut runtime = Runtime::with_clock(clock.clone());
        let handle = runtime.handle();
        let trace = Rc::new(RefCell::new(Vec::new()));

        let root = {
            let child_trace = Rc::clone(&trace);
            let root_trace = Rc::clone(&trace);

            async move {
                handle.spawn(async move {
                    sleep(Duration::from_secs(5)).await;
                    child_trace.borrow_mut().push("short");
                });

                sleep(Duration::from_secs(10)).await;
                root_trace.borrow_mut().push("long");
            }
        };

        runtime.block_on(root);

        assert_eq!(trace.borrow().as_slice(), &["short", "long"]);
        assert_eq!(clock.now(), start + Duration::from_secs(10));
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
