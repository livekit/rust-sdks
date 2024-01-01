use livekit::webrtc::{
    prelude::DataChannelState,
    stats::{
        self as rtc, DtlsRole, DtlsTransportState, IceCandidatePairState, IceCandidateType,
        IceRole, IceServerTransportProtocol, IceTcpCandidateType, IceTransportState,
        QualityLimitationReason,
    },
};

use crate::proto;

impl From<DataChannelState> for proto::DataChannelState {
    fn from(value: DataChannelState) -> Self {
        match value {
            DataChannelState::Connecting => Self::DcConnecting,
            DataChannelState::Open => Self::DcOpen,
            DataChannelState::Closing => Self::DcClosing,
            DataChannelState::Closed => Self::DcClosed,
        }
    }
}

impl From<QualityLimitationReason> for proto::QualityLimitationReason {
    fn from(value: QualityLimitationReason) -> Self {
        match value {
            QualityLimitationReason::None => Self::LimitationNone,
            QualityLimitationReason::Cpu => Self::LimitationCpu,
            QualityLimitationReason::Bandwidth => Self::LimitationBandwidth,
            QualityLimitationReason::Other => Self::LimitationOther,
        }
    }
}

impl From<IceRole> for proto::IceRole {
    fn from(value: IceRole) -> Self {
        match value {
            IceRole::Unknown => Self::IceUnknown,
            IceRole::Controlling => Self::IceControlling,
            IceRole::Controlled => Self::IceControlled,
        }
    }
}

impl From<DtlsTransportState> for proto::DtlsTransportState {
    fn from(value: DtlsTransportState) -> Self {
        match value {
            DtlsTransportState::New => Self::DtlsTransportNew,
            DtlsTransportState::Connecting => Self::DtlsTransportConnecting,
            DtlsTransportState::Connected => Self::DtlsTransportConnected,
            DtlsTransportState::Closed => Self::DtlsTransportClosed,
            DtlsTransportState::Failed => Self::DtlsTransportFailed,
        }
    }
}

impl From<IceTransportState> for proto::IceTransportState {
    fn from(value: IceTransportState) -> Self {
        match value {
            IceTransportState::New => Self::IceTransportNew,
            IceTransportState::Checking => Self::IceTransportChecking,
            IceTransportState::Connected => Self::IceTransportConnected,
            IceTransportState::Completed => Self::IceTransportCompleted,
            IceTransportState::Disconnected => Self::IceTransportDisconnected,
            IceTransportState::Failed => Self::IceTransportFailed,
            IceTransportState::Closed => Self::IceTransportClosed,
        }
    }
}

impl From<DtlsRole> for proto::DtlsRole {
    fn from(value: DtlsRole) -> Self {
        match value {
            DtlsRole::Unknown => Self::DtlsUnknown,
            DtlsRole::Client => Self::DtlsClient,
            DtlsRole::Server => Self::DtlsServer,
        }
    }
}

impl From<IceCandidatePairState> for proto::IceCandidatePairState {
    fn from(value: IceCandidatePairState) -> Self {
        match value {
            IceCandidatePairState::Frozen => Self::PairFrozen,
            IceCandidatePairState::Waiting => Self::PairWaiting,
            IceCandidatePairState::InProgress => Self::PairInProgress,
            IceCandidatePairState::Failed => Self::PairFailed,
            IceCandidatePairState::Succeeded => Self::PairSucceeded,
        }
    }
}

impl From<IceServerTransportProtocol> for proto::IceServerTransportProtocol {
    fn from(value: IceServerTransportProtocol) -> Self {
        match value {
            IceServerTransportProtocol::Udp => Self::TransportUdp,
            IceServerTransportProtocol::Tcp => Self::TransportTcp,
            IceServerTransportProtocol::Tls => Self::TransportTls,
        }
    }
}

impl From<IceCandidateType> for proto::IceCandidateType {
    fn from(value: IceCandidateType) -> Self {
        match value {
            IceCandidateType::Host => Self::Host,
            IceCandidateType::Srflx => Self::Srflx,
            IceCandidateType::Prflx => Self::Prflx,
            IceCandidateType::Relay => Self::Relay,
        }
    }
}

