use crate::platform;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataChannelError {
    #[error("failed to send data, dc not open? send buffer is full ?")]
    Send,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DataState {
    Connecting,
    Open,
    Closing,
    Closed,
}

pub type OnStateChange = Box<dyn FnMut(DataState) + Send + Sync>;
pub type OnMessage = Box<dyn FnMut(&[u8], bool) + Send + Sync>;
pub type OnBufferedAmountChange = Box<dyn FnMut(u64) + Send + Sync>;

pub(crate) trait DataChannelTrait {
    fn send(&self, data: &[u8], binary: bool) -> Result<(), DataChannelError>;
    fn label(&self) -> String;
    fn state(&self) -> DataState;
    fn close(&self);
    fn on_state_change(&self, callback: Option<OnStateChange>);
    fn on_message(&self, callback: Option<OnMessage>);
    fn on_buffered_amount_change(&self, callback: Option<OnBufferedAmountChange>);
}

#[derive(Clone)]
pub struct DataChannel {
    handle: platform::DataChannel,
}

impl DataChannel {}
