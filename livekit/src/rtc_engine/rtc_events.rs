use super::peer_transport::PeerTransport;
use crate::proto;
use crate::rtc_engine::peer_transport::OnOfferCreated;
use livekit_webrtc::{self as rtc, prelude::*};
use tokio::sync::mpsc;
use tracing::{debug, error};

pub type RtcEmitter = mpsc::UnboundedSender<RtcEvent>;
pub type RtcEvents = mpsc::UnboundedReceiver<RtcEvent>;

#[derive(Debug)]
pub enum RtcEvent {
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
    // TODO (theomonnom): Move Offer to PeerTransport
    Offer {
        offer: SessionDescription,
        target: proto::SignalTarget,
    },
    Track {
        receiver: RtpReceiver,
        streams: Vec<MediaStream>,
        track: MediaStreamTrack,
        transceiver: RtpTransceiver,
        target: proto::SignalTarget,
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
    emitter: RtcEmitter,
) -> rtc::peer_connection::OnConnectionChange {
    Box::new(move |state| {
        let _ = emitter.send(RtcEvent::ConnectionChange { state, target });
    })
}

fn on_ice_candidate(
    target: proto::SignalTarget,
    emitter: RtcEmitter,
) -> rtc::peer_connection::OnIceCandidate {
    Box::new(move |ice_candidate| {
        let _ = emitter.send(RtcEvent::IceCandidate {
            ice_candidate,
            target,
        });
    })
}

fn on_offer(target: proto::SignalTarget, emitter: RtcEmitter) -> OnOfferCreated {
    Box::new(move |offer| {
        let _ = emitter.send(RtcEvent::Offer { offer, target });
    })
}

fn on_data_channel(
    target: proto::SignalTarget,
    emitter: RtcEmitter,
) -> rtc::peer_connection::OnDataChannel {
    Box::new(move |data_channel| {
        data_channel.on_message(Some(on_message(emitter.clone())));

        let _ = emitter.send(RtcEvent::DataChannel {
            data_channel,
            target,
        });
    })
}

fn on_track(target: proto::SignalTarget, emitter: RtcEmitter) -> rtc::peer_connection::OnTrack {
    Box::new(move |event| {
        let _ = emitter.send(RtcEvent::Track {
            receiver: event.receiver,
            streams: event.streams,
            track: event.track,
            transceiver: event.transceiver,
            target,
        });
    })
}

fn on_ice_candidate_error(
    _target: proto::SignalTarget,
    _emitter: RtcEmitter,
) -> rtc::peer_connection::OnIceCandidateError {
    Box::new(move |ice_error| {
        error!("{:?}", ice_error);
    })
}

pub fn forward_pc_events(transport: &mut PeerTransport, rtc_emitter: RtcEmitter) {
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

    transport.on_offer(Some(on_offer(signal_target, rtc_emitter.clone())));
}

fn on_message(emitter: RtcEmitter) -> rtc::data_channel::OnMessage {
    Box::new(move |buffer| {
        let _ = emitter.send(RtcEvent::Data {
            data: buffer.data.to_vec(),
            binary: buffer.binary,
        });
    })
}

pub fn forward_dc_events(dc: &mut DataChannel, rtc_emitter: RtcEmitter) {
    dc.on_message(Some(on_message(rtc_emitter.clone())));
}