impl From<IceTcpCandidateType> for proto::IceTcpCandidateType {
    fn from(value: IceTcpCandidateType) -> Self {
        match value {
            IceTcpCandidateType::Active => Self::CandidateActive,
            IceTcpCandidateType::Passive => Self::CandidatePassive,
            IceTcpCandidateType::So => Self::CandidateSo,
        }
    }
}

impl From<rtc::RtcStats> for proto::RtcStats {
    fn from(value: rtc::RtcStats) -> Self {
        proto::RtcStats {
            stats: Some(match value {
                rtc::RtcStats::Codec(codec) => proto::rtc_stats::Stats::Codec(codec.into()),
                rtc::RtcStats::InboundRtp(inbound) => {
                    proto::rtc_stats::Stats::InboundRtp(inbound.into())
                }
                rtc::RtcStats::OutboundRtp(outbound) => {
                    proto::rtc_stats::Stats::OutboundRtp(outbound.into())
                }
                rtc::RtcStats::RemoteInboundRtp(remote) => {
                    proto::rtc_stats::Stats::RemoteInboundRtp(remote.into())
                }
                rtc::RtcStats::RemoteOutboundRtp(remote) => {
                    proto::rtc_stats::Stats::RemoteOutboundRtp(remote.into())
                }
                rtc::RtcStats::MediaSource(source) => {
                    proto::rtc_stats::Stats::MediaSource(source.into())
                }
                rtc::RtcStats::MediaPlayout(playout) => {
                    proto::rtc_stats::Stats::MediaPlayout(playout.into())
                }
                rtc::RtcStats::PeerConnection(peer) => {
                    proto::rtc_stats::Stats::PeerConnection(peer.into())
                }
                rtc::RtcStats::DataChannel(channel) => {
                    proto::rtc_stats::Stats::DataChannel(channel.into())
                }
                rtc::RtcStats::Transport(transport) => {
                    proto::rtc_stats::Stats::Transport(transport.into())
                }
                rtc::RtcStats::CandidatePair(pair) => {
                    proto::rtc_stats::Stats::CandidatePair(pair.into())
                }
                rtc::RtcStats::LocalCandidate(candidate) => {
                    proto::rtc_stats::Stats::LocalCandidate(candidate.into())
                }
                rtc::RtcStats::RemoteCandidate(candidate) => {
                    proto::rtc_stats::Stats::RemoteCandidate(candidate.into())
                }
                rtc::RtcStats::Certificate(certificate) => {
                    proto::rtc_stats::Stats::Certificate(certificate.into())
                }
                rtc::RtcStats::Track {} => {
                    proto::rtc_stats::Stats::Track(proto::rtc_stats::Track {})
                }
            }),
        }
    }
}

impl From<rtc::CodecStats> for proto::rtc_stats::Codec {
    fn from(value: rtc::CodecStats) -> Self {
        Self { rtc: Some(value.rtc.into()), codec: Some(value.codec.into()) }
    }
}

impl From<rtc::InboundRtpStats> for proto::rtc_stats::InboundRtp {
    fn from(value: rtc::InboundRtpStats) -> Self {
        Self {
            rtc: Some(value.rtc.into()),
            stream: Some(value.stream.into()),
            received: Some(value.received.into()),
            inbound: Some(value.inbound.into()),
        }
    }
}

impl From<rtc::OutboundRtpStats> for proto::rtc_stats::OutboundRtp {
    fn from(value: rtc::OutboundRtpStats) -> Self {
        Self {
            rtc: Some(value.rtc.into()),
            stream: Some(value.stream.into()),
            sent: Some(value.sent.into()),
            outbound: Some(value.outbound.into()),
        }
    }
}

impl From<rtc::RemoteInboundRtpStats> for proto::rtc_stats::RemoteInboundRtp {
    fn from(value: rtc::RemoteInboundRtpStats) -> Self {
        Self {
            rtc: Some(value.rtc.into()),
            stream: Some(value.stream.into()),
            received: Some(value.received.into()),
            remote_inbound: Some(value.remote_inbound.into()),
        }
    }
}

