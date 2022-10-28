use livekit_webrtc::data_channel::{DataChannel, OnMessageHandler};
use livekit_webrtc::jsep::{IceCandidate, SessionDescription};
use livekit_webrtc::media_stream::MediaStream;
use livekit_webrtc::peer_connection::{
    OnAddTrackHandler, OnConnectionChangeHandler, OnDataChannelHandler, OnIceCandidateHandler,
    PeerConnectionState,
};
use livekit_webrtc::rtp_receiver::RtpReceiver;
use tokio::sync::mpsc;

use crate::proto::SignalTarget;
use crate::rtc_engine::pc_transport::OnOfferHandler;

pub(super) type RTCEmitter = mpsc::UnboundedSender<RTCEvent>;
pub(super) type RTCEvents = mpsc::UnboundedReceiver<RTCEvent>;

#[derive(Debug)]
pub(super) enum RTCEvent {
    IceCandidate {
        ice_candidate: IceCandidate,
        target: SignalTarget,
    },
    ConnectionChange {
        state: PeerConnectionState,
        target: SignalTarget,
    },
    DataChannel {
        data_channel: DataChannel,
        target: SignalTarget,
    },
    Offer {
        offer: SessionDescription,
        target: SignalTarget,
    },
    AddTrack {
        rtp_receiver: RtpReceiver,
        streams: Vec<MediaStream>,
        target: SignalTarget,
    },
    Data {
        data: Vec<u8>,
        binary: bool,
    },
}

/// Handlers used to forward event to a channel
/// Every callback here is called on the signaling thread

pub(super) fn on_connection_change(
    target: SignalTarget,
    emitter: RTCEmitter,
) -> OnConnectionChangeHandler {
    Box::new(move |state| {
        let _ = emitter.send(RTCEvent::ConnectionChange { state, target });
    })
}

pub(super) fn on_ice_candidate(target: SignalTarget, emitter: RTCEmitter) -> OnIceCandidateHandler {
    Box::new(move |ice_candidate| {
        let _ = emitter.send(RTCEvent::IceCandidate {
            ice_candidate,
            target,
        });
    })
}

pub(super) fn on_offer(target: SignalTarget, emitter: RTCEmitter) -> OnOfferHandler {
    Box::new(move |offer| {
        let _ = emitter.send(RTCEvent::Offer { offer, target });

        Box::pin(async {})
    })
}

pub(super) fn on_data_channel(target: SignalTarget, emitter: RTCEmitter) -> OnDataChannelHandler {
    Box::new(move |mut data_channel| {
        data_channel.on_message(on_message(emitter.clone()));

        let _ = emitter.send(RTCEvent::DataChannel {
            data_channel,
            target,
        });
    })
}

pub(super) fn on_add_track(target: SignalTarget, emitter: RTCEmitter) -> OnAddTrackHandler {
    Box::new(move |rtp_receiver, streams| {
        let _ = emitter.send(RTCEvent::AddTrack {
            rtp_receiver,
            streams,
            target,
        });
    })
}

pub(super) fn on_message(emitter: RTCEmitter) -> OnMessageHandler {
    Box::new(move |data, binary| {
        let _ = emitter.send(RTCEvent::Data {
            data: data.to_vec(),
            binary,
        });
    })
}
