use core::num::flt2dec::Sign;
use std::fmt::Debug;

use thiserror::Error;
use tokio_tungstenite::tungstenite::Error as WsError;

use crate::event::{Emitter, Events};
use crate::proto::{signal_request, signal_response};
use crate::signal_client::signal_stream::SignalStream;

mod signal_stream;

type SignalEmitter = Emitter<SignalEvent>;
type SignalEvents = Events<SignalEvent>;
type SignalResult<T> = Result<T, SignalError>;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("websocket failure")]
    WsError(#[from] WsError),
    #[error("failed to parse the url")]
    UrlParse(#[from] url::ParseError),
    #[error("failed to decode messages from server")]
    ProtoParse(#[from] prost::DecodeError),
}

/// Events used by the RTCEngine who will handle the reconnection logic
#[derive(Clone, Debug)]
pub(crate) enum SignalEvent {
    Open,
    Signal(signal_response::Message),
    Close,
}

#[derive(Debug)]
pub(crate) struct SignalOptions {
    reconnect: bool,
    auto_subscribe: bool,
    sid: String,
    adaptive_stream: bool,
}

#[derive(Debug)]
pub struct SignalClient {
    stream: SignalStream,
    emitter: SignalEmitter,
}

impl SignalClient {
    pub async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> SignalResult<(Self, SignalEvents)> {
        // TODO(theomonnom) Retry initial connection
        let (emitter, receiver) = SignalEmitter::new();
        let events = SignalEvents::new(receiver);
        let stream = SignalStream::connect(url, token, options, emitter.clone()).await?;
        Ok((Self { stream, emitter }, events))
    }

    pub async fn send(&self, signal: signal_request::Message) {
        if let Err(_) = self.stream.send(signal).await {
            // TODO(theomonnom) Queue message ( Ignore on full reconnect )
        }
    }

    pub async fn reconnect(&self) {
        // TODO(theomonnom) Close & recreate SignalStream, also send the queue if needed
    }
}