impl From<rtc::RemoteOutboundRtpStats> for proto::rtc_stats::RemoteOutboundRtp {
    fn from(value: rtc::RemoteOutboundRtpStats) -> Self {
        Self {
            rtc: Some(value.rtc.into()),
            stream: Some(value.stream.into()),
            sent: Some(value.sent.into()),
            remote_outbound: Some(value.remote_outbound.into()),
        }
    }
}

impl From<rtc::MediaSourceStats> for proto::rtc_stats::MediaSource {
    fn from(value: rtc::MediaSourceStats) -> Self {
        Self {
            rtc: Some(value.rtc.into()),
            source: Some(value.source.into()),
            audio: Some(value.audio.into()),
            video: Some(value.video.into()),
        }
    }
}

impl From<rtc::MediaPlayoutStats> for proto::rtc_stats::MediaPlayout {
    fn from(value: rtc::MediaPlayoutStats) -> Self {
        Self { rtc: Some(value.rtc.into()), audio_playout: Some(value.audio_playout.into()) }
    }
}

impl From<rtc::PeerConnectionStats> for proto::rtc_stats::PeerConnection {
    fn from(value: rtc::PeerConnectionStats) -> Self {
        Self { rtc: Some(value.rtc.into()), pc: Some(value.pc.into()) }
    }
}

impl From<rtc::DataChannelStats> for proto::rtc_stats::DataChannel {
    fn from(value: rtc::DataChannelStats) -> Self {
        Self { rtc: Some(value.rtc.into()), dc: Some(value.dc.into()) }
    }
}

impl From<rtc::TransportStats> for proto::rtc_stats::Transport {
    fn from(value: rtc::TransportStats) -> Self {
        Self { rtc: Some(value.rtc.into()), transport: Some(value.transport.into()) }
    }
}

impl From<rtc::CandidatePairStats> for proto::rtc_stats::CandidatePair {
    fn from(value: rtc::CandidatePairStats) -> Self {
        Self { rtc: Some(value.rtc.into()), candidate_pair: Some(value.candidate_pair.into()) }
    }
}

impl From<rtc::LocalCandidateStats> for proto::rtc_stats::LocalCandidate {
    fn from(value: rtc::LocalCandidateStats) -> Self {
        Self { rtc: Some(value.rtc.into()), candidate: Some(value.local_candidate.into()) }
    }
}

impl From<rtc::RemoteCandidateStats> for proto::rtc_stats::RemoteCandidate {
    fn from(value: rtc::RemoteCandidateStats) -> Self {
        Self { rtc: Some(value.rtc.into()), candidate: Some(value.remote_candidate.into()) }
    }
}

impl From<rtc::CertificateStats> for proto::rtc_stats::Certificate {
    fn from(value: rtc::CertificateStats) -> Self {
        Self { rtc: Some(value.rtc.into()), certificate: Some(value.certificate.into()) }
    }
}

// Dictionaries

impl From<rtc::dictionaries::RtcStats> for proto::RtcStatsData {
    fn from(value: rtc::dictionaries::RtcStats) -> Self {
        Self { id: value.id, timestamp: value.timestamp }
    }
}

impl From<rtc::dictionaries::CodecStats> for proto::CodecStats {
    fn from(value: rtc::dictionaries::CodecStats) -> Self {
        Self {
            payload_type: value.payload_type,
            transport_id: value.transport_id,
            mime_type: value.mime_type,
            clock_rate: value.clock_rate,
            channels: value.channels,
            sdp_fmtp_line: value.sdp_fmtp_line,
        }
    }
}

impl From<rtc::dictionaries::RtpStreamStats> for proto::RtpStreamStats {
    fn from(value: rtc::dictionaries::RtpStreamStats) -> Self {
        Self {
            ssrc: value.ssrc,
            kind: value.kind,
            transport_id: value.transport_id,
            codec_id: value.codec_id,
        }
    }
}

impl From<rtc::dictionaries::ReceivedRtpStreamStats> for proto::ReceivedRtpStreamStats {
    fn from(value: rtc::dictionaries::ReceivedRtpStreamStats) -> Self {
        Self {
            packets_received: value.packets_received,
            packets_lost: value.packets_lost,
            jitter: value.jitter,
        }
    }
}

