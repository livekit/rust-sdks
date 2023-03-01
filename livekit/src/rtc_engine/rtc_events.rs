use super::pc_transport::PCTransport;
use crate::proto;
use crate::rtc_engine::pc_transport::OnOfferHandler;
use livekit_webrtc::{self as rtc, prelude::*};
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
    Track {
        transceiver: RtpTransceiver,
    },
    Data {
        data: Vec<u8>,
        binary: bool,
    },
}

/// Handlers used to forward events to a channel
/// Every callback here is called on the signaling thread

fn on_connection_state_change(
    target: proto::SignalTarget,
    emitter: RTCEmitter,
) -> rtc::peer_connection::OnConnectionChange {
    Box::new(move |state| {
        let _ = emitter.send(RTCEvent::ConnectionChange { state, target });
    })
}

fn on_ice_candidate(
    target: proto::SignalTarget,
    emitter: RTCEmitter,
) -> rtc::peer_connection::OnIceCandidate {
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

fn on_data_channel(
    target: proto::SignalTarget,
    emitter: RTCEmitter,
) -> rtc::peer_connection::OnDataChannel {
    Box::new(move |mut data_channel| {
        data_channel.on_message(Some(on_message(emitter.clone())));

        let _ = emitter.send(RTCEvent::DataChannel {
            data_channel,
            target,
        });
    })
}

fn on_track(target: proto::SignalTarget, emitter: RTCEmitter) -> rtc::peer_connection::OnTrack {
    Box::new(move |transceiver| {
        let _ = emitter.send(RTCEvent::Track { transceiver });
    })
}

fn on_ice_candidate_error(
    target: proto::SignalTarget,
    _emitter: RTCEmitter,
) -> rtc::peer_connection::OnIceCandidateError {
    Box::new(move |ice_error| {
        error!("{:?}", ice_error);
    })
}

pub fn forward_pc_events(transport: &mut PCTransport, rtc_emitter: RTCEmitter) {
    let signal_target = transport.signal_target();
    transport
        .peer_connection()
        .on_ice_candidate(Some(on_ice_candidate(signal_target, rtc_emitter.clone())));

    transport
        .peer_connection()
        .on_data_channel(Some(on_data_channel(signal_target, rtc_emitter.clone())));

    transport
        .peer_connection()
        .on_track(Some(on_track(signal_target, rtc_emitter.clone())));

    transport
        .peer_connection()
        .on_connection_state_change(Some(on_connection_state_change(
            signal_target,
            rtc_emitter.clone(),
        )));

    transport
        .peer_connection()
        .on_ice_candidate_error(Some(on_ice_candidate_error(
            signal_target,
            rtc_emitter.clone(),
        )));

    transport.on_offer(on_offer(transport.signal_target(), rtc_emitter.clone()));
}

fn on_message(emitter: RTCEmitter) -> rtc::data_channel::OnMessage {
    Box::new(move |buffer| {
        let _ = emitter.send(RTCEvent::Data {
            data: buffer.data.to_vec(),
            binary: buffer.binary,
        });
    })
}

pub fn forward_dc_events(dc: &mut DataChannel, rtc_emitter: RTCEmitter) {
    dc.on_message(Some(on_message(rtc_emitter.clone())));
}
