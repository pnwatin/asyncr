use std::{
    any::Any,
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    task::Poll,
};

pub(crate) struct CatchUnwind<F> {
    future: Pin<Box<F>>,
}

impl<F> CatchUnwind<F> {
    pub(crate) fn new(future: F) -> Self {
        Self {
            future: Box::pin(future),
        }
    }
}

impl<F> Future for CatchUnwind<F>
where
    F: Future,
{
    type Output = Result<F::Output, Box<dyn Any>>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let result = catch_unwind(AssertUnwindSafe(|| self.future.as_mut().poll(cx)));

        match result {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Err(panic) => Poll::Ready(Err(panic)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::poll_fn,
        pin::pin,
        task::{Context, Waker},
    };

    use super::*;

    #[test]
    fn returns_output_when_future_completes() {
        let mut fut = pin!(CatchUnwind::new(async { 42 }));

        let result = fut.as_mut().poll(&mut context());

        match result {
            Poll::Ready(Ok(value)) => assert_eq!(value, 42),
            other => panic!("expected Ready(Ok(42)), got {other:?}"),
        }
    }

    #[test]
    fn catches_panic_on_first_poll() {
        let mut fut = pin!(CatchUnwind::new(async {
            panic!("oups");
        }));

        let result = fut.as_mut().poll(&mut context());

        match result {
            Poll::Ready(Err(payload)) => assert_eq!(payload.downcast_ref::<&str>(), Some(&"oups")),
            _ => panic!("expected Ready(Err(...))"),
        }
    }

    #[test]
    fn propagates_pending() {
        let mut fut = pin!(CatchUnwind::new(poll_fn(|_| { Poll::<()>::Pending })));

        let result = fut.as_mut().poll(&mut context());

        assert!(result.is_pending());
    }

    #[test]
    fn catches_panic_after_future_was_previously_pending() {
        let mut polled = false;

        let inner = poll_fn(move |_| {
            if !polled {
                polled = true;
                Poll::<()>::Pending
            } else {
                panic!("oups");
            }
        });

        let mut fut = pin!(CatchUnwind::new(inner));

        assert!(fut.as_mut().poll(&mut context()).is_pending());

        match fut.as_mut().poll(&mut context()) {
            Poll::Ready(Err(payload)) => assert_eq!(payload.downcast_ref::<&str>(), Some(&"oups")),
            _ => panic!("expected Ready(Err(...))"),
        }
    }

    #[test]
    fn returns_output_after_future_was_previously_pending() {
        let mut polled = false;

        let inner = poll_fn(move |_| {
            if !polled {
                polled = true;
                Poll::Pending
            } else {
                Poll::Ready(42)
            }
        });

        let mut fut = pin!(CatchUnwind::new(inner));

        assert!(fut.as_mut().poll(&mut context()).is_pending());

        match fut.as_mut().poll(&mut context()) {
            Poll::Ready(Ok(value)) => assert_eq!(value, 42),
            other => panic!("expected Ready(Ok(42)), got {other:?}"),
        }
    }

    fn context() -> Context<'static> {
        Context::from_waker(Waker::noop())
    }
}