impl From<rtc::dictionaries::InboundRtpStreamStats> for proto::InboundRtpStreamStats {
    fn from(value: rtc::dictionaries::InboundRtpStreamStats) -> Self {
        Self {
            track_identifier: value.track_identifier,
            mid: value.mid,
            remote_id: value.remote_id,
            frames_decoded: value.frames_decoded,
            key_frames_decoded: value.key_frames_decoded,
            frames_rendered: value.frames_rendered,
            frames_dropped: value.frames_dropped,
            frame_width: value.frame_width,
            frame_height: value.frame_height,
            frames_per_second: value.frames_per_second,
            qp_sum: value.qp_sum,
            total_decode_time: value.total_decode_time,
            total_inter_frame_delay: value.total_inter_frame_delay,
            total_squared_inter_frame_delay: value.total_squared_inter_frame_delay,
            pause_count: value.pause_count,
            total_pause_duration: value.total_pause_duration,
            freeze_count: value.freeze_count,
            total_freeze_duration: value.total_freeze_duration,
            last_packet_received_timestamp: value.last_packet_received_timestamp,
            header_bytes_received: value.header_bytes_received,
            packets_discarded: value.packets_discarded,
            fec_bytes_received: value.fec_bytes_received,
            fec_packets_received: value.fec_packets_received,
            fec_packets_discarded: value.fec_packets_discarded,
            bytes_received: value.bytes_received,
            nack_count: value.nack_count,
            fir_count: value.fir_count,
            pli_count: value.pli_count,
            total_processing_delay: value.total_processing_delay,
            estimated_playout_timestamp: value.estimated_playout_timestamp,
            jitter_buffer_delay: value.jitter_buffer_delay,
            jitter_buffer_target_delay: value.jitter_buffer_target_delay,
            jitter_buffer_emitted_count: value.jitter_buffer_emitted_count,
            jitter_buffer_minimum_delay: value.jitter_buffer_minimum_delay,
            total_samples_received: value.total_samples_received,
            concealed_samples: value.concealed_samples,
            silent_concealed_samples: value.silent_concealed_samples,
            concealment_events: value.concealment_events,
            inserted_samples_for_deceleration: value.inserted_samples_for_deceleration,
            removed_samples_for_acceleration: value.removed_samples_for_acceleration,
            audio_level: value.audio_level,
            total_audio_energy: value.total_audio_energy,
            total_samples_duration: value.total_samples_duration,
            frames_received: value.frames_received,
            decoder_implementation: value.decoder_implementation,
            playout_id: value.playout_id,
            power_efficient_decoder: value.power_efficient_decoder,
            frames_assembled_from_multiple_packets: value.frames_assembled_from_multiple_packets,
            total_assembly_time: value.total_assembly_time,
            retransmitted_packets_received: value.retransmitted_packets_received,
            retransmitted_bytes_received: value.retransmitted_bytes_received,
            rtx_ssrc: value.rtx_ssrc,
            fec_ssrc: value.fec_ssrc,
        }
    }
}

impl From<rtc::dictionaries::SentRtpStreamStats> for proto::SentRtpStreamStats {
    fn from(value: rtc::dictionaries::SentRtpStreamStats) -> Self {
        Self { packets_sent: value.packets_sent, bytes_sent: value.bytes_sent }
    }
}

