use futures::channel::mpsc;

#[derive(Debug)]
pub struct Dispatcher<T>
where
    T: Clone,
{
    senders: Vec<mpsc::UnboundedSender<T>>,
}

impl<T> Default for Dispatcher<T>
where
    T: Clone,
{
    fn default() -> Self {
        Self {
            senders: Default::default(),
        }
    }
}

impl<T> Dispatcher<T>
where
    T: Clone,
{
    pub fn register(&mut self) -> mpsc::UnboundedReceiver<T> {
        let (tx, rx) = mpsc::unbounded();
        self.senders.push(tx);
        rx
    }

    pub fn dispatch(&mut self, msg: &T) {
        self.senders
            .retain(|sender| sender.unbounded_send(msg.clone()).is_ok());
    }
}
