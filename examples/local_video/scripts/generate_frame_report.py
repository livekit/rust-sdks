#!/usr/bin/env python3
"""Generate a concise PDF report from local_video per-frame CSV logs."""

from __future__ import annotations

import argparse
import csv
import math
import statistics
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence

try:
    from reportlab.lib.colors import HexColor, white
    from reportlab.lib.pagesizes import landscape, letter
    from reportlab.pdfgen import canvas
except ImportError as error:
    raise SystemExit(
        "reportlab is required; install it with: python3 -m pip install reportlab"
    ) from error


NAVY = HexColor("#102A43")
BLUE = HexColor("#147D92")
CYAN = HexColor("#2CB1BC")
INK = HexColor("#243B53")
MUTED = HexColor("#627D98")
GRID = HexColor("#D9E2EC")
PANEL = HexColor("#F0F4F8")
RED = HexColor("#D64545")
ORANGE = HexColor("#E88D14")


@dataclass(frozen=True)
class LogData:
    kind: str
    path: Path
    rows: list[dict[str, str]]
    latency_column: str
    interval_column: str

    @property
    def label(self) -> str:
        return "Publisher" if self.kind == "publisher" else "Subscriber"


@dataclass(frozen=True)
class Event:
    elapsed_ms: float
    count: int
    duration_ms: float = 0.0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a PDF from publisher and/or subscriber --log-csv output."
    )
    parser.add_argument("--publisher", type=Path, help="Publisher CSV log")
    parser.add_argument("--subscriber", type=Path, help="Subscriber CSV log")
    parser.add_argument("-o", "--output", type=Path, help="Output PDF path")
    parser.add_argument("--title", default="Local Video Frame Metrics")
    args = parser.parse_args()
    if args.publisher is None and args.subscriber is None:
        parser.error("at least one of --publisher or --subscriber is required")
    if args.output is None:
        source = args.subscriber or args.publisher
        assert source is not None
        args.output = source.with_suffix(".pdf")
    return args


def number(value: str | None) -> float | None:
    if value is None or not value.strip():
        return None
    try:
        parsed = float(value)
    except ValueError:
        return None
    return parsed if math.isfinite(parsed) else None


def values(rows: Iterable[dict[str, str]], column: str) -> list[float]:
    return [parsed for row in rows if (parsed := number(row.get(column))) is not None]


def read_log(path: Path, kind: str) -> LogData:
    latency_column = "capture_to_packetize_ms" if kind == "publisher" else "e2e_latency_ms"
    interval_column = "packetize_interval_ms" if kind == "publisher" else "render_interval_ms"
    with path.open(newline="", encoding="utf-8") as source:
        reader = csv.DictReader(source)
        required = {"elapsed_ms", "frame_id", latency_column}
        missing = required.difference(reader.fieldnames or ())
        if missing:
            raise ValueError(f"{path} is not a {kind} frame log; missing {', '.join(sorted(missing))}")
        rows = [row for row in reader if number(row.get(latency_column)) is not None]
    if not rows:
        raise ValueError(f"{path} contains no completed {kind} frame samples")
    return LogData(kind, path, rows, latency_column, interval_column)


def percentile(samples: Sequence[float], percent: float) -> float:
    ordered = sorted(samples)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * percent / 100.0
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def series(log: LogData) -> list[tuple[float, float]]:
    result = []
    for row in log.rows:
        elapsed = number(row.get("elapsed_ms"))
        latency = number(row.get(log.latency_column))
        if elapsed is not None and latency is not None:
            result.append((elapsed, latency))
    return result


def gap_events(log: LogData) -> list[Event]:
    events = []
    for row in log.rows:
        elapsed = number(row.get("elapsed_ms"))
        gap = number(row.get("frame_id_gap"))
        if elapsed is not None and gap is not None and gap > 0:
            events.append(Event(elapsed, round(gap)))
    return events


def inferred_freeze_events(log: LogData) -> list[Event]:
    intervals = [value for value in values(log.rows, log.interval_column) if value > 0]
    if not intervals:
        return []
    expected = statistics.median(intervals)
    threshold = expected * 3.0
    events = []
    for row in log.rows:
        elapsed = number(row.get("elapsed_ms"))
        interval = number(row.get(log.interval_column))
        if elapsed is not None and interval is not None and interval > threshold:
            events.append(Event(elapsed, 1, interval - expected))
    return events