impl From<rtc::dictionaries::OutboundRtpStreamStats> for proto::OutboundRtpStreamStats {
    fn from(value: rtc::dictionaries::OutboundRtpStreamStats) -> Self {
        Self {
            mid: value.mid,
            media_source_id: value.media_source_id,
            remote_id: value.remote_id,
            rid: value.rid,
            header_bytes_sent: value.header_bytes_sent,
            retransmitted_packets_sent: value.retransmitted_packets_sent,
            retransmitted_bytes_sent: value.retransmitted_bytes_sent,
            rtx_ssrc: value.rtx_ssrc,
            target_bitrate: value.target_bitrate,
            total_encoded_bytes_target: value.total_encoded_bytes_target,
            frame_width: value.frame_width,
            frame_height: value.frame_height,
            frames_per_second: value.frames_per_second,
            frames_sent: value.frames_sent,
            huge_frames_sent: value.huge_frames_sent,
            frames_encoded: value.frames_encoded,
            key_frames_encoded: value.key_frames_encoded,
            qp_sum: value.qp_sum,
            total_encode_time: value.total_encode_time,
            total_packet_send_delay: value.total_packet_send_delay,
            quality_limitation_reason: proto::QualityLimitationReason::from(
                value.quality_limitation_reason,
            ) as i32,
            quality_limitation_durations: value.quality_limitation_durations,
            quality_limitation_resolution_changes: value.quality_limitation_resolution_changes,
            nack_count: value.nack_count,
            fir_count: value.fir_count,
            pli_count: value.pli_count,
            encoder_implementation: value.encoder_implementation,
            power_efficient_encoder: value.power_efficient_encoder,
            active: value.active,
            scalibility_mode: value.scalibility_mode,
        }
    }
}

impl From<rtc::dictionaries::RemoteInboundRtpStreamStats> for proto::RemoteInboundRtpStreamStats {
    fn from(value: rtc::dictionaries::RemoteInboundRtpStreamStats) -> Self {
        Self {
            local_id: value.local_id,
            round_trip_time: value.round_trip_time,
            total_round_trip_time: value.total_round_trip_time,
            fraction_lost: value.fraction_lost,
            round_trip_time_measurements: value.round_trip_time_measurements,
        }
    }
}

impl From<rtc::dictionaries::RemoteOutboundRtpStreamStats> for proto::RemoteOutboundRtpStreamStats {
    fn from(value: rtc::dictionaries::RemoteOutboundRtpStreamStats) -> Self {
        Self {
            local_id: value.local_id,
            remote_timestamp: value.remote_timestamp,
            reports_sent: value.reports_sent,
            round_trip_time: value.round_trip_time,
            total_round_trip_time: value.total_round_trip_time,
            round_trip_time_measurements: value.round_trip_time_measurements,
        }
    }
}

impl From<rtc::dictionaries::MediaSourceStats> for proto::MediaSourceStats {
    fn from(value: rtc::dictionaries::MediaSourceStats) -> Self {
        Self { track_identifier: value.track_identifier, kind: value.kind }
    }
}

impl From<rtc::dictionaries::AudioSourceStats> for proto::AudioSourceStats {
    fn from(value: rtc::dictionaries::AudioSourceStats) -> Self {
        Self {
            audio_level: value.audio_level,
            total_audio_energy: value.total_audio_energy,
            total_samples_duration: value.total_samples_duration,
            echo_return_loss: value.echo_return_loss,
            echo_return_loss_enhancement: value.echo_return_loss_enhancement,
            dropped_samples_duration: value.dropped_samples_duration,
            dropped_samples_events: value.dropped_samples_events,
            total_capture_delay: value.total_capture_delay,
            total_samples_captured: value.total_samples_captured,
        }
    }
}

impl From<rtc::dictionaries::VideoSourceStats> for proto::VideoSourceStats {
    fn from(value: rtc::dictionaries::VideoSourceStats) -> Self {
        Self {
            width: value.width,
            height: value.height,
            frames: value.frames,
            frames_per_second: value.frames_per_second,
        }
    }
}

impl From<rtc::dictionaries::AudioPlayoutStats> for proto::AudioPlayoutStats {
    fn from(value: rtc::dictionaries::AudioPlayoutStats) -> Self {
        Self {
            kind: value.kind,
            synthesized_samples_duration: value.synthesized_samples_duration,
            synthesized_samples_events: value.synthesized_samples_events,
            total_samples_duration: value.total_samples_duration,
            total_playout_delay: value.total_playout_delay,
            total_samples_count: value.total_samples_count,
        }
    }
}

impl From<rtc::dictionaries::PeerConnectionStats> for proto::PeerConnectionStats {
    fn from(value: rtc::dictionaries::PeerConnectionStats) -> Self {
        Self {
            data_channels_opened: value.data_channels_opened,
            data_channels_closed: value.data_channels_closed,
        }
    }
}

