#!/usr/bin/env python3
"""Parse local_video latency logs and write CSV, Markdown, JSON, and PDF reports."""

from __future__ import annotations

import argparse
import csv
import json
import math
import re
import sys
from dataclasses import dataclass, field
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any


SUBSCRIBER_PREFIX = "Subscriber render latency:"
SUBSCRIBER_SINK_PREFIX = "Subscriber sink delivery:"
SUBSCRIBER_RENDER_LOOP_PREFIX = "Subscriber render loop:"
STUTTER_PREFIX = "Subscriber render stutter:"
PUBLISHER_PREFIX = "Publisher frame latency:"
PUBLISH_HEALTH_PREFIX = "Publish health:"
DECODE_HEALTH_PREFIX = "Decode health:"
JITTER_BUFFER_PREFIX = "WebRTC jitter buffer:"
HOST_PROCESS_RE = re.compile(
    r"^\s*(?P<pid>\d+)\s+(?P<cpu>[0-9.]+)\s+(?P<mem>[0-9.]+)\s+(?P<comm>.+?)\s*$"
)

WINDOW_RE = re.compile(
    r"(?P<metric>[A-Za-z0-9_]+) avg=(?P<avg>NA|[-0-9.]+ms) "
    r"min=(?P<min>NA|[-0-9.]+ms) max=(?P<max>NA|[-0-9.]+ms)"
)
PAIR_RE = re.compile(r"(?P<key>[A-Za-z0-9_]+)=(?P<value>[^,]+)")
TIMESTAMP_RE = re.compile(r"^\[(?P<timestamp>[^\]]+)\]")

STUTTER_STAGE_KEYS = (
    "exposure_to_receive",
    "receive_to_decode",
    "decoder_to_sink",
    "sink_to_select",
    "select_to_prepare",
    "prepare_to_paint",
)
HOST_BUSY_PROCESS_CPU_PCT = 50.0
HOST_BUSY_EXTERNAL_TOTAL_CPU_PCT = 150.0
MIN_COVERAGE_DURATION_SECONDS = 10.0
DEFAULT_MIN_FRAME_COVERAGE_PCT = 95.0
DEFAULT_MIN_TIME_COVERAGE_PCT = 95.0
BENCHMARK_PROCESS_NAMES = {
    "awk",
    "bash",
    "caffeinate",
    "head",
    "livekit-server",
    "publisher",
    "ps",
    "sleep",
    "subscriber",
    "sysmond",
    "VTDecoderXPCService",
    "VTEncoderXPCService",
}
ANALYSIS_SCHEMA_VERSION = 4
LATENCY_BUDGET_SPECS = {
    "max_sink_gap_p95_ms": (
        "subscriber",
        "sink_gap_p95_window_max_ms",
        "sink_gap p95 window max",
    ),
    "max_paint_gap_p95_ms": (
        "subscriber",
        "paint_gap_p95_window_max_ms",
        "paint_gap p95 window max",
    ),
    "max_e2e_p95_ms": (
        "subscriber",
        "e2e_p95_window_max_ms",
        "e2e p95 window max",
    ),
    "max_receive_to_decode_p95_ms": (
        "subscriber",
        "receive_to_decode_p95_window_max_ms",
        "receive_to_decode p95 window max",
    ),
    "max_receive_to_paint_p95_ms": (
        "subscriber",
        "receive_to_paint_p95_window_max_ms",
        "receive_to_paint p95 window max",
    ),
    "max_capture_to_packetize_p95_ms": (
        "publisher",
        "capture_to_packetize_p95_window_max_ms",
        "capture_to_packetize p95 window max",
    ),
    "max_encoder_upload_to_output_p95_ms": (
        "publisher",
        "encoder_upload_to_output_p95_window_max_ms",
        "encoder_upload_to_output p95 window max",
    ),
}
RENDER_LATENCY_BUDGET_KEYS = {
    "max_paint_gap_p95_ms",
    "max_e2e_p95_ms",
    "max_receive_to_decode_p95_ms",
    "max_receive_to_paint_p95_ms",
}


@dataclass
class WindowRow:
    source: str
    timestamp: str
    frames: int | None = None
    stutters_over_threshold: int | None = None
    metrics: dict[str, dict[str, float | None]] = field(default_factory=dict)
    values: dict[str, str] = field(default_factory=dict)


@dataclass
class StutterRow:
    timestamp: str
    values: dict[str, str]
    dominant_stage: str | None


@dataclass
class DecodeHealthRow:
    timestamp: str
    received: int | None
    decoded: int | None
    rendered: int | None
    dropped: int | None
    decode_time_s: float | None
    decoder: str | None

    @property
    def receive_decode_backlog(self) -> int | None:
        if self.received is None or self.decoded is None:
            return None
        return max(0, self.received - self.decoded)


@dataclass
class JitterBufferRow:
    timestamp: str
    delay_window_avg_ms: float | None
    delay_avg_ms: float | None
    target_avg_ms: float | None
    minimum_avg_ms: float | None
    emitted: int | None


@dataclass
class HostProcessRow:
    timestamp: str
    pid: int
    cpu_pct: float
    mem_pct: float
    command: str


def repo_root() -> Path:
    return Path(__file__).resolve().parents[3]


def result_dir(name: str) -> Path:
    return repo_root() / "target" / "local_video_latency" / name


def parse_metadata(directory: Path) -> dict[str, str]:
    path = directory / "metadata.txt"
    if not path.exists():
        return {}

    metadata: dict[str, str] = {}
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            key, separator, value = line.rstrip("\n").partition("=")
            if separator:
                metadata[key] = value
    return metadata


def parse_ms(value: str) -> float | None:
    value = value.strip()
    if value == "NA":
        return None
    if value.endswith("ms"):
        value = value[:-2]
    try:
        parsed = float(value)
    except ValueError:
        return None
    if math.isnan(parsed):
        return None
    return parsed


def parse_seconds(value: str) -> float | None:
    value = value.strip()
    if value == "NA":
        return None
    if value.endswith("s"):
        value = value[:-1]
    try:
        parsed = float(value)
    except ValueError:
        return None
    if math.isnan(parsed):
        return None
    return parsed


def parse_float(value: str | None) -> float | None:
    if value is None:
        return None
    try:
        parsed = float(value.strip())
    except ValueError:
        return None
    if math.isnan(parsed):
        return None
    return parsed


def parse_int(value: str | None) -> int | None:
    if value is None:
        return None
    try:
        return int(value.strip())
    except ValueError:
        return None


def parse_bool(value: str | None) -> bool | None:
    if value is None:
        return None
    normalized = value.strip().lower()
    if normalized in ("1", "true", "yes", "y", "on"):
        return True
    if normalized in ("0", "false", "no", "n", "off"):
        return False
    return None


def parse_log_timestamp(line: str) -> str:
    match = TIMESTAMP_RE.match(line)
    if not match:
        return ""
    return match.group("timestamp").split()[0]


def parse_iso_timestamp(value: str) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def parse_window_line(source: str, line: str) -> WindowRow | None:
    prefix_by_source = {
        "subscriber": SUBSCRIBER_PREFIX,
        "subscriber_sink": SUBSCRIBER_SINK_PREFIX,
        "subscriber_render_loop": SUBSCRIBER_RENDER_LOOP_PREFIX,
        "publisher": PUBLISHER_PREFIX,
    }
    prefix = prefix_by_source[source]
    if prefix not in line:
        return None

    payload = line.split(prefix, 1)[1].strip()
    row = WindowRow(source=source, timestamp=parse_log_timestamp(line))
    row.values = {match.group("key"): match.group("value").strip() for match in PAIR_RE.finditer(payload)}

    frames_match = re.search(r"frames=(\d+)", payload)
    if frames_match:
        row.frames = int(frames_match.group(1))

    stutters_match = re.search(r"stutters_over_(?:threshold|50ms)=(\d+)", payload)
    if stutters_match:
        row.stutters_over_threshold = int(stutters_match.group(1))

    for match in WINDOW_RE.finditer(payload):
        metric = match.group("metric")
        row.metrics[metric] = {
            "avg": parse_ms(match.group("avg")),
            "min": parse_ms(match.group("min")),
            "max": parse_ms(match.group("max")),
        }

    return row


def parse_stutter_line(line: str) -> StutterRow | None:
    if STUTTER_PREFIX not in line:
        return None

    payload = line.split(STUTTER_PREFIX, 1)[1].strip()
    values = {match.group("key"): match.group("value").strip() for match in PAIR_RE.finditer(payload)}

    dominant_stage = None
    dominant_value = -1.0
    for key in STUTTER_STAGE_KEYS:
        value = parse_ms(values.get(key, "NA"))
        if value is not None and value > dominant_value:
            dominant_stage = key
            dominant_value = value

    return StutterRow(
        timestamp=parse_log_timestamp(line),
        values=values,
        dominant_stage=dominant_stage,
    )


