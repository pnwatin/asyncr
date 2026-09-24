use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, channel},
    },
    task::{Context, Wake, Waker},
};

pub struct WakeChannel(mpsc::Sender<()>);

pub fn get_wake_channel() -> (WakeChannel, mpsc::Receiver<()>) {
    let (tx, rx) = channel();

    (WakeChannel(tx), rx)
}

impl Wake for WakeChannel {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(());
    }
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
