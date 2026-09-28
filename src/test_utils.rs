use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, channel},
    },
    task::{Context, Wake, Waker},
};

pub struct WakeChannel(mpsc::Sender<()>);

pub fn get_wake_channel() -> (Waker, mpsc::Receiver<()>) {
    let (tx, rx) = channel();

    let waker = Waker::from(Arc::new(WakeChannel(tx)));

    (waker, rx)
}

impl Wake for WakeChannel {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(());
    }
}

pub(crate) fn counting_waker() -> (Arc<WakeCounter>, Waker) {
    let counter = Arc::new(WakeCounter::default());
    let waker = Waker::from(Arc::clone(&counter));

    (counter, waker)
}

#[derive(Default)]
pub(crate) struct WakeCounter {
    wakes: AtomicUsize,
}

impl WakeCounter {
    pub(crate) fn count(&self) -> usize {
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

pub fn noop_context() -> Context<'static> {
    Context::from_waker(Waker::noop())
}
