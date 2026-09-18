use std::sync::{Arc, mpsc};

use crate::task::Task;

pub struct Runtime {
    ready_queue: mpsc::Receiver<Arc<Task>>,
    sender: mpsc::Sender<Arc<Task>>,
}

impl Runtime {
    pub fn new() -> Self {
        let (sender, ready_queue) = mpsc::channel();

        Self {
            ready_queue,
            sender,
        }
    }

    pub fn spawn<F>(&self, fut: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Task::spawn(fut, &self.sender);
    }

    pub fn run(self) {
        drop(self.sender);
        while let Ok(task) = self.ready_queue.recv() {
            task.poll_once();
        }
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
            Mutex,
            atomic::{AtomicUsize, Ordering},
            mpsc::channel,
        },
        task::{Poll, Waker},
        thread,
        time::Duration,
    };

    use super::*;

    #[test]
    fn spawned_task_is_initially_ready_but_not_repolled_without_wake() {
        let polls = Arc::new(AtomicUsize::new(0));
        let mut runtime = Runtime::default();

        runtime.spawn({
            let polls = Arc::clone(&polls);

            poll_fn(move |_| {
                polls.fetch_add(1, Ordering::SeqCst);
                Poll::<()>::Pending
            })
        });

        assert_eq!(run_until_stalled(&mut runtime), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 1);

        assert_eq!(run_until_stalled(&mut runtime), 0);
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn waking_idle_task_make_it_ready_again() {
        let polls = Arc::new(AtomicUsize::new(0));
        let stored_waker = Arc::new(Mutex::new(None::<Waker>));
        let mut runtime = Runtime::default();

        runtime.spawn({
            let polls = Arc::clone(&polls);
            let stored_waker = Arc::clone(&stored_waker);

            poll_fn(move |cx| {
                let poll = polls.fetch_add(1, Ordering::SeqCst);

                if poll == 0 {
                    *stored_waker.lock().unwrap() = Some(cx.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        });

        assert_eq!(run_until_stalled(&mut runtime), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 1);

        stored_waker.lock().unwrap().take().unwrap().wake();

        assert_eq!(run_until_stalled(&mut runtime), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 2);

        assert_eq!(run_until_stalled(&mut runtime), 0);
    }

    #[test]
    fn wake_during_poll_schedules_another_poll() {
        let polls = Arc::new(AtomicUsize::new(0));
        let mut runtime = Runtime::default();

        runtime.spawn({
            let polls = Arc::clone(&polls);

            poll_fn(move |cx| {
                let poll = polls.fetch_add(1, Ordering::SeqCst);

                if poll == 0 {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        });

        assert_eq!(run_until_stalled(&mut runtime), 2);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn multiple_wakes_during_poll_are_collapsed() {
        let polls = Arc::new(AtomicUsize::new(0));
        let mut runtime = Runtime::default();

        runtime.spawn({
            let polls = Arc::clone(&polls);

            poll_fn(move |cx| {
                let poll = polls.fetch_add(1, Ordering::SeqCst);

                if poll == 0 {
                    cx.waker().wake_by_ref();
                    cx.waker().wake_by_ref();
                    cx.waker().wake_by_ref();

                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        });

        assert_eq!(run_until_stalled(&mut runtime), 2);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn multiple_wakes_of_idle_task_are_collapsed() {
        let polls = Arc::new(AtomicUsize::new(0));
        let stored_waker = Arc::new(Mutex::new(None::<Waker>));
        let mut runtime = Runtime::default();

        runtime.spawn({
            let polls = Arc::clone(&polls);
            let stored_waker = Arc::clone(&stored_waker);

            poll_fn(move |cx| {
                polls.fetch_add(1, Ordering::SeqCst);

                *stored_waker.lock().unwrap() = Some(cx.waker().clone());

                Poll::Pending
            })
        });

        assert_eq!(run_until_stalled(&mut runtime), 1);

        let waker = stored_waker.lock().unwrap().clone().unwrap();

        waker.wake_by_ref();
        waker.wake_by_ref();
        waker.wake_by_ref();

        assert_eq!(run_until_stalled(&mut runtime), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn waking_completed_task_does_not_poll_it_again() {
        let polls = Arc::new(AtomicUsize::new(0));
        let stored_waker = Arc::new(Mutex::new(None::<Waker>));
        let mut runtime = Runtime::default();

        runtime.spawn({
            let polls = Arc::clone(&polls);
            let stored_waker = Arc::clone(&stored_waker);

            poll_fn(move |cx| {
                let poll = polls.fetch_add(1, Ordering::SeqCst);

                if poll == 0 {
                    *stored_waker.lock().unwrap() = Some(cx.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        });

        assert_eq!(run_until_stalled(&mut runtime), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 1);

        let waker = stored_waker.lock().unwrap().clone().unwrap();

        waker.wake_by_ref();

        assert_eq!(run_until_stalled(&mut runtime), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 2);

        waker.wake_by_ref();

        assert_eq!(run_until_stalled(&mut runtime), 0);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn run_waits_for_pending_task_and_resumes_after_wake() {
        let polls = Arc::new(AtomicUsize::new(0));
        let runtime = Runtime::default();

        let (first_poll_tx, first_poll_rx) = channel::<Waker>();
        let (return_tx, return_rx) = channel::<()>();

        runtime.spawn({
            let polls = Arc::clone(&polls);
            let first_poll_tx = first_poll_tx.clone();

            poll_fn(move |cx| {
                let poll = polls.fetch_add(1, Ordering::SeqCst);

                if poll == 0 {
                    let _ = first_poll_tx.send(cx.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        });

        drop(first_poll_tx);

        let thread_tx = return_tx.clone();

        let handle = std::thread::spawn(move || {
            runtime.run();
            let _ = thread_tx.send(());
        });

        drop(return_tx);

        let waker = first_poll_rx.recv().unwrap();

        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert_eq!(
            return_rx.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        );

        waker.wake();

        assert_eq!(return_rx.recv_timeout(Duration::from_millis(100)), Ok(()));
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        handle.join().unwrap();
    }

    #[test]
    fn ready_tasks_are_polled_in_fifo_order() {
        // WARN: this might go away once we implement multi-threaded system
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut runtime = Runtime::default();

        runtime.spawn({
            let trace = Arc::clone(&trace);
            let mut remaining = 1;

            poll_fn(move |cx| {
                trace.lock().unwrap().push('A');

                if remaining == 0 {
                    Poll::Ready(())
                } else {
                    remaining -= 1;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
        });

        runtime.spawn({
            let trace = Arc::clone(&trace);
            let mut remaining = 2;

            poll_fn(move |cx| {
                trace.lock().unwrap().push('B');

                if remaining == 0 {
                    Poll::Ready(())
                } else {
                    remaining -= 1;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
        });

        assert_eq!(run_until_stalled(&mut runtime), 5);
        assert_eq!(trace.lock().unwrap().as_slice(), &['A', 'B', 'A', 'B', 'B']);
    }

    #[test]
    fn async_task_resumes_after_awaited_future_completes() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut runtime = Runtime::default();

        runtime.spawn({
            let trace = Arc::clone(&trace);

            async move {
                trace.lock().unwrap().push("outer:start");

                let mut remaining = 1;

                poll_fn(|cx| {
                    trace.lock().unwrap().push("inner");

                    if remaining == 0 {
                        Poll::Ready(())
                    } else {
                        remaining -= 1;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                })
                .await;

                trace.lock().unwrap().push("outer:end");
            }
        });

        assert_eq!(run_until_stalled(&mut runtime), 2);
        assert_eq!(
            trace.lock().unwrap().as_slice(),
            &["outer:start", "inner", "inner", "outer:end"]
        );
    }

    fn run_until_stalled(runtime: &mut Runtime) -> usize {
        let mut executed = 0;

        while let Ok(task) = runtime.ready_queue.try_recv() {
            task.poll_once();
            executed += 1;
        }

        executed
    }
}