def subscriber_freeze_events(log: LogData) -> list[Event]:
    if log.kind != "subscriber":
        return inferred_freeze_events(log)
    counts = values(log.rows, "freeze_count")
    if not counts:
        return inferred_freeze_events(log)
    events = []
    previous_count = 0
    previous_duration = 0.0
    for row in log.rows:
        elapsed = number(row.get("elapsed_ms"))
        count = number(row.get("freeze_count"))
        duration = number(row.get("total_freeze_duration_ms"))
        if elapsed is None or count is None:
            continue
        rounded_count = round(count)
        if rounded_count > previous_count:
            duration_delta = max(0.0, (duration or previous_duration) - previous_duration)
            events.append(Event(elapsed, rounded_count - previous_count, duration_delta))
        previous_count = max(previous_count, rounded_count)
        if duration is not None:
            previous_duration = max(previous_duration, duration)
    return events


def last_value(log: LogData, column: str) -> float | None:
    return next(
        (parsed for row in reversed(log.rows) if (parsed := number(row.get(column))) is not None),
        None,
    )


def paired_loss_events(publisher: LogData, subscriber: LogData) -> list[Event]:
    publisher_ids = {round(value) for value in values(publisher.rows, "frame_id")}
    subscriber_ids = {round(value) for value in values(subscriber.rows, "frame_id")}
    if not publisher_ids or not subscriber_ids:
        return []
    low = max(min(publisher_ids), min(subscriber_ids))
    high = min(max(publisher_ids), max(subscriber_ids))
    missing_ids = {
        frame_id for frame_id in publisher_ids if low <= frame_id <= high
    } - subscriber_ids
    events = []
    for row in publisher.rows:
        frame_id = number(row.get("frame_id"))
        elapsed = number(row.get("elapsed_ms"))
        if frame_id is not None and elapsed is not None and round(frame_id) in missing_ids:
            events.append(Event(elapsed, 1))
    return events


def format_count(value: float | None) -> str:
    return "NA" if value is None else f"{round(value):,}"


def draw_header(pdf: canvas.Canvas, title: str, subtitle: str) -> None:
    width, height = landscape(letter)
    pdf.setFillColor(white)
    pdf.rect(0, 0, width, height, fill=1, stroke=0)
    pdf.setFillColor(NAVY)
    pdf.rect(0, height - 72, width, 72, fill=1, stroke=0)
    pdf.setFillColor(white)
    pdf.setFont("Helvetica-Bold", 21)
    pdf.drawString(38, height - 33, title)
    pdf.setFillColor(HexColor("#D9F2F4"))
    pdf.setFont("Helvetica", 8.5)
    pdf.drawString(39, height - 51, subtitle)


def draw_card(pdf: canvas.Canvas, x: float, y: float, width: float, label: str, value: str) -> None:
    pdf.setFillColor(PANEL)
    pdf.roundRect(x, y, width, 48, 5, fill=1, stroke=0)
    pdf.setFillColor(MUTED)
    pdf.setFont("Helvetica-Bold", 6.8)
    pdf.drawString(x + 9, y + 32, label.upper())
    pdf.setFillColor(INK)
    pdf.setFont("Helvetica-Bold", 15)
    pdf.drawString(x + 9, y + 11, value)