def parse_decode_health_line(line: str) -> DecodeHealthRow | None:
    if DECODE_HEALTH_PREFIX not in line:
        return None

    payload = line.split(DECODE_HEALTH_PREFIX, 1)[1].strip()
    values = {match.group("key"): match.group("value").strip() for match in PAIR_RE.finditer(payload)}
    return DecodeHealthRow(
        timestamp=parse_log_timestamp(line),
        received=parse_int(values.get("received")),
        decoded=parse_int(values.get("decoded")),
        rendered=parse_int(values.get("rendered")),
        dropped=parse_int(values.get("dropped")),
        decode_time_s=parse_seconds(values.get("decode_time", "NA")),
        decoder=values.get("decoder"),
    )


def parse_jitter_buffer_line(line: str) -> JitterBufferRow | None:
    if JITTER_BUFFER_PREFIX not in line:
        return None

    payload = line.split(JITTER_BUFFER_PREFIX, 1)[1].strip()
    values = {match.group("key"): match.group("value").strip() for match in PAIR_RE.finditer(payload)}
    return JitterBufferRow(
        timestamp=parse_log_timestamp(line),
        delay_window_avg_ms=parse_ms(values.get("delay_window_avg", "NA")),
        delay_avg_ms=parse_ms(values.get("delay_avg", "NA")),
        target_avg_ms=parse_ms(values.get("target_avg", "NA")),
        minimum_avg_ms=parse_ms(values.get("minimum_avg", "NA")),
        emitted=parse_int(values.get("emitted")),
    )


def parse_logs(
    directory: Path,
) -> tuple[
    list[WindowRow],
    list[StutterRow],
    dict[str, list[str]],
    list[DecodeHealthRow],
    list[JitterBufferRow],
]:
    windows: list[WindowRow] = []
    stutters: list[StutterRow] = []
    health: dict[str, list[str]] = {"publisher": [], "subscriber": []}
    decode_health: list[DecodeHealthRow] = []
    jitter_buffers: list[JitterBufferRow] = []

    for source, filename in (("publisher", "publisher.log"), ("subscriber", "subscriber.log")):
        path = directory / filename
        if not path.exists():
            continue
        with path.open("r", encoding="utf-8", errors="replace") as handle:
            for line in handle:
                stripped = line.rstrip("\n")
                if "Ctrl-C received, exiting" in stripped:
                    break
                sources = (
                    ("subscriber", "subscriber_sink", "subscriber_render_loop")
                    if source == "subscriber"
                    else (source,)
                )
                for window_source in sources:
                    window = parse_window_line(window_source, stripped)
                    if window:
                        windows.append(window)
                        break
                else:
                    window = None
                if window:
                    continue
                stutter = parse_stutter_line(stripped)
                if stutter:
                    stutters.append(stutter)
                    continue
                decode_row = parse_decode_health_line(stripped)
                if decode_row:
                    decode_health.append(decode_row)
                    continue
                jitter_row = parse_jitter_buffer_line(stripped)
                if jitter_row:
                    jitter_buffers.append(jitter_row)
                    continue
                if PUBLISH_HEALTH_PREFIX in stripped:
                    health[source].append(stripped)

    return windows, stutters, health, decode_health, jitter_buffers


def parse_host_load_file(path: Path) -> list[HostProcessRow]:
    if not path.exists():
        return []

    rows: list[HostProcessRow] = []
    timestamp = ""
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            stripped = line.strip()
            if stripped.startswith("captured_at="):
                timestamp = stripped.split("=", 1)[1]
                continue

            match = HOST_PROCESS_RE.match(line)
            if not match or not timestamp:
                continue
            rows.append(
                HostProcessRow(
                    timestamp=timestamp,
                    pid=int(match.group("pid")),
                    cpu_pct=float(match.group("cpu")),
                    mem_pct=float(match.group("mem")),
                    command=match.group("comm"),
                )
            )
    return rows


def summarize_host_load(directory: Path) -> dict[str, Any]:
    metadata = parse_metadata(directory)
    metadata_process_threshold = parse_float(
        metadata.get("host_busy_process_cpu_threshold_pct")
    )
    metadata_total_threshold = parse_float(
        metadata.get("host_busy_external_total_cpu_threshold_pct")
    )
    busy_process_cpu_threshold = (
        metadata_process_threshold
        if metadata_process_threshold is not None
        else HOST_BUSY_PROCESS_CPU_PCT
    )
    busy_external_total_cpu_threshold = (
        metadata_total_threshold
        if metadata_total_threshold is not None
        else HOST_BUSY_EXTERNAL_TOTAL_CPU_PCT
    )
    before_rows = parse_host_load_file(directory / "host-load-before.txt")
    sample_rows = parse_host_load_file(directory / "host-load-samples.txt")
    after_rows = parse_host_load_file(directory / "host-load-after.txt")
    all_rows = before_rows + sample_rows + after_rows
    status_rows = sample_rows or all_rows

    snapshots = sorted({row.timestamp for row in status_rows})
    if not status_rows:
        return {
            "status": "NA",
            "snapshots": 0,
            "sample_snapshots": 0,
            "busy_snapshots": 0,
            "busy_process_cpu_threshold_pct": busy_process_cpu_threshold,
            "busy_external_total_cpu_threshold_pct": busy_external_total_cpu_threshold,
            "max_top_process_cpu_pct": None,
            "max_top_process_name": None,
            "max_external_process_cpu_pct": None,
            "max_external_process_name": None,
            "max_external_total_cpu_pct": None,
        }

    top_process = max(status_rows, key=lambda row: row.cpu_pct)
    external_rows = [row for row in status_rows if row.command not in BENCHMARK_PROCESS_NAMES]
    top_external_process = max(external_rows, key=lambda row: row.cpu_pct, default=None)

    external_total_by_snapshot: dict[str, float] = {}
    for row in external_rows:
        external_total_by_snapshot[row.timestamp] = (
            external_total_by_snapshot.get(row.timestamp, 0.0) + row.cpu_pct
        )

    busy_snapshots = 0
    for timestamp in snapshots:
        external_process_cpu = max(
            (row.cpu_pct for row in external_rows if row.timestamp == timestamp),
            default=0.0,
        )
        external_total_cpu = external_total_by_snapshot.get(timestamp, 0.0)
        if (
            external_process_cpu >= busy_process_cpu_threshold
            or external_total_cpu >= busy_external_total_cpu_threshold
        ):
            busy_snapshots += 1

    return {
        "status": "BUSY" if busy_snapshots else "OK",
        "snapshots": len(snapshots),
        "sample_snapshots": len({row.timestamp for row in sample_rows}),
        "busy_snapshots": busy_snapshots,
        "busy_process_cpu_threshold_pct": busy_process_cpu_threshold,
        "busy_external_total_cpu_threshold_pct": busy_external_total_cpu_threshold,
        "max_top_process_cpu_pct": top_process.cpu_pct,
        "max_top_process_name": top_process.command,
        "max_external_process_cpu_pct": (
            top_external_process.cpu_pct if top_external_process else None
        ),
        "max_external_process_name": (
            top_external_process.command if top_external_process else None
        ),
        "max_external_total_cpu_pct": max(external_total_by_snapshot.values(), default=None),
    }


def coverage_pct(frames: int, expected_frames: int | None) -> float | None:
    if expected_frames is None or expected_frames <= 0:
        return None
    return frames * 100.0 / expected_frames


def time_span_seconds(rows: list[WindowRow]) -> float | None:
    timestamps = [
        parsed
        for parsed in (parse_iso_timestamp(row.timestamp) for row in rows)
        if parsed is not None
    ]
    if len(timestamps) < 2:
        return None
    return (max(timestamps) - min(timestamps)).total_seconds()


def time_coverage_pct(
    observed_span_seconds: float | None,
    requested_duration_seconds: float | None,
) -> float | None:
    if (
        observed_span_seconds is None
        or requested_duration_seconds is None
        or requested_duration_seconds <= 0
    ):
        return None
    return observed_span_seconds * 100.0 / requested_duration_seconds


