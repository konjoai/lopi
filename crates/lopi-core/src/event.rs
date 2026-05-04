use tokio::sync::broadcast;

/// Thin wrapper around `tokio::sync::broadcast` for workspace-wide event fanout.
/// All subscribers receive every event; lagged subscribers get `RecvError::Lagged`.
#[derive(Clone)]
pub struct EventBus<T: Clone> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone + Send + 'static> EventBus<T> {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn send(&self, event: T) {
        let _ = self.tx.send(event); // ignore "no subscribers"
    }

    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.tx.subscribe()
    }

    pub fn sender(&self) -> broadcast::Sender<T> {
        self.tx.clone()
    }
}
