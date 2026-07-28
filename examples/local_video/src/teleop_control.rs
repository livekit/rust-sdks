use anyhow::{Context, Result};
use livekit::prelude::{Room, RoomEvent};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

pub const ARM_COMMAND_TOPIC: &str = "teleop.arm.command.v1";
const COMMAND_DEADMAN: Duration = Duration::from_millis(350);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArmCommandV1 {
    version: u8,
    #[serde(rename = "type")]
    packet_type: String,
    sequence: u64,
    timestamp_ms: u64,
    marker_id: usize,
    engaged: bool,
    gripper_closed: bool,
    target_meters: [f64; 3],
    joints_radians: [f64; 5],
}

impl ArmCommandV1 {
    fn validate(&self, last_sequence: Option<u64>) -> Result<()> {
        anyhow::ensure!(self.version == 1, "unsupported command version");
        anyhow::ensure!(self.packet_type == "arm_command", "unexpected command type");
        anyhow::ensure!(
            last_sequence.is_none_or(|last| self.sequence > last),
            "out-of-order command sequence"
        );
        anyhow::ensure!(
            self.target_meters.iter().all(|value| value.is_finite()),
            "non-finite target"
        );
        anyhow::ensure!(
            self.joints_radians.iter().all(|value| value.is_finite()),
            "non-finite joint angle"
        );
        // The browser-side IK uses tighter per-joint limits. This receiver is a
        // final sanity boundary before an optional local robot bridge.
        anyhow::ensure!(
            self.joints_radians.iter().all(|value| value.abs() <= 3.2),
            "joint angle exceeds the safety envelope"
        );
        anyhow::ensure!(
            self.target_meters.iter().all(|value| value.abs() <= 0.75),
            "target exceeds the safety envelope"
        );
        Ok(())
    }

    fn disarmed(sequence: u64, marker_id: usize) -> Self {
        Self {
            version: 1,
            packet_type: "arm_command".to_owned(),
            sequence,
            timestamp_ms: 0,
            marker_id,
            engaged: false,
            gripper_closed: false,
            target_meters: [0.0; 3],
            joints_radians: [0.0; 5],
        }
    }
}

pub async fn run_arm_command_receiver(
    room: Arc<Room>,
    ctrl_c_received: Arc<AtomicBool>,
    udp_destination: Option<SocketAddr>,
) -> Result<()> {
    let socket = if udp_destination.is_some() {
        Some(
            UdpSocket::bind("0.0.0.0:0")
                .await
                .context("failed to bind SO-101 command UDP socket")?,
        )
    } else {
        None
    };
    let mut events = room.subscribe();
    let mut watchdog = tokio::time::interval(Duration::from_millis(50));
    watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_sequence = None;
    let mut last_command_at = None;
    let mut active_marker_id = 0;
    let mut engaged = false;

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }
        tokio::select! {
            event = events.recv() => {
                let Some(RoomEvent::DataReceived {
                    payload,
                    topic: Some(topic),
                    participant,
                    ..
                }) = event else {
                    continue;
                };
                if topic != ARM_COMMAND_TOPIC {
                    continue;
                }
                let identity = participant
                    .as_ref()
                    .map(|participant| participant.identity().to_string())
                    .unwrap_or_else(|| "unknown".to_owned());
                let command: ArmCommandV1 = match serde_json::from_slice(payload.as_ref()) {
                    Ok(command) => command,
                    Err(error) => {
                        log::warn!("Rejected malformed arm command from {identity}: {error}");
                        continue;
                    }
                };
                if let Err(error) = command.validate(last_sequence) {
                    log::warn!("Rejected unsafe arm command from {identity}: {error}");
                    continue;
                }

                last_sequence = Some(command.sequence);
                last_command_at = Some(Instant::now());
                active_marker_id = command.marker_id;
                engaged = command.engaged;
                if let (Some(socket), Some(destination)) = (&socket, udp_destination) {
                    let payload = serde_json::to_vec(&command)?;
                    socket.send_to(&payload, destination).await?;
                } else if command.engaged {
                    log::debug!(
                        "Validated SO-101 command seq={} target={:?} joints={:?} claw={}",
                        command.sequence,
                        command.target_meters,
                        command.joints_radians,
                        command.gripper_closed,
                    );
                }
            }
            _ = watchdog.tick() => {
                if engaged
                    && last_command_at.is_some_and(|last| last.elapsed() > COMMAND_DEADMAN)
                {
                    engaged = false;
                    let sequence = last_sequence.unwrap_or_default().saturating_add(1);
                    last_sequence = Some(sequence);
                    let stop = ArmCommandV1::disarmed(sequence, active_marker_id);
                    if let (Some(socket), Some(destination)) = (&socket, udp_destination) {
                        let payload = serde_json::to_vec(&stop)?;
                        socket.send_to(&payload, destination).await?;
                    }
                    log::warn!(
                        "SO-101 command deadman expired after {} ms; sent disarm",
                        COMMAND_DEADMAN.as_millis()
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_order_and_non_finite_commands() {
        let mut command = ArmCommandV1::disarmed(8, 0);
        assert!(command.validate(Some(8)).is_err());
        command.sequence = 9;
        command.target_meters[0] = f64::NAN;
        assert!(command.validate(Some(8)).is_err());
    }
}
