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

use super::pc_transport::PCTransport;

pub type RTCEmitter = mpsc::UnboundedSender<RTCEvent>;
pub type RTCEvents = mpsc::UnboundedReceiver<RTCEvent>;

#[derive(Debug)]
pub enum RTCEvent {
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
    // TODO (theomonnom): Move Offer to PCTransport
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

/// Handlers used to forward events to a channel
/// Every callback here is called on the signaling thread

fn on_connection_change(target: SignalTarget, emitter: RTCEmitter) -> OnConnectionChangeHandler {
    Box::new(move |state| {
        let _ = emitter.send(RTCEvent::ConnectionChange { state, target });
    })
}

fn on_ice_candidate(target: SignalTarget, emitter: RTCEmitter) -> OnIceCandidateHandler {
    Box::new(move |ice_candidate| {
        let _ = emitter.send(RTCEvent::IceCandidate {
            ice_candidate,
            target,
        });
    })
}

fn on_offer(target: SignalTarget, emitter: RTCEmitter) -> OnOfferHandler {
    Box::new(move |offer| {
        let _ = emitter.send(RTCEvent::Offer { offer, target });

        Box::pin(async {})
    })
}

fn on_data_channel(target: SignalTarget, emitter: RTCEmitter) -> OnDataChannelHandler {
    Box::new(move |mut data_channel| {
        data_channel.on_message(on_message(emitter.clone()));

        let _ = emitter.send(RTCEvent::DataChannel {
            data_channel,
            target,
        });
    })
}

fn on_add_track(target: SignalTarget, emitter: RTCEmitter) -> OnAddTrackHandler {
    Box::new(move |rtp_receiver, streams| {
        let _ = emitter.send(RTCEvent::AddTrack {
            rtp_receiver,
            streams,
            target,
        });
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
