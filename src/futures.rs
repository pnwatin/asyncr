use std::task::Poll;

#[derive(Default)]
pub struct YieldOnceFut {
    polled: bool,
}

impl YieldOnceFut {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Future for YieldOnceFut {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if self.polled {
            return Poll::Ready(());
        }

        self.get_mut().polled = true;
        cx.waker().wake_by_ref();

        Poll::Pending
    }
}

pub struct CountDownFut {
    pending_polls_remaining: u32,
}

impl CountDownFut {
    pub fn new(pending_polls_remaining: u32) -> Self {
        Self {
            pending_polls_remaining,
        }
    }
}

impl Future for CountDownFut {
    type Output = ();
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if self.pending_polls_remaining == 0 {
            return Poll::Ready(());
        }

        self.get_mut().pending_polls_remaining -= 1;
        cx.waker().wake_by_ref();

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        pin::pin,
        task::{Context, Waker},
    };

    #[test]
    fn countdown_is_pending_n_times_before_becoming_ready() {
        let mut fut = pin!(CountDownFut::new(3));

        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);

        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    #[test]
    fn countdown_with_one_remaining_poll_is_pending_then_ready() {
        let mut fut = pin!(CountDownFut::new(1));

        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);

        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    #[test]
    fn countdown_with_zero_remaining_polls_is_immediately_ready() {
        let mut fut = pin!(CountDownFut::new(0));

        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);

        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    #[test]
    fn yield_once_is_pending_once_then_ready() {
        let mut fut = pin!(YieldOnceFut::new());

        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);

        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(()));
    }
}
