use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Waker},
};

type Task = Pin<Box<dyn Future<Output = ()>>>;

#[derive(Default)]
pub struct Runtime {
    tasks: VecDeque<Task>,
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn<F>(&mut self, fut: F)
    where
        F: Future<Output = ()> + 'static,
    {
        self.tasks.push_back(Box::pin(fut));
    }

    pub fn run(&mut self) {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);

        while let Some(mut task) = self.tasks.pop_front() {
            if task.as_mut().poll(&mut cx).is_pending() {
                self.tasks.push_back(task);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc, task::Poll};

    use super::*;

    #[test]
    fn runtime_polls_pending_tasks_in_fifo_order_until_complete() {
        let shared_trace = SharedTrace::default();

        let mut runtime = Runtime::new();

        runtime.spawn(TraceFut::new("A", shared_trace.clone(), 1));
        runtime.spawn(TraceFut::new("B", shared_trace.clone(), 2));

        runtime.run();

        assert_eq!(shared_trace.borrow().as_slice(), &["A", "B", "A", "B", "B"]);
    }

    #[test]
    fn async_task_resumes_after_awaited_future_completes() {
        let shared_trace = SharedTrace::default();

        let mut runtime = Runtime::new();

        let trace = shared_trace.clone();

        runtime.spawn(async move {
            record(&trace, "outer:start");
            let fut = TraceFut::new("A", trace.clone(), 1);

            fut.await;
            record(&trace, "outer:end");
        });

        runtime.run();

        assert_eq!(
            shared_trace.borrow().as_slice(),
            &["outer:start", "A", "A", "outer:end"]
        );
    }

    type SharedTrace = Rc<RefCell<Vec<&'static str>>>;

    fn record(trace: &SharedTrace, event: &'static str) {
        trace.borrow_mut().push(event);
    }

    struct TraceFut {
        label: &'static str,
        shared_trace: SharedTrace,
        completed: bool,
        pending_polls_remaining: u32,
    }

    impl Future for TraceFut {
        type Output = ();
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> std::task::Poll<Self::Output> {
            let this = self.get_mut();

            assert!(!this.completed, "a completed future was polled again");

            record(&this.shared_trace, this.label);

            if this.pending_polls_remaining == 0 {
                this.completed = true;
                return Poll::Ready(());
            }

            this.pending_polls_remaining -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    impl TraceFut {
        fn new(
            label: &'static str,
            shared_trace: SharedTrace,
            pending_polls_remaining: u32,
        ) -> Self {
            Self {
                label,
                shared_trace,
                pending_polls_remaining,
                completed: false,
            }
        }
    }
}