def draw_time_series(
    pdf: canvas.Canvas,
    logs: Sequence[LogData],
    loss_events: Sequence[Event],
    freeze_events: Sequence[Event],
    x: float,
    y: float,
    width: float,
    height: float,
) -> None:
    all_series = [(log, series(log)) for log in logs]
    latency_values = [latency for _, samples in all_series for _, latency in samples]
    duration = max(elapsed for _, samples in all_series for elapsed, _ in samples)
    y_max = max(1.0, percentile(latency_values, 99) * 1.2)

    pdf.setFillColor(INK)
    pdf.setFont("Helvetica-Bold", 11)
    pdf.drawString(x, y + height + 17, "Latency over time")
    pdf.setFont("Helvetica", 7.5)
    pdf.setFillColor(MUTED)
    pdf.drawRightString(x + width, y + height + 17, "milliseconds")

    for tick in range(5):
        tick_y = y + height * tick / 4
        pdf.setStrokeColor(GRID)
        pdf.line(x, tick_y, x + width, tick_y)
        pdf.setFillColor(MUTED)
        pdf.setFont("Helvetica", 7)
        pdf.drawRightString(x - 7, tick_y - 2, f"{y_max * tick / 4:.0f}")

    colors = {"publisher": CYAN, "subscriber": BLUE}
    for log, samples in all_series:
        stride = max(1, math.ceil(len(samples) / 1800))
        path = pdf.beginPath()
        for index, (elapsed, latency) in enumerate(samples[::stride]):
            point_x = x if duration <= 0 else x + width * elapsed / duration
            point_y = y + height * min(latency, y_max) / y_max
            (path.moveTo if index == 0 else path.lineTo)(point_x, point_y)
        pdf.setStrokeColor(colors[log.kind])
        pdf.setLineWidth(1.05)
        pdf.drawPath(path, stroke=1, fill=0)

    for event, color, offset in [
        *((event, RED, -1.0) for event in loss_events),
        *((event, ORANGE, 1.0) for event in freeze_events),
    ]:
        event_x = (
            x
            if duration <= 0
            else x + width * min(event.elapsed_ms, duration) / duration + offset
        )
        event_x = max(x, min(x + width, event_x))
        pdf.setStrokeColor(color)
        pdf.setLineWidth(0.55)
        pdf.setDash(2, 2)
        pdf.line(event_x, y, event_x, y + height)
    pdf.setDash()

    legend_x = x + 8
    for label, color in [
        *((log.label, colors[log.kind]) for log in logs),
        ("Frame loss", RED),
        ("Freeze", ORANGE),
    ]:
        pdf.setStrokeColor(color)
        pdf.setLineWidth(2)
        pdf.line(legend_x, y + height - 12, legend_x + 14, y + height - 12)
        pdf.setFillColor(MUTED)
        pdf.setFont("Helvetica", 7)
        pdf.drawString(legend_x + 18, y + height - 15, label)
        legend_x += 73

    pdf.setFillColor(MUTED)
    pdf.setFont("Helvetica", 7)
    for tick in range(5):
        tick_x = x + width * tick / 4
        pdf.drawCentredString(tick_x, y - 13, f"{duration * tick / 4000:.1f}s")
    pdf.setStrokeColor(INK)
    pdf.rect(x, y, width, height, fill=0, stroke=1)


def latency_rows(logs: Sequence[LogData]) -> list[tuple[str, list[float]]]:
    metrics = []
    for log in logs:
        if log.kind == "publisher":
            columns = (
                ("Publisher capture to buffer", "capture_to_buffer_ms"),
                ("Publisher encode", "encode_ms"),
                ("Publisher capture to packetize", "capture_to_packetize_ms"),
            )
        else:
            columns = (
                ("Subscriber exposure to receive", "exposure_to_receive_ms"),
                ("Subscriber receive to decode", "receive_to_decode_ms"),
                ("Subscriber receive to paint", "receive_to_paint_ms"),
                ("Subscriber end to end", "e2e_latency_ms"),
            )
        metrics.extend((label, values(log.rows, column)) for label, column in columns)
    return [(label, samples) for label, samples in metrics if samples]


def draw_latency_table(
    pdf: canvas.Canvas, logs: Sequence[LogData], x: float, y: float, width: float
) -> None:
    rows = latency_rows(logs)
    pdf.setFillColor(INK)
    pdf.setFont("Helvetica-Bold", 10.5)
    pdf.drawString(x, y + 18, "Latency summary")
    headers = (("Stage", 0), ("Mean", width - 135), ("P50", width - 88), ("P95", width - 41))
    pdf.setFillColor(NAVY)
    pdf.rect(x, y - 4, width, 21, fill=1, stroke=0)
    pdf.setFillColor(white)
    pdf.setFont("Helvetica-Bold", 7)
    for label, offset in headers:
        pdf.drawString(x + offset + 7, y + 4, label)
    row_y = y - 20
    for index, (label, samples) in enumerate(rows):
        pdf.setFillColor(PANEL if index % 2 == 0 else white)
        pdf.rect(x, row_y, width, 15, fill=1, stroke=0)
        pdf.setFillColor(INK)
        pdf.setFont("Helvetica", 7.3)
        pdf.drawString(x + 7, row_y + 4.5, label)
        for offset, value in zip(
            (width - 128, width - 81, width - 34),
            (statistics.fmean(samples), percentile(samples, 50), percentile(samples, 95)),
        ):
            pdf.drawRightString(x + offset, row_y + 4.5, f"{value:.1f}")
        row_y -= 15


