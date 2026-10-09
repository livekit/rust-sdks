use std::process::ExitCode;

use clap::Parser;
use livekit::{
    prelude::{Room, RoomOptions},
    webrtc::stats::{IceCandidateType, IceServerTransportProtocol, RtcStats},
};

#[derive(Debug, Parser)]
#[command(about = "Report how this machine connects to LiveKit")]
struct Args {
    /// LiveKit server URL.
    #[arg(long)]
    livekit_url: String,

    /// LiveKit participant token.
    #[arg(long)]
    livekit_token: String,
}

#[derive(Debug, PartialEq)]
struct ConnectionPath {
    local_candidate_type: Option<IceCandidateType>,
    protocol: String,
    relay_protocol: Option<IceServerTransportProtocol>,
    turn_url: String,
    remote_address: String,
    remote_port: i32,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();

    let mut options = RoomOptions::default();
    options.auto_subscribe = false;

    let (room, _events) = match Room::connect(&args.livekit_url, &args.livekit_token, options).await
    {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("Could not connect to LiveKit: {error}");
            return ExitCode::FAILURE;
        }
    };

    let stats = match room.get_stats().await {
        Ok(stats) => stats,
        Err(error) => {
            eprintln!("Connected to LiveKit, but could not inspect the connection: {error}");
            return ExitCode::FAILURE;
        }
    };

    let paths = [
        ("Publisher", selected_path(&stats.publisher_stats)),
        ("Subscriber", selected_path(&stats.subscriber_stats)),
    ];
    let selected: Vec<_> =
        paths.into_iter().filter_map(|(name, path)| path.map(|path| (name, path))).collect();

    println!("Connected to LiveKit.");

    match selected.as_slice() {
        [] => {
            eprintln!("No selected WebRTC candidate pair was found.");
            return ExitCode::FAILURE;
        }
        [(_, path)] => print_path(path),
        paths => {
            for (index, (name, path)) in paths.iter().enumerate() {
                if index > 0 {
                    println!();
                }
                println!("{name} connection:");
                print_path(path);
            }
        }
    }

    if let Err(error) = room.close().await {
        eprintln!("Warning: could not close the diagnostic room cleanly: {error}");
    }

    ExitCode::SUCCESS
}

fn selected_path(stats: &[RtcStats]) -> Option<ConnectionPath> {
    let pair_id = stats.iter().find_map(|stat| match stat {
        RtcStats::Transport(transport)
            if !transport.transport.selected_candidate_pair_id.is_empty() =>
        {
            Some(transport.transport.selected_candidate_pair_id.as_str())
        }
        _ => None,
    })?;

    let pair = stats.iter().find_map(|stat| match stat {
        RtcStats::CandidatePair(pair) if pair.rtc.id == pair_id => Some(pair),
        _ => None,
    })?;
    let local = stats.iter().find_map(|stat| match stat {
        RtcStats::LocalCandidate(candidate)
            if candidate.rtc.id == pair.candidate_pair.local_candidate_id =>
        {
            Some(candidate)
        }
        _ => None,
    })?;
    let remote = stats.iter().find_map(|stat| match stat {
        RtcStats::RemoteCandidate(candidate)
            if candidate.rtc.id == pair.candidate_pair.remote_candidate_id =>
        {
            Some(candidate)
        }
        _ => None,
    })?;

    Some(ConnectionPath {
        local_candidate_type: local.local_candidate.candidate_type,
        protocol: local.local_candidate.protocol.clone(),
        relay_protocol: local.local_candidate.relay_protocol,
        turn_url: local.local_candidate.url.clone(),
        remote_address: remote.remote_candidate.address.clone(),
        remote_port: remote.remote_candidate.port,
    })
}

fn print_path(path: &ConnectionPath) {
    println!("  Connection: {}", connection_label(path));

    if path.local_candidate_type == Some(IceCandidateType::Relay) && !path.turn_url.is_empty() {
        println!("  TURN server: {}", path.turn_url);
    }
    if !path.remote_address.is_empty() {
        println!(
            "  LiveKit endpoint: {}:{}/{}",
            path.remote_address,
            path.remote_port,
            path.protocol.to_ascii_lowercase()
        );
    }
}

fn connection_label(path: &ConnectionPath) -> String {
    let protocol = path.protocol.to_ascii_uppercase();

    if path.local_candidate_type != Some(IceCandidateType::Relay) {
        return format!("Direct {protocol}");
    }

    match path.relay_protocol {
        Some(IceServerTransportProtocol::Udp) => "TURN/UDP".to_string(),
        Some(IceServerTransportProtocol::Tcp) => "TURN/TCP".to_string(),
        Some(IceServerTransportProtocol::Tls) => "TURN/TLS".to_string(),
        None => format!("TURN ({protocol} candidate)"),
    }
}

#[cfg(test)]
mod tests {
    use livekit::webrtc::stats::{
        dictionaries, CandidatePairStats, LocalCandidateStats, RemoteCandidateStats, TransportStats,
    };

    use super::*;

    fn stats_for_path(
        candidate_type: IceCandidateType,
        protocol: &str,
        relay_protocol: Option<IceServerTransportProtocol>,
    ) -> Vec<RtcStats> {
        vec![
            RtcStats::Transport(TransportStats {
                rtc: dictionaries::RtcStats { id: "transport".to_string(), ..Default::default() },
                transport: dictionaries::TransportStats {
                    selected_candidate_pair_id: "selected-pair".to_string(),
                    ..Default::default()
                },
            }),
            RtcStats::CandidatePair(CandidatePairStats {
                rtc: dictionaries::RtcStats {
                    id: "selected-pair".to_string(),
                    ..Default::default()
                },
                candidate_pair: dictionaries::CandidatePairStats {
                    local_candidate_id: "local".to_string(),
                    remote_candidate_id: "remote".to_string(),
                    ..Default::default()
                },
            }),
            RtcStats::LocalCandidate(LocalCandidateStats {
                rtc: dictionaries::RtcStats { id: "local".to_string(), ..Default::default() },
                local_candidate: dictionaries::IceCandidateStats {
                    protocol: protocol.to_string(),
                    candidate_type: Some(candidate_type),
                    relay_protocol,
                    url: "turns:turn.example.com:443".to_string(),
                    ..Default::default()
                },
            }),
            RtcStats::RemoteCandidate(RemoteCandidateStats {
                rtc: dictionaries::RtcStats { id: "remote".to_string(), ..Default::default() },
                remote_candidate: dictionaries::IceCandidateStats {
                    address: "203.0.113.10".to_string(),
                    port: 50000,
                    protocol: protocol.to_string(),
                    ..Default::default()
                },
            }),
        ]
    }

    #[test]
    fn reports_direct_udp_path() {
        let stats = stats_for_path(IceCandidateType::Srflx, "udp", None);
        let path = selected_path(&stats).expect("selected path");

        assert_eq!(connection_label(&path), "Direct UDP");
        assert_eq!(path.remote_address, "203.0.113.10");
        assert_eq!(path.remote_port, 50000);
    }

    #[test]
    fn reports_turn_tls_path() {
        let stats =
            stats_for_path(IceCandidateType::Relay, "udp", Some(IceServerTransportProtocol::Tls));
        let path = selected_path(&stats).expect("selected path");

        assert_eq!(connection_label(&path), "TURN/TLS");
        assert_eq!(path.local_candidate_type, Some(IceCandidateType::Relay));
    }
}