impl From<rtc::dictionaries::DataChannelStats> for proto::DataChannelStats {
    fn from(value: rtc::dictionaries::DataChannelStats) -> Self {
        Self {
            label: value.label,
            protocol: value.protocol,
            data_channel_identifier: value.data_channel_identifier,
            state: value.state.map(|v| proto::DataChannelState::from(v) as i32),
            messages_sent: value.messages_sent,
            bytes_sent: value.bytes_sent,
            messages_received: value.messages_received,
            bytes_received: value.bytes_received,
        }
    }
}

impl From<rtc::dictionaries::TransportStats> for proto::TransportStats {
    fn from(value: rtc::dictionaries::TransportStats) -> Self {
        Self {
            packets_sent: value.packets_sent,
            packets_received: value.packets_received,
            bytes_sent: value.bytes_sent,
            bytes_received: value.bytes_received,
            ice_role: proto::IceRole::from(value.ice_role) as i32,
            ice_local_username_fragment: value.ice_local_username_fragment,
            dtls_state: value.dtls_state.map(|v| proto::DtlsTransportState::from(v) as i32),
            ice_state: value.ice_state.map(|v| proto::IceTransportState::from(v) as i32),
            selected_candidate_pair_id: value.selected_candidate_pair_id,
            local_certificate_id: value.local_certificate_id,
            remote_certificate_id: value.remote_certificate_id,
            tls_version: value.tls_version,
            dtls_cipher: value.dtls_cipher,
            dtls_role: proto::DtlsRole::from(value.dtls_role) as i32,
            srtp_cipher: value.srtp_cipher,
            selected_candidate_pair_changes: value.selected_candidate_pair_changes,
        }
    }
}

impl From<rtc::dictionaries::CandidatePairStats> for proto::CandidatePairStats {
    fn from(value: rtc::dictionaries::CandidatePairStats) -> Self {
        Self {
            transport_id: value.transport_id,
            local_candidate_id: value.local_candidate_id,
            remote_candidate_id: value.remote_candidate_id,
            state: value.state.map(|v| proto::IceCandidatePairState::from(v) as i32),
            nominated: value.nominated,
            packets_sent: value.packets_sent,
            packets_received: value.packets_received,
            bytes_sent: value.bytes_sent,
            bytes_received: value.bytes_received,
            last_packet_sent_timestamp: value.last_packet_sent_timestamp,
            last_packet_received_timestamp: value.last_packet_received_timestamp,
            total_round_trip_time: value.total_round_trip_time,
            current_round_trip_time: value.current_round_trip_time,
            available_outgoing_bitrate: value.available_outgoing_bitrate,
            available_incoming_bitrate: value.available_incoming_bitrate,
            requests_received: value.requests_received,
            requests_sent: value.requests_sent,
            responses_received: value.responses_received,
            responses_sent: value.responses_sent,
            consent_requests_sent: value.consent_requests_sent,
            packets_discarded_on_send: value.packets_discarded_on_send,
            bytes_discarded_on_send: value.bytes_discarded_on_send,
        }
    }
}

impl From<rtc::dictionaries::IceCandidateStats> for proto::IceCandidateStats {
    fn from(value: rtc::dictionaries::IceCandidateStats) -> Self {
        Self {
            transport_id: value.transport_id,
            address: value.address,
            port: value.port,
            protocol: value.protocol,
            candidate_type: value.candidate_type.map(|v| proto::IceCandidateType::from(v) as i32),
            priority: value.priority,
            url: value.url,
            relay_protocol: value
                .relay_protocol
                .map(|v| proto::IceServerTransportProtocol::from(v) as i32),
            foundation: value.foundation,
            related_address: value.related_address,
            related_port: value.related_port,
            username_fragment: value.username_fragment,
            tcp_type: value.tcp_type.map(|v| proto::IceTcpCandidateType::from(v) as i32),
        }
    }
}

impl From<rtc::dictionaries::CertificateStats> for proto::CertificateStats {
    fn from(value: rtc::dictionaries::CertificateStats) -> Self {
        Self {
            fingerprint: value.fingerprint,
            fingerprint_algorithm: value.fingerprint_algorithm,
            base64_certificate: value.base64_certificate,
            issuer_certificate_id: value.issuer_certificate_id,
        }
    }
}