def summarize_coverage(
    directory: Path,
    publisher_frames: int,
    subscriber_frames: int,
    subscriber_sink_frames: int,
    publisher_windows: list[WindowRow],
    subscriber_windows: list[WindowRow],
    subscriber_sink_windows: list[WindowRow],
    min_frame_coverage_pct: float,
    min_time_coverage_pct: float,
) -> dict[str, Any]:
    metadata = parse_metadata(directory)
    requested_duration_seconds = parse_float(metadata.get("duration_seconds"))
    requested_fps = parse_float(metadata.get("fps"))
    expected_frames = None
    if (
        requested_duration_seconds is not None
        and requested_duration_seconds > 0
        and requested_fps is not None
        and requested_fps > 0
    ):
        expected_frames = round(requested_duration_seconds * requested_fps)

    subscriber_delivery_frames = (
        subscriber_sink_frames if subscriber_sink_frames else subscriber_frames
    )
    publisher_coverage = coverage_pct(publisher_frames, expected_frames)
    subscriber_delivery_coverage = coverage_pct(
        subscriber_delivery_frames, expected_frames
    )
    subscriber_render_coverage = coverage_pct(subscriber_frames, expected_frames)
    headless = parse_bool(metadata.get("headless"))
    render_coverage_required = headless is False
    coverage_inputs = [publisher_coverage, subscriber_delivery_coverage]
    if render_coverage_required:
        coverage_inputs.append(subscriber_render_coverage)
    coverage_values = [value for value in coverage_inputs if value is not None]
    minimum_frame_coverage = min(coverage_values) if coverage_values else None

    subscriber_delivery_windows = (
        subscriber_sink_windows if subscriber_sink_windows else subscriber_windows
    )
    publisher_span_seconds = time_span_seconds(publisher_windows)
    subscriber_delivery_span_seconds = time_span_seconds(subscriber_delivery_windows)
    subscriber_render_span_seconds = time_span_seconds(subscriber_windows)
    publisher_time_coverage = time_coverage_pct(
        publisher_span_seconds, requested_duration_seconds
    )
    subscriber_delivery_time_coverage = time_coverage_pct(
        subscriber_delivery_span_seconds, requested_duration_seconds
    )
    subscriber_render_time_coverage = time_coverage_pct(
        subscriber_render_span_seconds, requested_duration_seconds
    )
    time_coverage_inputs = [publisher_time_coverage, subscriber_delivery_time_coverage]
    if render_coverage_required:
        time_coverage_inputs.append(subscriber_render_time_coverage)
    time_coverage_values = [value for value in time_coverage_inputs if value is not None]
    minimum_time_coverage = min(time_coverage_values) if time_coverage_values else None

    if expected_frames is None or requested_duration_seconds is None:
        status = "UNKNOWN"
    elif requested_duration_seconds < MIN_COVERAGE_DURATION_SECONDS:
        status = "SHORT_RUN"
    elif (
        minimum_frame_coverage is None
        or minimum_frame_coverage < min_frame_coverage_pct
    ):
        status = "LOW_FRAME_COVERAGE"
    elif (
        minimum_time_coverage is None
        or minimum_time_coverage < min_time_coverage_pct
    ):
        status = "LOW_TIME_COVERAGE"
    else:
        status = "OK"

    return {
        "status": status,
        "requested_duration_seconds": requested_duration_seconds,
        "requested_fps": requested_fps,
        "expected_frames": expected_frames,
        "publisher_frame_coverage_pct": publisher_coverage,
        "subscriber_delivery_frame_coverage_pct": subscriber_delivery_coverage,
        "subscriber_render_frame_coverage_pct": subscriber_render_coverage,
        "subscriber_render_coverage_required": render_coverage_required,
        "minimum_frame_coverage_pct": minimum_frame_coverage,
        "minimum_required_frame_coverage_pct": min_frame_coverage_pct,
        "publisher_observed_span_seconds": publisher_span_seconds,
        "subscriber_delivery_observed_span_seconds": subscriber_delivery_span_seconds,
        "subscriber_render_observed_span_seconds": subscriber_render_span_seconds,
        "publisher_time_coverage_pct": publisher_time_coverage,
        "subscriber_delivery_time_coverage_pct": subscriber_delivery_time_coverage,
        "subscriber_render_time_coverage_pct": subscriber_render_time_coverage,
        "minimum_time_coverage_pct": minimum_time_coverage,
        "minimum_required_time_coverage_pct": min_time_coverage_pct,
        "minimum_coverage_duration_seconds": MIN_COVERAGE_DURATION_SECONDS,
    }


def benchmark_status(
    smoothness_status: str,
    host_load_status: str,
    coverage_status: str,
    latency_budget_status: str,
) -> str:
    if smoothness_status == "INVALID":
        return "INVALID"
    if coverage_status in ("LOW_FRAME_COVERAGE", "LOW_TIME_COVERAGE"):
        base_status = "INCOMPLETE"
    elif coverage_status == "SHORT_RUN":
        base_status = f"{smoothness_status}_SHORT_RUN"
    elif coverage_status == "UNKNOWN":
        base_status = f"{smoothness_status}_COVERAGE_UNKNOWN"
    else:
        base_status = smoothness_status

    if base_status == "PASS":
        if latency_budget_status == "EXCEEDED":
            base_status = "LATENCY_BUDGET_EXCEEDED"
        elif latency_budget_status == "UNKNOWN":
            base_status = "LATENCY_BUDGET_UNKNOWN"

    if host_load_status == "BUSY":
        return f"{base_status}_HOST_BUSY"
    if host_load_status == "NA":
        return f"{base_status}_HOST_UNKNOWN"
    return base_status


def latency_budget_summary(
    summary: dict[str, Any], thresholds: dict[str, float]
) -> dict[str, Any]:
    observed: dict[str, float | None] = {}
    violations = []
    missing = []
    inapplicable = []
    render_coverage_required = summary.get("coverage", {}).get(
        "subscriber_render_coverage_required"
    )

    for key, threshold in thresholds.items():
        scope, metric, label = LATENCY_BUDGET_SPECS[key]
        if render_coverage_required is False and key in RENDER_LATENCY_BUDGET_KEYS:
            observed[key] = None
            inapplicable.append(
                {
                    "key": key,
                    "label": label,
                    "threshold_ms": threshold,
                    "reason": "headless run does not emit subscriber render latency windows",
                }
            )
            continue

        value = summary.get(scope, {}).get(metric)
        observed[key] = value
        if value is None:
            missing.append(
                {"key": key, "label": label, "threshold_ms": threshold}
            )
        elif value > threshold:
            violations.append(
                {
                    "key": key,
                    "label": label,
                    "observed_ms": value,
                    "threshold_ms": threshold,
                }
            )

    if not thresholds:
        status = "NOT_SET"
    elif violations:
        status = "EXCEEDED"
    elif missing:
        status = "UNKNOWN"
    else:
        status = "OK"

    return {
        "status": status,
        "thresholds_ms": thresholds,
        "observed_ms": observed,
        "violations": violations,
        "missing": missing,
        "inapplicable": inapplicable,
    }


def configured_latency_budgets(
    directory: Path, args: argparse.Namespace
) -> dict[str, float]:
    metadata = parse_metadata(directory)
    thresholds: dict[str, float] = {}
    for key in LATENCY_BUDGET_SPECS:
        value = getattr(args, key)
        if value is None:
            value = parse_float(metadata.get(key))
        if value is not None:
            thresholds[key] = value
    return thresholds


def configured_coverage_thresholds(
    directory: Path, args: argparse.Namespace
) -> dict[str, float]:
    metadata = parse_metadata(directory)
    min_frame_coverage_pct = args.min_frame_coverage_pct
    if min_frame_coverage_pct is None:
        min_frame_coverage_pct = parse_float(metadata.get("min_frame_coverage_pct"))
    if min_frame_coverage_pct is None:
        min_frame_coverage_pct = DEFAULT_MIN_FRAME_COVERAGE_PCT

    min_time_coverage_pct = args.min_time_coverage_pct
    if min_time_coverage_pct is None:
        min_time_coverage_pct = parse_float(metadata.get("min_time_coverage_pct"))
    if min_time_coverage_pct is None:
        min_time_coverage_pct = DEFAULT_MIN_TIME_COVERAGE_PCT

    return {
        "min_frame_coverage_pct": min_frame_coverage_pct,
        "min_time_coverage_pct": min_time_coverage_pct,
    }


def filter_warmup(
    windows: list[WindowRow],
    stutters: list[StutterRow],
    decode_health: list[DecodeHealthRow],
    jitter_buffers: list[JitterBufferRow],
    warmup_seconds: float,
) -> tuple[list[WindowRow], list[StutterRow], list[DecodeHealthRow], list[JitterBufferRow]]:
    if warmup_seconds <= 0:
        return windows, stutters, decode_health, jitter_buffers

    timestamps = [
        parsed
        for parsed in [parse_iso_timestamp(row.timestamp) for row in windows]
        + [parse_iso_timestamp(row.timestamp) for row in stutters]
        + [parse_iso_timestamp(row.timestamp) for row in decode_health]
        + [parse_iso_timestamp(row.timestamp) for row in jitter_buffers]
        if parsed is not None
    ]
    if not timestamps:
        return windows, stutters, decode_health, jitter_buffers

    cutoff = min(timestamps) + timedelta(seconds=warmup_seconds)
    return (
        [row for row in windows if (parse_iso_timestamp(row.timestamp) or cutoff) >= cutoff],
        [row for row in stutters if (parse_iso_timestamp(row.timestamp) or cutoff) >= cutoff],
        [row for row in decode_health if (parse_iso_timestamp(row.timestamp) or cutoff) >= cutoff],
        [row for row in jitter_buffers if (parse_iso_timestamp(row.timestamp) or cutoff) >= cutoff],
    )


def weighted_avg(rows: list[WindowRow], metric: str, field_name: str = "avg") -> float | None:
    total = 0.0
    weight = 0
    for row in rows:
        value = row.metrics.get(metric, {}).get(field_name)
        frames = row.frames or 0
        if value is None or frames == 0:
            continue
        total += value * frames
        weight += frames
    if weight == 0:
        return None
    return total / weight


