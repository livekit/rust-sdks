use anyhow::{Context, Result};
use apriltag::{Detector, Family, Image, TagParams};
use livekit::prelude::{DataPacket, Room};
use serde::Serialize;
use std::sync::{
    mpsc::{self, SyncSender, TrySendError},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

pub const MARKER_POSE_TOPIC: &str = "teleop.marker.v1";

const REFERENCE_WIDTH: f64 = 1920.0;
const REFERENCE_HEIGHT: f64 = 1080.0;
const SOURCE_FX: f64 = 538.7143161761946;
const SOURCE_FY: f64 = 538.8912666335148;
const SOURCE_CX: f64 = 948.5034348398358;
const SOURCE_CY: f64 = 563.536984959879;
const DISTORTION: [f64; 4] =
    [-0.03579060568836083, -0.023648574141658232, 0.05397743838805422, -0.04099689497026788];
const RECTIFIED_FX: f64 = 614.9743626140053;
const RECTIFIED_FY: f64 = 614.9743626140053;
const RECTIFIED_CX: f64 = 971.1460113525391;
const RECTIFIED_CY: f64 = 539.9486312866211;
// Transpose of the calibrated left-eye rectification matrix.
const INVERSE_RECTIFICATION: [[f64; 3]; 3] = [
    [0.9999913447042573, -0.003984833777836211, 0.0011965017067951445],
    [0.003984227368875251, 0.9999919335076701, 0.0005087748664081354],
    [-0.0011985194384962834, -0.0005040033279640959, 0.9999991547655432],
];

#[derive(Clone, Copy, Debug)]
pub struct MarkerDetectorConfig {
    pub marker_id: usize,
    pub marker_size_m: f64,
    pub detection_fps: f64,
    pub minimum_decision_margin: f32,
}

#[derive(Debug)]
struct DetectionFrame {
    luma: Vec<u8>,
    eye_width: usize,
    height: usize,
    timestamp_us: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarkerPoseV1 {
    version: u8,
    #[serde(rename = "type")]
    packet_type: &'static str,
    family: &'static str,
    marker_id: usize,
    timestamp_us: u64,
    visible: bool,
    decision_margin: f32,
    hamming: usize,
    translation_meters: [f64; 3],
    rotation_matrix: [f64; 9],
    corners_pixels: [[f64; 2]; 4],
    rectified_image_size: [usize; 2],
}

impl MarkerPoseV1 {
    fn not_visible(timestamp_us: u64, marker_id: usize, size: [usize; 2]) -> Self {
        Self {
            version: 1,
            packet_type: "marker_pose",
            family: "tag36h11",
            marker_id,
            timestamp_us,
            visible: false,
            decision_margin: 0.0,
            hamming: 0,
            translation_meters: [0.0; 3],
            rotation_matrix: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            corners_pixels: [[0.0; 2]; 4],
            rectified_image_size: size,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisibilityTransition {
    None,
    Acquired,
    Lost,
}

fn visibility_transition(was_visible: bool, is_visible: bool) -> VisibilityTransition {
    match (was_visible, is_visible) {
        (false, true) => VisibilityTransition::Acquired,
        (true, false) => VisibilityTransition::Lost,
        _ => VisibilityTransition::None,
    }
}

pub struct MarkerDetectorHandle {
    frame_tx: Option<SyncSender<DetectionFrame>>,
    detector_thread: Option<thread::JoinHandle<()>>,
    minimum_interval: Duration,
    last_submission: Option<Instant>,
}

impl MarkerDetectorHandle {
    pub fn try_submit_luma(
        &mut self,
        data_y: &[u8],
        stride_y: usize,
        stereo_width: usize,
        height: usize,
        timestamp_us: u64,
    ) {
        if self.last_submission.is_some_and(|last| last.elapsed() < self.minimum_interval) {
            return;
        }
        if stereo_width % 2 != 0 || stride_y < stereo_width {
            log::warn!("Skipping marker detection for invalid stereo luma dimensions");
            return;
        }

        let eye_width = stereo_width / 2;
        let mut luma = vec![0_u8; eye_width * height];
        for (destination, source) in
            luma.chunks_exact_mut(eye_width).zip(data_y.chunks(stride_y).take(height))
        {
            destination.copy_from_slice(&source[..eye_width]);
        }

        let frame = DetectionFrame { luma, eye_width, height, timestamp_us };
        let Some(frame_tx) = self.frame_tx.as_ref() else {
            return;
        };
        match frame_tx.try_send(frame) {
            Ok(()) => self.last_submission = Some(Instant::now()),
            Err(TrySendError::Full(_)) => {
                // Detection intentionally drops frames instead of adding teleop latency.
            }
            Err(TrySendError::Disconnected(_)) => {
                log::warn!("AprilTag detector stopped; marker frames will no longer be published");
                self.frame_tx = None;
            }
        }
    }
}

impl Drop for MarkerDetectorHandle {
    fn drop(&mut self) {
        self.frame_tx.take();
        if let Some(handle) = self.detector_thread.take() {
            if handle.join().is_err() {
                log::warn!("AprilTag detector thread panicked during shutdown");
            }
        }
    }
}

pub fn spawn_marker_detector(
    config: MarkerDetectorConfig,
    room: Arc<Room>,
) -> Result<MarkerDetectorHandle> {
    anyhow::ensure!(config.marker_size_m > 0.0, "--marker-size-m must be positive");
    anyhow::ensure!(config.detection_fps > 0.0, "--marker-detection-fps must be positive");

    let (frame_tx, frame_rx) = mpsc::sync_channel::<DetectionFrame>(1);
    let (pose_tx, mut pose_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    tokio::spawn(async move {
        while let Some(payload) = pose_rx.recv().await {
            let packet = DataPacket {
                payload,
                topic: Some(MARKER_POSE_TOPIC.to_owned()),
                reliable: false,
                ..Default::default()
            };
            if let Err(error) = room.local_participant().publish_data(packet).await {
                log::warn!("Failed to publish AprilTag pose: {error}");
            }
        }
    });

    let detector_thread = thread::Builder::new()
        .name("local-video-apriltag".to_owned())
        .spawn(move || {
            let result = run_detector(config, frame_rx, pose_tx);
            if let Err(error) = result {
                log::error!("AprilTag detector stopped: {error:#}");
            }
        })
        .context("failed to start AprilTag detector thread")?;

    log::info!(
        "AprilTag detection enabled: tag36h11 ID {}, {:.1} mm, {:.1} Hz",
        config.marker_id,
        config.marker_size_m * 1_000.0,
        config.detection_fps,
    );
    Ok(MarkerDetectorHandle {
        frame_tx: Some(frame_tx),
        detector_thread: Some(detector_thread),
        minimum_interval: Duration::from_secs_f64(1.0 / config.detection_fps),
        last_submission: None,
    })
}

fn run_detector(
    config: MarkerDetectorConfig,
    frame_rx: mpsc::Receiver<DetectionFrame>,
    pose_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
) -> Result<()> {
    let mut detector = Detector::builder()
        .add_family_bits(Family::tag_36h11(), 1)
        .build()
        .context("failed to build tag36h11 detector")?;
    detector.set_thread_number(2);
    detector.set_decimation(1.5);
    detector.set_refine_edges(true);
    let mut marker_visible = false;

    while let Ok(frame) = frame_rx.recv() {
        let rectified_width = (frame.eye_width / 2).max(320);
        let rectified_height = (frame.height / 2).max(180);
        let mut image =
            Image::zeros_with_stride(rectified_width, rectified_height, rectified_width)
                .context("failed to allocate rectified AprilTag image")?;
        rectify_left_luma(&frame, image.as_slice_mut(), rectified_width, rectified_height);

        let scale_x = rectified_width as f64 / REFERENCE_WIDTH;
        let scale_y = rectified_height as f64 / REFERENCE_HEIGHT;
        let tag_params = TagParams {
            tagsize: config.marker_size_m,
            fx: RECTIFIED_FX * scale_x,
            fy: RECTIFIED_FY * scale_y,
            cx: RECTIFIED_CX * scale_x,
            cy: RECTIFIED_CY * scale_y,
        };

        let detection = detector
            .detect(&image)
            .into_iter()
            .filter(|detection| detection.id() == config.marker_id)
            .filter(|detection| detection.decision_margin() >= config.minimum_decision_margin)
            .max_by(|left, right| left.decision_margin().total_cmp(&right.decision_margin()));

        let packet = if let Some(detection) = detection {
            if let Some(pose) = detection.estimate_tag_pose(&tag_params) {
                let rotation = pose.rotation().data();
                let translation = pose.translation().data();
                MarkerPoseV1 {
                    version: 1,
                    packet_type: "marker_pose",
                    family: "tag36h11",
                    marker_id: detection.id(),
                    timestamp_us: frame.timestamp_us,
                    visible: true,
                    decision_margin: detection.decision_margin(),
                    hamming: detection.hamming(),
                    translation_meters: [translation[0], translation[1], translation[2]],
                    rotation_matrix: [
                        rotation[0],
                        rotation[1],
                        rotation[2],
                        rotation[3],
                        rotation[4],
                        rotation[5],
                        rotation[6],
                        rotation[7],
                        rotation[8],
                    ],
                    corners_pixels: detection.corners(),
                    rectified_image_size: [rectified_width, rectified_height],
                }
            } else {
                MarkerPoseV1::not_visible(
                    frame.timestamp_us,
                    config.marker_id,
                    [rectified_width, rectified_height],
                )
            }
        } else {
            MarkerPoseV1::not_visible(
                frame.timestamp_us,
                config.marker_id,
                [rectified_width, rectified_height],
            )
        };

        match visibility_transition(marker_visible, packet.visible) {
            VisibilityTransition::Acquired => {
                let [x, y, z] = packet.translation_meters;
                let distance_m = x.hypot(y).hypot(z);
                log::info!(
                    "AprilTag acquired: family={} id={} distance={distance_m:.3}m \
                     position=({x:.3}, {y:.3}, {z:.3})m margin={:.1} hamming={}",
                    packet.family,
                    packet.marker_id,
                    packet.decision_margin,
                    packet.hamming,
                );
            }
            VisibilityTransition::Lost => {
                log::info!("AprilTag lost: family={} id={}", packet.family, packet.marker_id,);
            }
            VisibilityTransition::None => {}
        }
        marker_visible = packet.visible;

        let payload = serde_json::to_vec(&packet).context("failed to encode marker pose")?;
        if pose_tx.send(payload).is_err() {
            break;
        }
    }
    Ok(())
}

fn rectify_left_luma(
    frame: &DetectionFrame,
    output: &mut [u8],
    output_width: usize,
    output_height: usize,
) {
    let source_scale_x = frame.eye_width as f64 / REFERENCE_WIDTH;
    let source_scale_y = frame.height as f64 / REFERENCE_HEIGHT;
    let output_scale_x = output_width as f64 / REFERENCE_WIDTH;
    let output_scale_y = output_height as f64 / REFERENCE_HEIGHT;

    for y in 0..output_height {
        for x in 0..output_width {
            let rectified = [
                (x as f64 - RECTIFIED_CX * output_scale_x) / (RECTIFIED_FX * output_scale_x),
                (y as f64 - RECTIFIED_CY * output_scale_y) / (RECTIFIED_FY * output_scale_y),
                1.0,
            ];
            let source_ray = multiply_matrix_vector(INVERSE_RECTIFICATION, rectified);
            let radial = source_ray[0].hypot(source_ray[1]);
            let theta = radial.atan2(source_ray[2]);
            let theta2 = theta * theta;
            let distorted_theta = theta
                * (1.0
                    + DISTORTION[0] * theta2
                    + DISTORTION[1] * theta2.powi(2)
                    + DISTORTION[2] * theta2.powi(3)
                    + DISTORTION[3] * theta2.powi(4));
            let radial_scale = if radial > 1e-9 { distorted_theta / radial } else { 1.0 };
            let source_x = (SOURCE_FX * source_ray[0] * radial_scale + SOURCE_CX) * source_scale_x;
            let source_y = (SOURCE_FY * source_ray[1] * radial_scale + SOURCE_CY) * source_scale_y;
            output[y * output_width + x] = sample_nearest(frame, source_x, source_y);
        }
    }
}

fn sample_nearest(frame: &DetectionFrame, x: f64, y: f64) -> u8 {
    let x = x.round() as isize;
    let y = y.round() as isize;
    if x < 0 || y < 0 || x >= frame.eye_width as isize || y >= frame.height as isize {
        return 0;
    }
    frame.luma[y as usize * frame.eye_width + x as usize]
}

fn multiply_matrix_vector(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectified_center_maps_near_calibrated_source_center() {
        let ray = multiply_matrix_vector(INVERSE_RECTIFICATION, [0.0, 0.0, 1.0]);
        let radial = ray[0].hypot(ray[1]);
        assert!(radial < 0.01);
    }

    #[test]
    fn hidden_packet_uses_protocol_shape() {
        let json = serde_json::to_value(MarkerPoseV1::not_visible(7, 0, [960, 540]))
            .expect("packet should serialize");
        assert_eq!(json["type"], "marker_pose");
        assert_eq!(json["markerId"], 0);
        assert_eq!(json["visible"], false);
        assert_eq!(json["rectifiedImageSize"], serde_json::json!([960, 540]));
    }

    #[test]
    fn visibility_logging_only_reports_edges() {
        assert_eq!(visibility_transition(false, true), VisibilityTransition::Acquired);
        assert_eq!(visibility_transition(true, false), VisibilityTransition::Lost);
        assert_eq!(visibility_transition(true, true), VisibilityTransition::None);
        assert_eq!(visibility_transition(false, false), VisibilityTransition::None);
    }
}
