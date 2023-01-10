use super::pc_transport::PCTransport;
use crate::proto;
use crate::rtc_engine::pc_transport::OnOfferHandler;
use livekit_webrtc::data_channel::OnMessageHandler;
use livekit_webrtc::peer_connection::{
    OnAddTrackHandler, OnConnectionChangeHandler, OnDataChannelHandler, OnIceCandidateErrorHandler,
    OnIceCandidateHandler, PeerConnectionState,
};
use livekit_webrtc::prelude::*;
use tokio::sync::mpsc;
use tracing::error;

pub type RTCEmitter = mpsc::UnboundedSender<RTCEvent>;
pub type RTCEvents = mpsc::UnboundedReceiver<RTCEvent>;

#[derive(Debug)]
pub enum RTCEvent {
    IceCandidate {
        ice_candidate: IceCandidate,
        target: proto::SignalTarget,
    },
    ConnectionChange {
        state: PeerConnectionState,
        target: proto::SignalTarget,
    },
    DataChannel {
        data_channel: DataChannel,
        target: proto::SignalTarget,
    },
    // TODO (theomonnom): Move Offer to PCTransport
    Offer {
        offer: SessionDescription,
        target: proto::SignalTarget,
    },
    AddTrack {
        rtp_receiver: RtpReceiver,
        streams: Vec<MediaStream>,
        target: proto::SignalTarget,
    },
    Data {
        data: Vec<u8>,
        binary: bool,
    },
}

/// Handlers used to forward events to a channel
/// Every callback here is called on the signaling thread

fn on_connection_change(
    target: proto::SignalTarget,
    emitter: RTCEmitter,
) -> OnConnectionChangeHandler {
    Box::new(move |state| {
        let _ = emitter.send(RTCEvent::ConnectionChange { state, target });
    })
}

fn on_ice_candidate(target: proto::SignalTarget, emitter: RTCEmitter) -> OnIceCandidateHandler {
    Box::new(move |ice_candidate| {
        let _ = emitter.send(RTCEvent::IceCandidate {
            ice_candidate,
            target,
        });
    })
}

fn on_offer(target: proto::SignalTarget, emitter: RTCEmitter) -> OnOfferHandler {
    Box::new(move |offer| {
        let _ = emitter.send(RTCEvent::Offer { offer, target });

        Box::pin(async {})
    })
}

fn on_data_channel(target: proto::SignalTarget, emitter: RTCEmitter) -> OnDataChannelHandler {
    Box::new(move |mut data_channel| {
        data_channel.on_message(on_message(emitter.clone()));

        let _ = emitter.send(RTCEvent::DataChannel {
            data_channel,
            target,
        });
    })
}

fn on_add_track(target: proto::SignalTarget, emitter: RTCEmitter) -> OnAddTrackHandler {
    Box::new(move |rtp_receiver, streams| {
        let _ = emitter.send(RTCEvent::AddTrack {
            rtp_receiver,
            streams,
            target,
        });
    })
}

fn on_ice_candidate_error(
    target: proto::SignalTarget,
    _emitter: RTCEmitter,
) -> OnIceCandidateErrorHandler {
    Box::new(move |address, port, url, error_code, error_text| {
        error!(
            "ICE candidate error ({:?}): address: {} - port: {} - url: {} - error_code: {} - error_text: {}",
            target, address, port, url, error_code, error_text
        );
    })
}

pub fn forward_pc_events(transport: &mut PCTransport, rtc_emitter: RTCEmitter) {
    let signal_target = transport.signal_target();
    transport
        .peer_connection()
        .on_ice_candidate(on_ice_candidate(signal_target, rtc_emitter.clone()));

    transport
        .peer_connection()
        .on_data_channel(on_data_channel(signal_target, rtc_emitter.clone()));

    transport
        .peer_connection()
        .on_add_track(on_add_track(signal_target, rtc_emitter.clone()));

    transport
        .peer_connection()
        .on_connection_change(on_connection_change(signal_target, rtc_emitter.clone()));

    transport
        .peer_connection()
        .on_ice_candidate_error(on_ice_candidate_error(signal_target, rtc_emitter.clone()));

    transport.on_offer(on_offer(transport.signal_target(), rtc_emitter.clone()));
}

fn on_message(emitter: RTCEmitter) -> OnMessageHandler {
    Box::new(move |data, binary| {
        let _ = emitter.send(RTCEvent::Data {
            data: data.to_vec(),
            binary,
        });
    })
}

pub fn forward_dc_events(dc: &mut DataChannel, rtc_emitter: RTCEmitter) {
    dc.on_message(on_message(rtc_emitter.clone()));
}