def weighted_avg_by_value(
    rows: list[WindowRow], metric: str, weight_key: str, field_name: str = "avg"
) -> float | None:
    total = 0.0
    weight = 0
    for row in rows:
        value = row.metrics.get(metric, {}).get(field_name)
        row_weight = parse_int(row.values.get(weight_key)) or 0
        if value is None or row_weight == 0:
            continue
        total += value * row_weight
        weight += row_weight
    if weight == 0:
        return None
    return total / weight


def max_metric(rows: list[WindowRow], metric: str, field_name: str = "max") -> float | None:
    values = [
        row.metrics.get(metric, {}).get(field_name)
        for row in rows
        if row.metrics.get(metric, {}).get(field_name) is not None
    ]
    return max(values) if values else None


def sum_int_metric(rows: list[WindowRow], metric: str) -> int:
    total = 0
    for row in rows:
        try:
            total += int(row.values.get(metric, "0"))
        except ValueError:
            pass
    return total


def percentile(values: list[float], pct: float) -> float | None:
    if not values:
        return None
    sorted_values = sorted(values)
    index = math.ceil(pct * len(sorted_values)) - 1
    return sorted_values[max(0, min(index, len(sorted_values) - 1))]


def p95(values: list[float]) -> float | None:
    return percentile(values, 0.95)


def percentile_metric(
    rows: list[WindowRow],
    metric: str,
    field_name: str = "max",
    pct: float = 0.95,
) -> float | None:
    values = [
        value
        for row in rows
        if (value := row.metrics.get(metric, {}).get(field_name)) is not None
    ]
    return percentile(values, pct)


def worst_windows(
    rows: list[WindowRow],
    metric: str,
    label: str,
    limit: int = 5,
) -> list[dict[str, Any]]:
    windows = []
    for row in rows:
        values = row.metrics.get(metric)
        if not values or values.get("max") is None:
            continue
        windows.append(
            {
                "label": label,
                "source": row.source,
                "metric": metric,
                "timestamp": row.timestamp,
                "frames": row.frames,
                "stutters_over_threshold": row.stutters_over_threshold,
                "avg_ms": values.get("avg"),
                "min_ms": values.get("min"),
                "max_ms": values.get("max"),
            }
        )

    windows.sort(key=lambda row: row["max_ms"], reverse=True)
    return windows[:limit]


def smoothness_signal_distribution(
    subscriber_windows: list[WindowRow],
    subscriber_sink_windows: list[WindowRow],
    stutters: list[StutterRow],
    include_render_signals: bool,
) -> dict[str, Any]:
    signal_rows: list[tuple[datetime, str, int]] = []
    detail_log_timestamps = []
    ignored_detail_log_rows = 0
    if include_render_signals:
        detail_log_timestamps = [
            timestamp
            for timestamp in (parse_iso_timestamp(row.timestamp) for row in stutters)
            if timestamp is not None
        ]
    else:
        ignored_detail_log_rows = len(stutters)
    observed_windows = subscriber_sink_windows + (
        subscriber_windows if include_render_signals else []
    )
    all_timestamps = [
        parsed
        for parsed in [
            parse_iso_timestamp(row.timestamp)
            for row in observed_windows
        ]
        if parsed is not None
    ]

    for row in subscriber_sink_windows:
        signal_count = row.stutters_over_threshold or 0
        if include_render_signals:
            signal_count += parse_int(row.values.get("replaced_before_render")) or 0
            signal_count += parse_int(row.values.get("dropped_late_before_render")) or 0
        timestamp = parse_iso_timestamp(row.timestamp)
        if signal_count > 0 and timestamp is not None:
            signal_rows.append((timestamp, "subscriber_sink", signal_count))

    if include_render_signals:
        for row in subscriber_windows:
            signal_count = row.stutters_over_threshold or 0
            timestamp = parse_iso_timestamp(row.timestamp)
            if signal_count > 0 and timestamp is not None:
                signal_rows.append((timestamp, "subscriber_render", signal_count))

    signal_rows.sort(key=lambda row: row[0])
    detail_log_timestamps.sort()
    source_counts: dict[str, int] = {}
    for _, source, count in signal_rows:
        source_counts[source] = source_counts.get(source, 0) + count

    first_signal = signal_rows[0][0] if signal_rows else None
    last_signal = signal_rows[-1][0] if signal_rows else None
    first_observed = min(all_timestamps) if all_timestamps else None
    last_observed = max(all_timestamps) if all_timestamps else None

    def seconds_between(start: datetime | None, end: datetime | None) -> float | None:
        if start is None or end is None:
            return None
        return (end - start).total_seconds()

    return {
        "smoothness_signal_log_rows": len(signal_rows),
        "smoothness_detail_log_rows": len(detail_log_timestamps),
        "smoothness_detail_log_rows_ignored": ignored_detail_log_rows,
        "smoothness_signal_windows": len(
            {(timestamp, source) for timestamp, source, _ in signal_rows}
        ),
        "smoothness_signal_sources": dict(sorted(source_counts.items())),
        "first_smoothness_detail_timestamp": (
            detail_log_timestamps[0].isoformat().replace("+00:00", "Z")
            if detail_log_timestamps
            else None
        ),
        "last_smoothness_detail_timestamp": (
            detail_log_timestamps[-1].isoformat().replace("+00:00", "Z")
            if detail_log_timestamps
            else None
        ),
        "first_smoothness_signal_timestamp": (
            first_signal.isoformat().replace("+00:00", "Z") if first_signal else None
        ),
        "last_smoothness_signal_timestamp": (
            last_signal.isoformat().replace("+00:00", "Z") if last_signal else None
        ),
        "first_smoothness_signal_offset_seconds": seconds_between(
            first_observed, first_signal
        ),
        "last_smoothness_signal_offset_seconds": seconds_between(first_observed, last_signal),
        "smoothness_signal_span_seconds": seconds_between(first_signal, last_signal),
        "post_signal_clean_tail_seconds": seconds_between(last_signal, last_observed),
    }


def max_optional(values: list[float | int | None]) -> float | int | None:
    present = [value for value in values if value is not None]
    return max(present) if present else None


def fmt_ms(value: float | None) -> str:
    if value is None:
        return "NA"
    return f"{value:.1f}ms"


def fmt_pct(value: float | None) -> str:
    if value is None:
        return "NA"
    return f"{value:.1f}%"


def fmt_value(value: Any, suffix: str = "") -> str:
    if value is None:
        return "NA"
    return f"{value}{suffix}"


