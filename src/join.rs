use std::{
    cell::RefCell,
    rc::Rc,
    task::{Poll, Waker},
};

pub struct JoinHandle<T> {
    state: Rc<RefCell<JoinState<T>>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum JoinError {
    Cancelled,
    Panic,
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut state = self.state.borrow_mut();

        match &mut *state {
            JoinState::Pending { waker } => {
                *waker = Some(cx.waker().clone());

                Poll::Pending
            }
            JoinState::Ready(_) => {
                let old = std::mem::replace(&mut *state, JoinState::Consumed);

                let JoinState::Ready(result) = old else {
                    unreachable!();
                };

                Poll::Ready(result)
            }
            JoinState::Consumed => {
                unreachable!("JoinHandle should not be polled after completion");
            }
        }
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if let JoinState::Pending { waker } = &mut *self.state.borrow_mut() {
            *waker = None;
        }
    }
}

enum JoinState<T> {
    Pending { waker: Option<Waker> },
    Ready(Result<T, JoinError>),
    Consumed,
}

pub(crate) fn join_pair<T>() -> (JoinSender<T>, JoinHandle<T>) {
    let state = Rc::new(RefCell::new(JoinState::Pending { waker: None }));

    let sender = JoinSender {
        state: Some(Rc::clone(&state)),
    };
    let handle = JoinHandle { state };

    (sender, handle)
}

impl<T> JoinState<T> {
    fn complete(&mut self, result: Result<T, JoinError>) -> Option<Waker> {
        match self {
            JoinState::Pending { waker } => {
                let waker = waker.take();
                *self = JoinState::Ready(result);
                waker
            }
            JoinState::Ready(_) | JoinState::Consumed => {
                panic!("JoinState already completed should not call complete")
            }
        }
    }
}

pub(crate) struct JoinSender<T> {
    state: Option<Rc<RefCell<JoinState<T>>>>,
}

impl<T> JoinSender<T> {
    pub(crate) fn publish(mut self, result: Result<T, JoinError>) {
        self.publish_inner(result);
    }

    fn publish_inner(&mut self, result: Result<T, JoinError>) {
        let Some(state) = self.state.take() else {
            return;
        };

        let waker = state.borrow_mut().complete(result);

        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Drop for JoinSender<T> {
    fn drop(&mut self) {
        self.publish_inner(Err(JoinError::Cancelled));
    }
}

#[cfg(test)]
mod tests {
    use std::{
        pin::pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Wake},
    };

    use super::*;

    #[test]
    fn completed_join_is_immediately_ready() {
        let (sender, handle) = join_pair::<i32>();

        sender.publish(Ok(42));

        let mut handle = pin!(handle);

        let result = handle.as_mut().poll(&mut context());

        assert_eq!(result, Poll::Ready(Ok(42)));
    }

    #[test]
    fn completion_wakes_join_waiter() {
        let (sender, handle) = join_pair::<i32>();

        let counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&counter));
        let mut cx = Context::from_waker(&waker);
        let mut handle = pin!(handle);

        let result = handle.as_mut().poll(&mut cx);

        assert!(result.is_pending());

        sender.publish(Ok(42));

        assert_eq!(counter.count(), 1);
        assert_eq!(handle.as_mut().poll(&mut cx), Poll::Ready(Ok(42)));
    }

    #[test]
    fn dropping_sender_cancels_join() {
        let (sender, handle) = join_pair::<i32>();

        let counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&counter));
        let mut cx = Context::from_waker(&waker);
        let mut handle = pin!(handle);

        let result = handle.as_mut().poll(&mut cx);

        assert!(result.is_pending());

        drop(sender);

        assert_eq!(counter.count(), 1);
        assert_eq!(
            handle.as_mut().poll(&mut cx),
            Poll::Ready(Err(JoinError::Cancelled))
        );
    }

    #[test]
    fn latest_join_waker_is_notified() {
        let (sender, handle) = join_pair::<i32>();

        let counter_a = Arc::new(WakeCounter::default());
        let counter_b = Arc::new(WakeCounter::default());

        let waker_a = Waker::from(Arc::clone(&counter_a));
        let waker_b = Waker::from(Arc::clone(&counter_b));

        let mut cx_a = Context::from_waker(&waker_a);
        let mut cx_b = Context::from_waker(&waker_b);
        let mut handle = pin!(handle);

        let _ = handle.as_mut().poll(&mut cx_a);
        let _ = handle.as_mut().poll(&mut cx_b);

        sender.publish(Ok(42));

        assert_eq!(counter_a.count(), 0);
        assert_eq!(counter_b.count(), 1);

        assert_eq!(handle.as_mut().poll(&mut cx_b), Poll::Ready(Ok(42)));
    }

    #[test]
    fn dropping_join_handle_removes_waiter() {
        let (sender, handle) = join_pair::<i32>();

        let counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&counter));
        let mut cx = Context::from_waker(&waker);
        {
            let mut handle = pin!(handle);

            let result = handle.as_mut().poll(&mut cx);

            assert!(result.is_pending());
        }

        drop(sender);

        assert_eq!(counter.count(), 0);
    }

    fn context() -> Context<'static> {
        Context::from_waker(Waker::noop())
    }

    #[derive(Default)]
    struct WakeCounter {
        wakes: AtomicUsize,
    }

    impl WakeCounter {
        fn count(&self) -> usize {
            self.wakes.load(Ordering::SeqCst)
        }
    }

    impl Wake for WakeCounter {
        fn wake(self: Arc<Self>) {
            self.wakes.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.wakes.fetch_add(1, Ordering::SeqCst);
        }
    }
}