def draw_delivery_table(
    pdf: canvas.Canvas,
    publisher: LogData | None,
    subscriber: LogData | None,
    losses: int,
    freezes: Sequence[Event],
    x: float,
    y: float,
    width: float,
) -> None:
    if subscriber is not None:
        packet_loss = last_value(subscriber, "packets_lost")
        dropped = last_value(subscriber, "frames_dropped")
        freeze_duration = last_value(subscriber, "total_freeze_duration_ms")
    else:
        packet_loss = dropped = freeze_duration = None
    freeze_count = sum(event.count for event in freezes)
    if publisher is not None and subscriber is not None:
        loss_label = "Publisher IDs not rendered"
    elif subscriber is not None:
        loss_label = "Rendered frame-ID gaps"
    else:
        loss_label = "Packetized frame-ID gaps"
    rows = (
        (loss_label, f"{losses:,}"),
        ("RTP packets lost", format_count(packet_loss)),
        ("WebRTC frames dropped", format_count(dropped)),
        ("Freezes", f"{freeze_count:,}"),
        ("Freeze duration", "NA" if freeze_duration is None else f"{freeze_duration:.0f} ms"),
    )
    pdf.setFillColor(INK)
    pdf.setFont("Helvetica-Bold", 10.5)
    pdf.drawString(x, y + 18, "Delivery quality")
    row_y = y - 4
    for index, (label, value) in enumerate(rows):
        pdf.setFillColor(PANEL if index % 2 == 0 else white)
        pdf.rect(x, row_y - 20, width, 20, fill=1, stroke=0)
        pdf.setFillColor(MUTED)
        pdf.setFont("Helvetica", 7.5)
        pdf.drawString(x + 7, row_y - 13, label)
        pdf.setFillColor(INK)
        pdf.setFont("Helvetica-Bold", 8)
        pdf.drawRightString(x + width - 7, row_y - 13, value)
        row_y -= 20


def generate_report(
    publisher: LogData | None,
    subscriber: LogData | None,
    output: Path,
    title: str,
) -> None:
    logs = [log for log in (publisher, subscriber) if log is not None]
    assert logs
    primary = subscriber or publisher
    assert primary is not None
    primary_latencies = values(primary.rows, primary.latency_column)
    duration_ms = max(values(primary.rows, "elapsed_ms"), default=0.0)

    event_log = subscriber or publisher
    assert event_log is not None
    loss_events = gap_events(event_log)
    freeze_events = subscriber_freeze_events(event_log)
    if publisher is not None and subscriber is not None:
        loss_events = paired_loss_events(publisher, subscriber)
    losses = sum(event.count for event in loss_events)

    sources = " + ".join(f"{log.label}: {log.path.name}" for log in logs)
    subtitle = f"{sources}  |  inclusive logged frame range"
    output.parent.mkdir(parents=True, exist_ok=True)
    pdf = canvas.Canvas(str(output), pagesize=landscape(letter))
    pdf.setTitle(title)
    pdf.setAuthor("LiveKit local_video")
    draw_header(pdf, title, subtitle)

    cards = (
        ("Rendered frames" if subscriber else "Packetized frames", f"{len(primary.rows):,}"),
        ("Duration", f"{duration_ms / 1000:.1f} s"),
        ("Mean latency", f"{statistics.fmean(primary_latencies):.1f} ms"),
        ("P50 latency", f"{percentile(primary_latencies, 50):.1f} ms"),
        ("P95 latency", f"{percentile(primary_latencies, 95):.1f} ms"),
        ("Frame losses", f"{losses:,}"),
    )
    card_width = 112
    for index, (label, value) in enumerate(cards):
        draw_card(pdf, 38 + index * (card_width + 11), 461, card_width, label, value)

    draw_time_series(pdf, logs, loss_events, freeze_events, 50, 206, 692, 205)
    draw_latency_table(pdf, logs, 38, 145, 470)
    draw_delivery_table(pdf, publisher, subscriber, losses, freeze_events, 530, 145, 224)

    pdf.setStrokeColor(GRID)
    pdf.line(38, 28, 754, 28)
    pdf.setFillColor(MUTED)
    pdf.setFont("Helvetica", 6.8)
    freeze_note = (
        "Freeze markers use subscriber WebRTC freeze counters."
        if subscriber and values(subscriber.rows, "freeze_count")
        else "Freeze markers are inter-frame gaps over 3x the median interval."
    )
    pdf.drawString(
        38,
        17,
        "Frame losses are frame-ID gaps; with paired logs they are publisher IDs not rendered by the subscriber. "
        + freeze_note,
    )
    pdf.save()


def main() -> int:
    args = parse_args()
    try:
        publisher = read_log(args.publisher, "publisher") if args.publisher else None
        subscriber = read_log(args.subscriber, "subscriber") if args.subscriber else None
        generate_report(publisher, subscriber, args.output, args.title)
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"Wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