def summarize(
    name: str,
    directory: Path,
    warmup_seconds: float,
    coverage_thresholds: dict[str, float],
    latency_budget_thresholds: dict[str, float],
    windows: list[WindowRow],
    stutters: list[StutterRow],
    health: dict[str, list[str]],
    decode_health: list[DecodeHealthRow],
    jitter_buffers: list[JitterBufferRow],
) -> dict[str, Any]:
    subscriber_windows = [row for row in windows if row.source == "subscriber"]
    subscriber_sink_windows = [row for row in windows if row.source == "subscriber_sink"]
    subscriber_render_loop_windows = [
        row for row in windows if row.source == "subscriber_render_loop"
    ]
    publisher_windows = [row for row in windows if row.source == "publisher"]
    subscriber_frames = sum(row.frames or 0 for row in subscriber_windows)
    subscriber_sink_frames = sum(row.frames or 0 for row in subscriber_sink_windows)
    publisher_frames = sum(row.frames or 0 for row in publisher_windows)
    invalid_reasons = []
    if not subscriber_windows and not subscriber_sink_windows:
        invalid_reasons.append("no subscriber render or sink latency windows")
    if subscriber_frames == 0 and subscriber_sink_frames == 0:
        invalid_reasons.append("no subscriber render or sink frames")
    if not publisher_windows:
        invalid_reasons.append("no publisher frame latency windows")
    if publisher_frames == 0:
        invalid_reasons.append("no publisher frames")

    dominant_stage_counts: dict[str, int] = {}
    stutter_paint_gaps = []
    skipped_frames = 0
    for row in stutters:
        if row.dominant_stage:
            dominant_stage_counts[row.dominant_stage] = dominant_stage_counts.get(row.dominant_stage, 0) + 1
        paint_gap = parse_ms(row.values.get("paint_gap", "NA"))
        if paint_gap is not None:
            stutter_paint_gaps.append(paint_gap)
        try:
            skipped_frames += int(row.values.get("skipped_frame_count", "0"))
        except ValueError:
            pass

    last_decode_health = decode_health[-1] if decode_health else None
    decoders = sorted({row.decoder for row in decode_health if row.decoder})
    receive_decode_backlog_max = max_optional(
        [row.receive_decode_backlog for row in decode_health]
    )

    summary = {
        "name": name,
        "directory": str(directory),
        "analysis_schema_version": ANALYSIS_SCHEMA_VERSION,
        "generated_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "warmup_seconds_excluded": warmup_seconds,
        "valid": not invalid_reasons,
        "invalid_reasons": invalid_reasons,
        "host_load": summarize_host_load(directory),
        "coverage": summarize_coverage(
            directory,
            publisher_frames,
            subscriber_frames,
            subscriber_sink_frames,
            publisher_windows,
            subscriber_windows,
            subscriber_sink_windows,
            coverage_thresholds["min_frame_coverage_pct"],
            coverage_thresholds["min_time_coverage_pct"],
        ),
        "subscriber": {
            "windows": len(subscriber_windows),
            "frames": subscriber_frames,
            "sink_windows": len(subscriber_sink_windows),
            "sink_frames": subscriber_sink_frames,
            "sink_gap_avg_ms": weighted_avg(subscriber_sink_windows, "sink_gap"),
            "sink_gap_max_ms": max_metric(subscriber_sink_windows, "sink_gap"),
            "sink_gap_p95_window_max_ms": percentile_metric(
                subscriber_sink_windows, "sink_gap"
            ),
            "sink_stutters_over_threshold": sum(
                row.stutters_over_threshold or 0 for row in subscriber_sink_windows
            ),
            "sink_dropped_before_store": sum_int_metric(subscriber_sink_windows, "dropped_before_store"),
            "sink_replaced_before_render": sum_int_metric(subscriber_sink_windows, "replaced_before_render"),
            "sink_dropped_late_before_render": sum_int_metric(
                subscriber_sink_windows, "dropped_late_before_render"
            ),
            "sink_late_drop_detail_logs_suppressed": sum_int_metric(
                subscriber_sink_windows, "late_drop_detail_logs_suppressed"
            ),
            "window_stutters_over_threshold": sum(
                row.stutters_over_threshold or 0 for row in subscriber_windows
            ),
            "stutter_lines": len(stutters),
            "skipped_frames_on_stutters": skipped_frames,
            "e2e_avg_ms": weighted_avg(subscriber_windows, "e2e"),
            "e2e_max_ms": max_metric(subscriber_windows, "e2e"),
            "e2e_p95_window_max_ms": percentile_metric(subscriber_windows, "e2e"),
            "receive_to_decode_avg_ms": weighted_avg(subscriber_windows, "receive_to_decode"),
            "receive_to_decode_max_ms": max_metric(subscriber_windows, "receive_to_decode"),
            "receive_to_decode_p95_window_max_ms": percentile_metric(
                subscriber_windows, "receive_to_decode"
            ),
            "receive_to_paint_avg_ms": weighted_avg(subscriber_windows, "receive_to_paint"),
            "receive_to_paint_max_ms": max_metric(subscriber_windows, "receive_to_paint"),
            "receive_to_paint_p95_window_max_ms": percentile_metric(
                subscriber_windows, "receive_to_paint"
            ),
            "paint_gap_avg_ms": weighted_avg(subscriber_windows, "paint_gap"),
            "paint_gap_max_ms": max_metric(subscriber_windows, "paint_gap"),
            "paint_gap_p95_window_max_ms": percentile_metric(
                subscriber_windows, "paint_gap"
            ),
            "stutter_paint_gap_p95_ms": p95(stutter_paint_gaps),
            "dominant_stutter_stages": dict(
                sorted(dominant_stage_counts.items(), key=lambda item: item[1], reverse=True)
            ),
            "decode_health_samples": len(decode_health),
            "decoder_implementations": decoders,
            "receive_decode_backlog_last_frames": (
                last_decode_health.receive_decode_backlog if last_decode_health else None
            ),
            "receive_decode_backlog_max_frames": receive_decode_backlog_max,
            "frames_received_last": last_decode_health.received if last_decode_health else None,
            "frames_decoded_last": last_decode_health.decoded if last_decode_health else None,
            "frames_dropped_last": last_decode_health.dropped if last_decode_health else None,
            "decode_time_last_s": last_decode_health.decode_time_s if last_decode_health else None,
            "jitter_buffer_samples": len(jitter_buffers),
            "jitter_delay_window_avg_max_ms": max_optional(
                [row.delay_window_avg_ms for row in jitter_buffers]
            ),
            "jitter_delay_avg_max_ms": max_optional([row.delay_avg_ms for row in jitter_buffers]),
            "jitter_target_avg_max_ms": max_optional([row.target_avg_ms for row in jitter_buffers]),
            "jitter_minimum_avg_max_ms": max_optional(
                [row.minimum_avg_ms for row in jitter_buffers]
            ),
            "jitter_emitted_last": jitter_buffers[-1].emitted if jitter_buffers else None,
            "render_loop_windows": len(subscriber_render_loop_windows),
            "render_loop_updates": sum_int_metric(subscriber_render_loop_windows, "updates"),
            "render_loop_updates_with_frame": sum_int_metric(
                subscriber_render_loop_windows, "updates_with_frame"
            ),
            "render_loop_updates_without_frame": sum_int_metric(
                subscriber_render_loop_windows, "updates_without_frame"
            ),
            "render_loop_update_gap_avg_ms": weighted_avg_by_value(
                subscriber_render_loop_windows, "update_gap", "updates"
            ),
            "render_loop_update_gap_max_ms": max_metric(
                subscriber_render_loop_windows, "update_gap"
            ),
            "render_loop_update_gap_p95_window_max_ms": percentile_metric(
                subscriber_render_loop_windows, "update_gap"
            ),
            "render_loop_prepares": sum_int_metric(subscriber_render_loop_windows, "prepares"),
            "render_loop_prepares_no_dims": sum_int_metric(
                subscriber_render_loop_windows, "prepares_no_dims"
            ),
            "render_loop_prepares_without_frame": sum_int_metric(
                subscriber_render_loop_windows, "prepares_without_frame"
            ),
            "render_loop_prepares_native": sum_int_metric(
                subscriber_render_loop_windows, "prepares_native"
            ),
            "render_loop_prepares_cpu": sum_int_metric(
                subscriber_render_loop_windows, "prepares_cpu"
            ),
            "render_loop_prepare_duration_avg_ms": weighted_avg_by_value(
                subscriber_render_loop_windows, "prepare_duration", "prepares"
            ),
            "render_loop_prepare_duration_max_ms": max_metric(
                subscriber_render_loop_windows, "prepare_duration"
            ),
            "render_loop_prepare_duration_p95_window_max_ms": percentile_metric(
                subscriber_render_loop_windows, "prepare_duration"
            ),
            "render_loop_paints": sum_int_metric(subscriber_render_loop_windows, "paints"),
            "render_loop_paints_with_sample": sum_int_metric(
                subscriber_render_loop_windows, "paints_with_sample"
            ),
            "render_loop_paints_without_sample": sum_int_metric(
                subscriber_render_loop_windows, "paints_without_sample"
            ),
            "render_loop_paint_gap_avg_ms": weighted_avg_by_value(
                subscriber_render_loop_windows, "paint_gap", "paints"
            ),
            "render_loop_paint_gap_max_ms": max_metric(
                subscriber_render_loop_windows, "paint_gap"
            ),
            "render_loop_paint_gap_p95_window_max_ms": percentile_metric(
                subscriber_render_loop_windows, "paint_gap"
            ),
            "render_loop_stutters_over_threshold": sum(
                row.stutters_over_threshold or 0 for row in subscriber_render_loop_windows
            ),
        },
        "publisher": {
            "windows": len(publisher_windows),
            "frames": publisher_frames,
            "capture_gap_avg_ms": weighted_avg(publisher_windows, "capture_gap"),
            "capture_gap_max_ms": max_metric(publisher_windows, "capture_gap"),
            "capture_gap_p95_window_max_ms": percentile_metric(
                publisher_windows, "capture_gap"
            ),
            "capture_to_packetize_avg_ms": weighted_avg(publisher_windows, "capture_to_packetize"),
            "capture_to_packetize_max_ms": max_metric(publisher_windows, "capture_to_packetize"),
            "capture_to_packetize_p95_window_max_ms": percentile_metric(
                publisher_windows, "capture_to_packetize"
            ),
            "encoder_upload_to_output_avg_ms": weighted_avg(publisher_windows, "encoder_upload_to_output"),
            "encoder_upload_to_output_max_ms": max_metric(publisher_windows, "encoder_upload_to_output"),
            "encoder_upload_to_output_p95_window_max_ms": percentile_metric(
                publisher_windows, "encoder_upload_to_output"
            ),
            "health_lines": len(health["publisher"]),
        },
        "worst_windows": {
            "subscriber_sink_gap": worst_windows(
                subscriber_sink_windows, "sink_gap", "subscriber sink_gap"
            ),
            "subscriber_e2e": worst_windows(
                subscriber_windows, "e2e", "subscriber e2e"
            ),
            "subscriber_paint_gap": worst_windows(
                subscriber_windows, "paint_gap", "subscriber paint_gap"
            ),
            "subscriber_receive_to_decode": worst_windows(
                subscriber_windows,
                "receive_to_decode",
                "subscriber receive_to_decode",
            ),
            "subscriber_receive_to_paint": worst_windows(
                subscriber_windows,
                "receive_to_paint",
                "subscriber receive_to_paint",
            ),
            "render_loop_update_gap": worst_windows(
                subscriber_render_loop_windows,
                "update_gap",
                "render loop update_gap",
            ),
            "render_loop_paint_gap": worst_windows(
                subscriber_render_loop_windows,
                "paint_gap",
                "render loop paint_gap",
            ),
            "publisher_capture_gap": worst_windows(
                publisher_windows, "capture_gap", "publisher capture_gap"
            ),
            "publisher_capture_to_packetize": worst_windows(
                publisher_windows,
                "capture_to_packetize",
                "publisher capture_to_packetize",
            ),
            "publisher_encoder_upload_to_output": worst_windows(
                publisher_windows,
                "encoder_upload_to_output",
                "publisher encoder_upload_to_output",
            ),
        },
    }
    subscriber = summary["subscriber"]
    include_render_signals = bool(
        summary["coverage"]["subscriber_render_coverage_required"]
    )
    render_stutter_signals = (
        max(subscriber["window_stutters_over_threshold"], subscriber["stutter_lines"])
        if include_render_signals
        else 0
    )
    raw_visual_frame_drop_signals = (
        subscriber["sink_replaced_before_render"]
        + subscriber["sink_dropped_late_before_render"]
    )
    subscriber["total_stutters_over_threshold"] = (
        subscriber["sink_stutters_over_threshold"] + render_stutter_signals
    )
    subscriber["render_stutter_signals"] = render_stutter_signals
    subscriber["visual_frame_drop_signals_counted"] = include_render_signals
    subscriber["visual_frame_drop_signals_raw"] = raw_visual_frame_drop_signals
    subscriber["visual_frame_drop_signals"] = (
        raw_visual_frame_drop_signals if include_render_signals else 0
    )
    subscriber["visual_frame_drop_signals_ignored"] = (
        0 if include_render_signals else raw_visual_frame_drop_signals
    )
    subscriber["total_smoothness_signals"] = (
        subscriber["total_stutters_over_threshold"]
        + subscriber["visual_frame_drop_signals"]
    )
    subscriber.update(
        smoothness_signal_distribution(
            subscriber_windows,
            subscriber_sink_windows,
            stutters,
            include_render_signals,
        )
    )
    if not summary["valid"]:
        summary["smoothness_status"] = "INVALID"
    elif subscriber["total_smoothness_signals"] == 0:
        summary["smoothness_status"] = "PASS"
    else:
        summary["smoothness_status"] = "STUTTERS_DETECTED"
    summary["latency_budget"] = latency_budget_summary(
        summary, latency_budget_thresholds
    )
    summary["benchmark_status"] = benchmark_status(
        summary["smoothness_status"],
        summary["host_load"]["status"],
        summary["coverage"]["status"],
        summary["latency_budget"]["status"],
    )
    return summary


def write_timeseries_csv(path: Path, windows: list[WindowRow]) -> None:
    metrics = sorted({metric for row in windows for metric in row.metrics})
    fields = ["timestamp", "source", "frames", "stutters_over_threshold"]
    for metric in metrics:
        fields.extend([f"{metric}_avg_ms", f"{metric}_min_ms", f"{metric}_max_ms"])

    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in windows:
            output: dict[str, Any] = {
                "timestamp": row.timestamp,
                "source": row.source,
                "frames": row.frames,
                "stutters_over_threshold": row.stutters_over_threshold,
            }
            for metric in metrics:
                values = row.metrics.get(metric, {})
                output[f"{metric}_avg_ms"] = values.get("avg")
                output[f"{metric}_min_ms"] = values.get("min")
                output[f"{metric}_max_ms"] = values.get("max")
            writer.writerow(output)


def write_stutters_csv(path: Path, stutters: list[StutterRow]) -> None:
    keys = sorted({key for row in stutters for key in row.values})
    fields = ["timestamp", "dominant_stage"] + keys
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in stutters:
            output: dict[str, Any] = {"timestamp": row.timestamp, "dominant_stage": row.dominant_stage}
            output.update(row.values)
            writer.writerow(output)


def write_receiver_stats_csv(
    path: Path,
    decode_health: list[DecodeHealthRow],
    jitter_buffers: list[JitterBufferRow],
) -> None:
    fields = [
        "timestamp",
        "type",
        "received",
        "decoded",
        "receive_decode_backlog",
        "rendered",
        "dropped",
        "decode_time_s",
        "decoder",
        "delay_window_avg_ms",
        "delay_avg_ms",
        "target_avg_ms",
        "minimum_avg_ms",
        "emitted",
    ]
    rows: list[dict[str, Any]] = []
    for row in decode_health:
        rows.append(
            {
                "timestamp": row.timestamp,
                "type": "decode_health",
                "received": row.received,
                "decoded": row.decoded,
                "receive_decode_backlog": row.receive_decode_backlog,
                "rendered": row.rendered,
                "dropped": row.dropped,
                "decode_time_s": row.decode_time_s,
                "decoder": row.decoder,
            }
        )
    for row in jitter_buffers:
        rows.append(
            {
                "timestamp": row.timestamp,
                "type": "jitter_buffer",
                "delay_window_avg_ms": row.delay_window_avg_ms,
                "delay_avg_ms": row.delay_avg_ms,
                "target_avg_ms": row.target_avg_ms,
                "minimum_avg_ms": row.minimum_avg_ms,
                "emitted": row.emitted,
            }
        )

    rows.sort(key=lambda row: row["timestamp"])
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)


def write_worst_windows_csv(path: Path, summary: dict[str, Any]) -> None:
    fields = [
        "label",
        "source",
        "metric",
        "timestamp",
        "frames",
        "stutters_over_threshold",
        "avg_ms",
        "min_ms",
        "max_ms",
    ]
    rows = [
        row
        for metric_windows in summary["worst_windows"].values()
        for row in metric_windows
    ]
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)


def report_lines(summary: dict[str, Any]) -> list[str]:
    subscriber = summary["subscriber"]
    publisher = summary["publisher"]
    host_load = summary["host_load"]
    coverage = summary["coverage"]
    latency_budget = summary["latency_budget"]
    stages = subscriber["dominant_stutter_stages"]
    benchmark = summary["benchmark_status"].replace("_", " ")
    status = summary["smoothness_status"].replace("_", " ")

    lines = [
        f"# local_video latency report: {summary['name']}",
        "",
        f"- Generated: {summary['generated_at']}",
        f"- Directory: {summary['directory']}",
        f"- Warmup excluded: {summary['warmup_seconds_excluded']}s",
        f"- Benchmark status: {benchmark}",
        f"- Smoothness status: {status}",
        f"- Coverage status: {coverage['status'].replace('_', ' ')}",
        f"- Host load status: {host_load['status']}",
        f"- Latency budget status: {latency_budget['status'].replace('_', ' ')}",
    ]
    if summary["invalid_reasons"]:
        lines.append(f"- Invalid reasons: {', '.join(summary['invalid_reasons'])}")
    if latency_budget["violations"]:
        lines.append(
            "- Latency budget violations: "
            + ", ".join(
                (
                    f"{violation['label']} "
                    f"{fmt_ms(violation['observed_ms'])}>{fmt_ms(violation['threshold_ms'])}"
                )
                for violation in latency_budget["violations"]
            )
        )
    if latency_budget["missing"]:
        lines.append(
            "- Latency budget missing metrics: "
            + ", ".join(item["label"] for item in latency_budget["missing"])
        )
    if latency_budget["inapplicable"]:
        lines.append(
            "- Latency budget inapplicable metrics: "
            + ", ".join(item["label"] for item in latency_budget["inapplicable"])
        )
    lines.extend(
        [
            "",
            "## Subscriber",
            "",
            f"- Windows: {subscriber['windows']}",
            f"- Frames: {subscriber['frames']}",
            f"- Sink delivery windows: {subscriber['sink_windows']}",
            f"- Sink delivery frames: {subscriber['sink_frames']}",
            (
                "- sink_gap avg / max / stutters: "
                f"{fmt_ms(subscriber['sink_gap_avg_ms'])} / "
                f"{fmt_ms(subscriber['sink_gap_max_ms'])} / "
                f"{subscriber['sink_stutters_over_threshold']}"
            ),
            f"- sink_gap p95 window max: {fmt_ms(subscriber['sink_gap_p95_window_max_ms'])}",
            (
                "- frame coverage publisher / subscriber delivery / render: "
                f"{fmt_pct(coverage['publisher_frame_coverage_pct'])} / "
                f"{fmt_pct(coverage['subscriber_delivery_frame_coverage_pct'])} / "
                f"{fmt_pct(coverage['subscriber_render_frame_coverage_pct'])}"
            ),
            f"- Render coverage required: {coverage['subscriber_render_coverage_required']}",
            (
                "- time coverage publisher / subscriber delivery / render: "
                f"{fmt_pct(coverage['publisher_time_coverage_pct'])} / "
                f"{fmt_pct(coverage['subscriber_delivery_time_coverage_pct'])} / "
                f"{fmt_pct(coverage['subscriber_render_time_coverage_pct'])}"
            ),
            (
                "- observed span publisher / subscriber delivery / render: "
                f"{fmt_value(coverage['publisher_observed_span_seconds'], 's')} / "
                f"{fmt_value(coverage['subscriber_delivery_observed_span_seconds'], 's')} / "
                f"{fmt_value(coverage['subscriber_render_observed_span_seconds'], 's')}"
            ),
            (
                "- requested duration / fps / expected frames: "
                f"{fmt_value(coverage['requested_duration_seconds'], 's')} / "
                f"{fmt_value(coverage['requested_fps'])} / "
                f"{fmt_value(coverage['expected_frames'])}"
            ),
            f"- Sink frames drained before store: {subscriber['sink_dropped_before_store']}",
            f"- Pending render frames replaced before paint: {subscriber['sink_replaced_before_render']}",
            f"- Late decoded frames dropped before render: {subscriber['sink_dropped_late_before_render']}",
            f"- Window stutters over threshold: {subscriber['window_stutters_over_threshold']}",
            f"- Stutter warning lines: {subscriber['stutter_lines']}",
            f"- Render stutter signals counted: {subscriber['render_stutter_signals']}",
            f"- Total stutter signals: {subscriber['total_stutters_over_threshold']}",
            f"- Visual frame drop signals before paint: {subscriber['visual_frame_drop_signals']}",
            f"- Visual frame drop signals ignored for headless: {subscriber['visual_frame_drop_signals_ignored']}",
            f"- Total smoothness signals: {subscriber['total_smoothness_signals']}",
            f"- Smoothness signal windows: {subscriber['smoothness_signal_windows']}",
            f"- Smoothness detail log rows: {subscriber['smoothness_detail_log_rows']}",
            f"- Smoothness detail log rows ignored for headless: {subscriber['smoothness_detail_log_rows_ignored']}",
            (
                "- First / last smoothness signal offset: "
                f"{fmt_value(subscriber['first_smoothness_signal_offset_seconds'], 's')} / "
                f"{fmt_value(subscriber['last_smoothness_signal_offset_seconds'], 's')}"
            ),
            f"- Clean tail after last smoothness signal: {fmt_value(subscriber['post_signal_clean_tail_seconds'], 's')}",
            f"- Skipped frames on stutter warnings: {subscriber['skipped_frames_on_stutters']}",
            f"- e2e avg / max: {fmt_ms(subscriber['e2e_avg_ms'])} / {fmt_ms(subscriber['e2e_max_ms'])}",
            f"- e2e p95 window max: {fmt_ms(subscriber['e2e_p95_window_max_ms'])}",
            (
                "- receive_to_decode avg / max: "
                f"{fmt_ms(subscriber['receive_to_decode_avg_ms'])} / "
                f"{fmt_ms(subscriber['receive_to_decode_max_ms'])}"
            ),
            (
                "- receive_to_decode / receive_to_paint p95 window max: "
                f"{fmt_ms(subscriber['receive_to_decode_p95_window_max_ms'])} / "
                f"{fmt_ms(subscriber['receive_to_paint_p95_window_max_ms'])}"
            ),
            (
                "- receive_to_paint avg / max: "
                f"{fmt_ms(subscriber['receive_to_paint_avg_ms'])} / "
                f"{fmt_ms(subscriber['receive_to_paint_max_ms'])}"
            ),
            (
                "- paint_gap avg / max / stutter p95: "
                f"{fmt_ms(subscriber['paint_gap_avg_ms'])} / "
                f"{fmt_ms(subscriber['paint_gap_max_ms'])} / "
                f"{fmt_ms(subscriber['stutter_paint_gap_p95_ms'])}"
            ),
            f"- paint_gap p95 window max: {fmt_ms(subscriber['paint_gap_p95_window_max_ms'])}",
            (
                "- decode health samples / decoder: "
                f"{subscriber['decode_health_samples']} / "
                f"{', '.join(subscriber['decoder_implementations']) or 'NA'}"
            ),
            (
                "- received / decoded / dropped last: "
                f"{fmt_value(subscriber['frames_received_last'])} / "
                f"{fmt_value(subscriber['frames_decoded_last'])} / "
                f"{fmt_value(subscriber['frames_dropped_last'])}"
            ),
            (
                "- receive-decode backlog last / max: "
                f"{fmt_value(subscriber['receive_decode_backlog_last_frames'])} / "
                f"{fmt_value(subscriber['receive_decode_backlog_max_frames'], ' frames')}"
            ),
            f"- total decode time last: {fmt_value(subscriber['decode_time_last_s'], 's')}",
            (
                "- jitter delay window max / cumulative max: "
                f"{fmt_ms(subscriber['jitter_delay_window_avg_max_ms'])} / "
                f"{fmt_ms(subscriber['jitter_delay_avg_max_ms'])}"
            ),
            (
                "- jitter target max / minimum max / emitted last: "
                f"{fmt_ms(subscriber['jitter_target_avg_max_ms'])} / "
                f"{fmt_ms(subscriber['jitter_minimum_avg_max_ms'])} / "
                f"{fmt_value(subscriber['jitter_emitted_last'])}"
            ),
            (
                "- render loop windows / updates / paints: "
                f"{subscriber['render_loop_windows']} / "
                f"{subscriber['render_loop_updates']} / "
                f"{subscriber['render_loop_paints']}"
            ),
            (
                "- render loop update_gap avg / max: "
                f"{fmt_ms(subscriber['render_loop_update_gap_avg_ms'])} / "
                f"{fmt_ms(subscriber['render_loop_update_gap_max_ms'])}"
            ),
            (
                "- render loop update_gap p95 window max: "
                f"{fmt_ms(subscriber['render_loop_update_gap_p95_window_max_ms'])}"
            ),
            (
                "- render loop prepare_duration avg / max: "
                f"{fmt_ms(subscriber['render_loop_prepare_duration_avg_ms'])} / "
                f"{fmt_ms(subscriber['render_loop_prepare_duration_max_ms'])}"
            ),
            (
                "- render loop prepare_duration p95 window max: "
                f"{fmt_ms(subscriber['render_loop_prepare_duration_p95_window_max_ms'])}"
            ),
            (
                "- render loop paint_gap avg / max / stutters: "
                f"{fmt_ms(subscriber['render_loop_paint_gap_avg_ms'])} / "
                f"{fmt_ms(subscriber['render_loop_paint_gap_max_ms'])} / "
                f"{subscriber['render_loop_stutters_over_threshold']}"
            ),
            (
                "- render loop paint_gap p95 window max: "
                f"{fmt_ms(subscriber['render_loop_paint_gap_p95_window_max_ms'])}"
            ),
            (
                "- render loop prepare paths native / cpu / no-frame: "
                f"{subscriber['render_loop_prepares_native']} / "
                f"{subscriber['render_loop_prepares_cpu']} / "
                f"{subscriber['render_loop_prepares_without_frame']}"
            ),
            "",
            "## Dominant Stutter Stages",
            "",
        ]
    )
    if stages:
        for stage, count in stages.items():
            lines.append(f"- {stage}: {count}")
    else:
        lines.append("- none")

    lines.extend(["", "## Worst Windows", ""])
    worst_groups = summary["worst_windows"]
    worst_group_order = (
        "subscriber_sink_gap",
        "subscriber_paint_gap",
        "subscriber_e2e",
        "subscriber_receive_to_decode",
        "subscriber_receive_to_paint",
        "render_loop_update_gap",
        "render_loop_paint_gap",
        "publisher_capture_gap",
        "publisher_capture_to_packetize",
        "publisher_encoder_upload_to_output",
    )
    any_worst_windows = False
    for group_name in worst_group_order:
        windows = worst_groups.get(group_name, [])[:3]
        if not windows:
            continue
        any_worst_windows = True
        lines.append(f"- {windows[0]['label']}:")
        for window in windows:
            lines.append(
                "  - "
                f"{window['timestamp'] or 'unknown timestamp'}: "
                f"max={fmt_ms(window['max_ms'])}, "
                f"avg={fmt_ms(window['avg_ms'])}, "
                f"frames={fmt_value(window['frames'])}, "
                f"stutters={fmt_value(window['stutters_over_threshold'])}"
            )
    if not any_worst_windows:
        lines.append("- none")

    lines.extend(
        [
            "",
            "## Host Load",
            "",
            f"- Status: {host_load['status']}",
            (
                "- Run sample snapshots / busy snapshots: "
                f"{host_load['sample_snapshots']} / {host_load['busy_snapshots']}"
            ),
            f"- Status snapshots considered: {host_load['snapshots']}",
            (
                "- Busiest process: "
                f"{host_load['max_top_process_name'] or 'NA'} "
                f"({fmt_pct(host_load['max_top_process_cpu_pct'])})"
            ),
            (
                "- Busiest external process: "
                f"{host_load['max_external_process_name'] or 'NA'} "
                f"({fmt_pct(host_load['max_external_process_cpu_pct'])})"
            ),
            (
                "- Peak external top-process / total CPU: "
                f"{fmt_pct(host_load['max_external_process_cpu_pct'])} / "
                f"{fmt_pct(host_load['max_external_total_cpu_pct'])}"
            ),
            (
                "- Busy thresholds process / external total CPU: "
                f"{fmt_pct(host_load['busy_process_cpu_threshold_pct'])} / "
                f"{fmt_pct(host_load['busy_external_total_cpu_threshold_pct'])}"
            ),
            "",
            "## Publisher",
            "",
            f"- Windows: {publisher['windows']}",
            f"- Frames: {publisher['frames']}",
            (
                "- capture_gap avg / max: "
                f"{fmt_ms(publisher['capture_gap_avg_ms'])} / "
                f"{fmt_ms(publisher['capture_gap_max_ms'])}"
            ),
            f"- capture_gap p95 window max: {fmt_ms(publisher['capture_gap_p95_window_max_ms'])}",
            (
                "- capture_to_packetize avg / max: "
                f"{fmt_ms(publisher['capture_to_packetize_avg_ms'])} / "
                f"{fmt_ms(publisher['capture_to_packetize_max_ms'])}"
            ),
            (
                "- capture_to_packetize p95 window max: "
                f"{fmt_ms(publisher['capture_to_packetize_p95_window_max_ms'])}"
            ),
            (
                "- encoder_upload_to_output avg / max: "
                f"{fmt_ms(publisher['encoder_upload_to_output_avg_ms'])} / "
                f"{fmt_ms(publisher['encoder_upload_to_output_max_ms'])}"
            ),
            (
                "- encoder_upload_to_output p95 window max: "
                f"{fmt_ms(publisher['encoder_upload_to_output_p95_window_max_ms'])}"
            ),
            "",
            "## Artifacts",
            "",
            "- latency-timeseries.csv",
            "- stutters.csv",
            "- receiver-stats.csv",
            "- worst-windows.csv",
            "- summary.json",
            "- report.md",
            "- report.pdf",
            "- host-load-before.txt",
            "- host-load-samples.txt",
            "- host-load-after.txt",
        ]
    )
    return lines


def write_markdown(path: Path, lines: list[str]) -> None:
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def escape_pdf_text(text: str) -> str:
    return text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def wrap_text(text: str, width: int = 92) -> list[str]:
    if not text:
        return [""]
    words = text.split()
    lines: list[str] = []
    current = ""
    for word in words:
        next_line = word if not current else f"{current} {word}"
        if len(next_line) <= width:
            current = next_line
        else:
            if current:
                lines.append(current)
            current = word
    if current:
        lines.append(current)
    return lines


def write_pdf(path: Path, lines: list[str]) -> None:
    page_width = 612
    page_height = 792
    margin_x = 54
    top_y = 738
    bottom_y = 54
    default_line_height = 13

    pages: list[list[tuple[str, int, str, int]]] = [[]]
    y = top_y
    for source_line in lines:
        if source_line == "":
            y -= 8
            continue

        font = "F1"
        font_size = 10
        line_height = default_line_height
        gap_after = 0
        pdf_line = source_line
        if source_line.startswith("# "):
            font = "F2"
            font_size = 15
            line_height = 18
            gap_after = 10
            pdf_line = source_line[2:]
        elif source_line.startswith("## "):
            font = "F2"
            font_size = 12
            line_height = 15
            gap_after = 5
            pdf_line = source_line[3:]

        wrap_width = 92 if font_size <= 10 else 76
        for line in wrap_text(pdf_line, wrap_width):
            if y < bottom_y:
                pages.append([])
                y = top_y
            pages[-1].append((font, font_size, line, y))
            y -= line_height
        y -= gap_after

    objects: list[bytes] = []

    def add_object(body: bytes) -> int:
        objects.append(body)
        return len(objects)

    font_obj = add_object(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    bold_font_obj = add_object(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>")
    content_objects: list[int] = []
    page_objects: list[int] = []

    for page_lines in pages:
        commands = []
        for font, font_size, line, line_y in page_lines:
            commands.append("BT")
            commands.append(f"/{font} {font_size} Tf")
            commands.append(f"1 0 0 1 {margin_x} {line_y} Tm")
            commands.append(f"({escape_pdf_text(line)}) Tj")
            commands.append("ET")
        stream = "\n".join(commands).encode("ascii", errors="replace")
        content_obj = add_object(
            b"<< /Length " + str(len(stream)).encode("ascii") + b" >>\nstream\n" + stream + b"\nendstream"
        )
        content_objects.append(content_obj)
        page_obj = add_object(
            (
                f"<< /Type /Page /Parent {{pages}} 0 R /MediaBox [0 0 {page_width} {page_height}] "
                f"/Resources << /Font << /F1 {font_obj} 0 R /F2 {bold_font_obj} 0 R >> >> "
                f"/Contents {content_obj} 0 R >>"
            ).encode("ascii")
        )
        page_objects.append(page_obj)

    kids = " ".join(f"{page_obj} 0 R" for page_obj in page_objects)
    pages_obj = add_object(f"<< /Type /Pages /Kids [{kids}] /Count {len(page_objects)} >>".encode("ascii"))

    for page_obj in page_objects:
        objects[page_obj - 1] = objects[page_obj - 1].replace(b"{pages}", str(pages_obj).encode("ascii"))

    catalog_obj = add_object(f"<< /Type /Catalog /Pages {pages_obj} 0 R >>".encode("ascii"))

    output = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for index, body in enumerate(objects, start=1):
        offsets.append(len(output))
        output.extend(f"{index} 0 obj\n".encode("ascii"))
        output.extend(body)
        output.extend(b"\nendobj\n")

    xref_offset = len(output)
    output.extend(f"xref\n0 {len(objects) + 1}\n".encode("ascii"))
    output.extend(b"0000000000 65535 f \n")
    for offset in offsets[1:]:
        output.extend(f"{offset:010d} 00000 n \n".encode("ascii"))
    output.extend(
        (
            f"trailer\n<< /Size {len(objects) + 1} /Root {catalog_obj} 0 R >>\n"
            f"startxref\n{xref_offset}\n%%EOF\n"
        ).encode("ascii")
    )
    path.write_bytes(bytes(output))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--name", required=True, help="Benchmark name; reads target/local_video_latency/<name>")
    parser.add_argument(
        "--warmup-seconds",
        type=float,
        default=0.0,
        help="Exclude this many seconds from the beginning of the parsed logs.",
    )
    parser.add_argument(
        "--fail-on-stutter",
        action="store_true",
        help="Exit with status 2 when any subscriber stutter or visible frame skip is present. Invalid runs always exit non-zero.",
    )
    parser.add_argument(
        "--require-benchmark-pass",
        action="store_true",
        help="Exit with status 4 unless benchmark_status is PASS: valid, smooth, enough frame/time coverage, and host load OK.",
    )
    parser.add_argument(
        "--min-frame-coverage-pct",
        type=float,
        help=f"Minimum publisher/subscriber frame coverage percentage; default: {DEFAULT_MIN_FRAME_COVERAGE_PCT:g}.",
    )
    parser.add_argument(
        "--min-time-coverage-pct",
        type=float,
        help=f"Minimum publisher/subscriber time coverage percentage; default: {DEFAULT_MIN_TIME_COVERAGE_PCT:g}.",
    )
    for key, (_, _, label) in LATENCY_BUDGET_SPECS.items():
        parser.add_argument(
            "--" + key.replace("_", "-"),
            type=float,
            help=f"Fail clean PASS runs when {label} exceeds this many ms.",
        )
    args = parser.parse_args()

    directory = result_dir(args.name)
    if not directory.exists():
        print(f"error: benchmark directory does not exist: {directory}", file=sys.stderr)
        return 1
    for key in ("min_frame_coverage_pct", "min_time_coverage_pct"):
        value = getattr(args, key)
        if value is not None and not (0 <= value <= 100):
            print(
                f"error: --{key.replace('_', '-')} must be between 0 and 100",
                file=sys.stderr,
            )
            return 1
    for key in LATENCY_BUDGET_SPECS:
        value = getattr(args, key)
        if value is not None and value < 0:
            print(
                f"error: --{key.replace('_', '-')} must be non-negative",
                file=sys.stderr,
            )
            return 1

    windows, stutters, health, decode_health, jitter_buffers = parse_logs(directory)
    windows, stutters, decode_health, jitter_buffers = filter_warmup(
        windows,
        stutters,
        decode_health,
        jitter_buffers,
        args.warmup_seconds,
    )
    summary = summarize(
        args.name,
        directory,
        args.warmup_seconds,
        configured_coverage_thresholds(directory, args),
        configured_latency_budgets(directory, args),
        windows,
        stutters,
        health,
        decode_health,
        jitter_buffers,
    )
    lines = report_lines(summary)

    write_timeseries_csv(directory / "latency-timeseries.csv", windows)
    write_stutters_csv(directory / "stutters.csv", stutters)
    write_receiver_stats_csv(directory / "receiver-stats.csv", decode_health, jitter_buffers)
    write_worst_windows_csv(directory / "worst-windows.csv", summary)
    (directory / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    write_markdown(directory / "report.md", lines)
    write_pdf(directory / "report.pdf", lines)

    print("\n".join(lines[:25]))
    print("")
    print(f"Wrote report artifacts to {directory}")

    if not summary["valid"]:
        return 3
    if args.fail_on_stutter and summary["subscriber"]["total_smoothness_signals"] > 0:
        return 2
    if args.require_benchmark_pass and summary["benchmark_status"] != "PASS":
        return 4
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
