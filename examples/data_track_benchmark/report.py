#!/usr/bin/env python3
"""Generate presentation-ready data-track benchmark reports.

Inputs, all under --input-dir by default:
  results.csv       one row per benchmark cell
  latency.csv       one row per received frame
  publish.csv       one row per publisher send attempt
  livekit-server.log

Outputs:
  report.md
  report.html       charted browser report
  report.pdf        self-contained vector PDF, no browser dependency
  timeseries.csv    per-cell bucketed latency/throughput/message-loss series
  sfu_timeseries.csv optional SFU packet-loss samples parsed from server logs
"""

from __future__ import annotations

import argparse
import bisect
import csv
import html
import json
import math
import os
import re
from collections import defaultdict
from dataclasses import dataclass, field
from typing import Iterable


PALETTE = {
    "blue": (0.08, 0.32, 0.75),
    "green": (0.05, 0.50, 0.30),
    "red": (0.78, 0.17, 0.18),
    "orange": (0.90, 0.45, 0.10),
    "purple": (0.40, 0.26, 0.70),
    "gray": (0.38, 0.43, 0.50),
    "light": (0.94, 0.96, 0.98),
    "grid": (0.84, 0.87, 0.91),
    "ink": (0.08, 0.12, 0.20),
}


def fnum(value, default=0.0):
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def inum(value, default=0):
    try:
        return int(float(value))
    except (TypeError, ValueError):
        return default


def fmt(value, digits=1):
    if value is None or not math.isfinite(value):
        return "-"
    return f"{value:,.{digits}f}"


def fmt_int(value):
    return f"{int(value or 0):,}"


def pct(value, digits=1):
    return f"{fmt(100.0 * value, digits)}%"


def ms(value):
    return f"{fmt(value, 1)} ms"


def mibps(value):
    return f"{fmt(value, 2)} MiB/s"


def key_of(row):
    return (row["reliability"], int(row["size_kb"]), int(row["freq_hz"]))


def message_key(row):
    return (inum(row.get("run_id")), inum(row.get("seq")))


def label_of(key):
    reliability, size_kb, freq_hz = key
    return f"{reliability} {size_kb} KiB @ {freq_hz} Hz"


def percentile(values, p):
    if not values:
        return 0.0
    values = sorted(values)
    idx = min(len(values) - 1, math.ceil((len(values) - 1) * p))
    return values[idx]


def read_csv(path):
    if not os.path.exists(path):
        return []
    with open(path, newline="") as f:
        rows = []
        for row in csv.DictReader(f):
            parsed = {}
            for k, v in row.items():
                if v == "true":
                    parsed[k] = True
                elif v == "false":
                    parsed[k] = False
                else:
                    try:
                        parsed[k] = float(v) if "." in v else int(v)
                    except (TypeError, ValueError):
                        parsed[k] = v
            rows.append(parsed)
        return rows


def write_csv(path, rows, headers):
    with open(path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=headers)
        writer.writeheader()
        for row in rows:
            writer.writerow({h: row.get(h, "") for h in headers})


@dataclass
class Cell:
    key: tuple
    result_rows: list[dict] = field(default_factory=list)
    latency_rows: list[dict] = field(default_factory=list)
    publish_rows: list[dict] = field(default_factory=list)
    ts_rows: list[dict] = field(default_factory=list)

    @property
    def reliability(self):
        return self.key[0]

    @property
    def size_kb(self):
        return self.key[1]

    @property
    def freq_hz(self):
        return self.key[2]

    @property
    def frame_bytes(self):
        return self.size_kb * 1024

    def sent(self):
        if self.publish_rows:
            return sum(1 for r in self.publish_rows if r.get("sent"))
        return sum(inum(r.get("sent")) for r in self.result_rows)

    def attempted(self):
        if self.publish_rows:
            return len(self.publish_rows)
        return sum(inum(r.get("attempted")) for r in self.result_rows)

    def failed(self):
        if self.publish_rows:
            return sum(1 for r in self.publish_rows if not r.get("sent"))
        return sum(inum(r.get("failed")) for r in self.result_rows)

    def unique_received(self):
        return len({message_key(r) for r in self.latency_rows})

    def delivery_ratio(self):
        sent = self.sent()
        return self.unique_received() / sent if sent else 0.0

    def latencies(self):
        return [fnum(r.get("latency_ms")) for r in self.latency_rows]

    def avg_latency(self):
        vals = self.latencies()
        return sum(vals) / len(vals) if vals else 0.0

    def p95_latency(self):
        return percentile(self.latencies(), 0.95)

    def p99_latency(self):
        return percentile(self.latencies(), 0.99)

    def max_send_wait(self):
        vals = [fnum(r.get("send_wait_ms")) for r in self.publish_rows]
        if vals:
            return max(vals)
        return max((fnum(r.get("max_send_wait_ms")) for r in self.result_rows), default=0.0)

    def actual_mibps(self):
        vals = [fnum(r.get("actual_mibps")) for r in self.result_rows]
        return sum(vals) / len(vals) if vals else 0.0


def load_cells(input_dir):
    results = read_csv(os.path.join(input_dir, "results.csv"))
    latency = read_csv(os.path.join(input_dir, "latency.csv"))
    publish = read_csv(os.path.join(input_dir, "publish.csv"))
    cells = defaultdict(lambda: None)

    def cell(key):
        if cells[key] is None:
            cells[key] = Cell(key=key)
        return cells[key]

    for row in results:
        cell(key_of(row)).result_rows.append(row)
    for row in latency:
        cell(key_of(row)).latency_rows.append(row)
    for row in publish:
        cell(key_of(row)).publish_rows.append(row)

    return dict(sorted(cells.items(), key=lambda kv: (kv[0][0], kv[0][1], kv[0][2])))


def build_timeseries(cells, bucket_ms):
    rows = []
    for key, cell in cells.items():
        buckets = defaultdict(lambda: {
            "latencies": [],
            "received": 0,
            "received_bytes": 0,
            "attempted": 0,
            "sent": 0,
            "failed": 0,
            "sent_bytes": 0,
            "lost_messages": 0,
        })

        sent_messages = set()
        for row in cell.publish_rows:
            bucket = inum(row.get("send_elapsed_ms")) // bucket_ms
            buckets[bucket]["attempted"] += 1
            if row.get("sent"):
                buckets[bucket]["sent"] += 1
                buckets[bucket]["sent_bytes"] += inum(row.get("frame_bytes"), cell.frame_bytes)
                sent_messages.add(message_key(row))
            else:
                buckets[bucket]["failed"] += 1

        recv_by_message = {}
        for row in cell.latency_rows:
            msg = message_key(row)
            if msg in recv_by_message:
                continue
            recv_by_message[msg] = row
            bucket = inum(row.get("receive_elapsed_ms")) // bucket_ms
            buckets[bucket]["received"] += 1
            buckets[bucket]["received_bytes"] += cell.frame_bytes
            buckets[bucket]["latencies"].append(fnum(row.get("latency_ms")))

        if not sent_messages and cell.result_rows:
            for run_idx, row in enumerate(cell.result_rows):
                run_id = inum(row.get("run_id"), run_idx)
                sent_messages.update((run_id, seq) for seq in range(inum(row.get("sent"))))

        recv_messages = sorted(recv_by_message)
        recv_times = [inum(recv_by_message[msg].get("receive_elapsed_ms")) for msg in recv_messages]
        max_elapsed = 0
        if cell.latency_rows:
            max_elapsed = max(inum(r.get("receive_elapsed_ms")) for r in cell.latency_rows)
        if cell.publish_rows:
            max_elapsed = max(max_elapsed, max(inum(r.get("send_elapsed_ms")) for r in cell.publish_rows))
        drain_ms = max((inum(r.get("drain_ms")) for r in cell.result_rows), default=0)
        max_elapsed = max(max_elapsed, max((inum(r.get("duration_s")) * 1000 + drain_ms for r in cell.result_rows), default=0))

        for msg in sorted(sent_messages - set(recv_by_message)):
            idx = bisect.bisect_right(recv_messages, msg)
            elapsed = recv_times[idx] if idx < len(recv_times) else max_elapsed
            bucket = elapsed // bucket_ms
            buckets[bucket]["lost_messages"] += 1

        for bucket in sorted(buckets):
            b = buckets[bucket]
            lat = b["latencies"]
            row = {
                "reliability": key[0],
                "size_kb": key[1],
                "freq_hz": key[2],
                "bucket_s": bucket_ms * bucket / 1000.0,
                "attempted": b["attempted"],
                "sent": b["sent"],
                "failed": b["failed"],
                "received": b["received"],
                "lost_messages": b["lost_messages"],
                "sent_mibps": b["sent_bytes"] / 1024.0 / 1024.0 / (bucket_ms / 1000.0),
                "received_mibps": b["received_bytes"] / 1024.0 / 1024.0 / (bucket_ms / 1000.0),
                "avg_latency_ms": sum(lat) / len(lat) if lat else 0.0,
                "p95_latency_ms": percentile(lat, 0.95),
                "p99_latency_ms": percentile(lat, 0.99),
            }
            rows.append(row)
            cell.ts_rows.append(row)
    return rows


def parse_sfu_stats(log_path):
    if not os.path.exists(log_path):
        return [], []
    samples = []
    finals = []
    with open(log_path) as f:
        for line in f:
            if "data track stats" not in line:
                continue
            start = line.find("{")
            if start < 0:
                continue
            try:
                data = json.loads(line[start:])
            except json.JSONDecodeError:
                continue
            room = data.get("room", "")
            name = data.get("name", "")
            reliability = "reliable" if "reliable" in room or "reliable" in name else "lossy"
            row = {
                "reliability": reliability,
                "duration_s": fnum(data.get("duration")),
                "num_packets": inum(data.get("numPackets")),
                "num_packets_lost": inum(data.get("numPacketsLost")),
                "num_packets_out_of_order": inum(data.get("numPacketsOutOfOrder")),
                "num_frames": inum(data.get("numFrames")),
                "num_bytes": inum(data.get("numBytes")),
            }
            if "data track stats sample" in line:
                samples.append(row)
            else:
                finals.append(row)
    return samples, finals


def sfu_sample_deltas(samples):
    by_mode = defaultdict(list)
    for row in samples:
        by_mode[row["reliability"]].append(row)
    out = []
    for reliability, rows in by_mode.items():
        rows.sort(key=lambda r: r["duration_s"])
        prev = None
        for cur in rows:
            if prev is None:
                prev = cur
                continue
            dt = cur["duration_s"] - prev["duration_s"]
            if dt <= 0:
                prev = cur
                continue
            out.append({
                "reliability": reliability,
                "time_s": cur["duration_s"],
                "packet_rate": max(0, cur["num_packets"] - prev["num_packets"]) / dt,
                "packet_loss_rate": max(0, cur["num_packets_lost"] - prev["num_packets_lost"]) / dt,
                "byte_mibps": max(0, cur["num_bytes"] - prev["num_bytes"]) / 1024.0 / 1024.0 / dt,
                "cumulative_packets_lost": cur["num_packets_lost"],
            })
            prev = cur
    return out


def aggregate_by_mode(cells):
    rows = []
    for mode in ["lossy", "reliable"]:
        group = [c for c in cells.values() if c.reliability == mode]
        sent = sum(c.sent() for c in group)
        unique = sum(c.unique_received() for c in group)
        failed = sum(c.failed() for c in group)
        lats = [v for c in group for v in c.latencies()]
        max_tp = max(group, key=lambda c: c.actual_mibps(), default=None)
        worst = min(group, key=lambda c: c.delivery_ratio(), default=None)
        rows.append({
            "mode": mode,
            "cells": len(group),
            "sent": sent,
            "failed": failed,
            "unique": unique,
            "delivery": unique / sent if sent else 0.0,
            "avg_latency": sum(lats) / len(lats) if lats else 0.0,
            "p95": percentile(lats, 0.95),
            "p99": percentile(lats, 0.99),
            "max_tp": max_tp,
            "worst": worst,
        })
    return rows


def esc(s):
    return html.escape(str(s), quote=True)


def svg_line_chart(series, width=900, height=260, y_label="value", x_label="seconds"):
    left, right, top, bottom = 58, 18, 18, 36
    plot_w = width - left - right
    plot_h = height - top - bottom
    all_x = [x for s in series for x, _ in s["points"]]
    all_y = [y for s in series for _, y in s["points"]]
    max_x = max(all_x) if all_x else 1.0
    max_y = max(all_y) if all_y else 1.0
    max_x = max(max_x, 1.0)
    max_y = max(max_y, 1.0)

    def sx(x):
        return left + x / max_x * plot_w

    def sy(y):
        return top + plot_h - y / max_y * plot_h

    parts = [f'<svg viewBox="0 0 {width} {height}" class="chart-svg">']
    for t in [0, .25, .5, .75, 1]:
        yy = top + plot_h - t * plot_h
        xx = left + t * plot_w
        parts.append(f'<line x1="{left}" y1="{yy:.1f}" x2="{left + plot_w}" y2="{yy:.1f}" class="grid"/>')
        parts.append(f'<text x="{left - 8}" y="{yy + 4:.1f}" text-anchor="end" class="axis">{esc(fmt(max_y * t, 0))}</text>')
        parts.append(f'<line x1="{xx:.1f}" y1="{top}" x2="{xx:.1f}" y2="{top + plot_h}" class="grid"/>')
        parts.append(f'<text x="{xx:.1f}" y="{height - 11}" text-anchor="middle" class="axis">{esc(fmt(max_x * t, 1))}</text>')
    for s in series:
        pts = s["points"]
        if not pts:
            continue
        d = " ".join(f"{'M' if i == 0 else 'L'}{sx(x):.1f},{sy(y):.1f}" for i, (x, y) in enumerate(pts))
        color = s.get("color", "#175cd3")
        parts.append(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="2.2"/>')
    legend_x = left + 8
    legend_y = top + 15
    for i, s in enumerate(series):
        color = s.get("color", "#175cd3")
        y = legend_y + i * 16
        parts.append(f'<line x1="{legend_x}" y1="{y}" x2="{legend_x + 18}" y2="{y}" stroke="{color}" stroke-width="2.5"/>')
        parts.append(f'<text x="{legend_x + 24}" y="{y + 4}" class="axis">{esc(s["label"])}</text>')
    parts.append(f'<text x="{left}" y="{top + 11}" class="axis">{esc(y_label)}</text>')
    parts.append(f'<text x="{left + plot_w}" y="{height - 11}" text-anchor="end" class="axis">{esc(x_label)}</text>')
    parts.append("</svg>")
    return "".join(parts)


def svg_heatmap(cells, reliability, metric):
    group = [c for c in cells.values() if c.reliability == reliability]
    sizes = sorted({c.size_kb for c in group})
    freqs = sorted({c.freq_hz for c in group})
    values = {}
    for c in group:
        if metric == "p99":
            values[(c.size_kb, c.freq_hz)] = c.p99_latency()
        elif metric == "delivery":
            values[(c.size_kb, c.freq_hz)] = c.delivery_ratio() * 100.0
        else:
            values[(c.size_kb, c.freq_hz)] = c.avg_latency()
    max_v = max(values.values(), default=1.0)
    cell_w, cell_h = 92, 42
    left, top = 82, 34
    width = left + len(freqs) * cell_w + 20
    height = top + len(sizes) * cell_h + 30
    parts = [f'<svg viewBox="0 0 {width} {height}" class="heatmap-svg">']
    for i, f in enumerate(freqs):
        parts.append(f'<text x="{left + i * cell_w + cell_w / 2}" y="18" text-anchor="middle" class="axis">{f} Hz</text>')
    for j, s in enumerate(sizes):
        parts.append(f'<text x="{left - 8}" y="{top + j * cell_h + cell_h / 2 + 4}" text-anchor="end" class="axis">{s} KiB</text>')
    for j, s in enumerate(sizes):
        for i, f in enumerate(freqs):
            v = values.get((s, f))
            x = left + i * cell_w
            y = top + j * cell_h
            if v is None:
                color = "#f1f5f9"
                text = "-"
            else:
                t = min(1.0, v / max_v)
                if metric == "delivery":
                    t = 1.0 - min(1.0, v / 100.0)
                r = int(245 - 120 * (1 - t))
                g = int(248 - 120 * t)
                b = int(252 - 130 * t)
                color = f"#{r:02x}{g:02x}{b:02x}"
                text = fmt(v, 0) if metric != "delivery" else f"{fmt(v, 0)}%"
            parts.append(f'<rect x="{x}" y="{y}" width="{cell_w - 2}" height="{cell_h - 2}" fill="{color}" stroke="#d8dee9"/>')
            parts.append(f'<text x="{x + cell_w / 2}" y="{y + cell_h / 2 + 4}" text-anchor="middle" class="celltext">{esc(text)}</text>')
    parts.append("</svg>")
    return "".join(parts)


def downsample(points, max_points=450):
    if len(points) <= max_points:
        return points
    step = max(1, len(points) // max_points)
    return [p for i, p in enumerate(points) if i % step == 0 or i == len(points) - 1]


def top_cells(cells, reliability, count=4):
    return sorted(
        [c for c in cells.values() if c.reliability == reliability],
        key=lambda c: (c.p99_latency(), c.failed(), 1.0 - c.delivery_ratio()),
        reverse=True,
    )[:count]


def cell_series(cell, metric):
    points = []
    for row in cell.ts_rows:
        if metric == "latency":
            v = fnum(row["p95_latency_ms"])
        elif metric == "throughput":
            v = fnum(row["received_mibps"])
        elif metric == "send":
            v = fnum(row["sent_mibps"])
        elif metric == "loss":
            v = fnum(row["lost_messages"])
        else:
            v = 0.0
        points.append((fnum(row["bucket_s"]), v))
    return downsample(points)


def table_html(headers, rows):
    return "<table><thead><tr>" + "".join(f"<th>{esc(h)}</th>" for h in headers) + "</tr></thead><tbody>" + "".join(
        "<tr>" + "".join(f"<td>{c}</td>" for c in row) + "</tr>" for row in rows
    ) + "</tbody></table>"


def markdown_table(headers, rows):
    out = ["| " + " | ".join(headers) + " |", "| " + " | ".join("---" for _ in headers) + " |"]
    out.extend("| " + " | ".join(str(c) for c in row) + " |" for row in rows)
    return "\n".join(out)


def build_report(input_dir, output_dir, bucket_ms):
    cells = load_cells(input_dir)
    ts_rows = build_timeseries(cells, bucket_ms)
    sfu_samples, sfu_finals = parse_sfu_stats(os.path.join(input_dir, "livekit-server.log"))
    sfu_ts = sfu_sample_deltas(sfu_samples)

    os.makedirs(output_dir, exist_ok=True)
    write_csv(os.path.join(output_dir, "timeseries.csv"), ts_rows, [
        "reliability", "size_kb", "freq_hz", "bucket_s", "attempted", "sent", "failed",
        "received", "lost_messages", "sent_mibps", "received_mibps",
        "avg_latency_ms", "p95_latency_ms", "p99_latency_ms",
    ])
    write_csv(os.path.join(output_dir, "sfu_timeseries.csv"), sfu_ts, [
        "reliability", "time_s", "packet_rate", "packet_loss_rate", "byte_mibps", "cumulative_packets_lost",
    ])

    aggregate = aggregate_by_mode(cells)
    aggregate_rows = [[
        a["mode"], fmt_int(a["cells"]), fmt_int(a["sent"]), fmt_int(a["failed"]),
        fmt_int(a["unique"]), pct(a["delivery"]), ms(a["avg_latency"]), ms(a["p95"]),
        ms(a["p99"]), label_of(a["max_tp"].key) if a["max_tp"] else "-",
    ] for a in aggregate]
    sfu_rows = [[
        r["reliability"], fmt(r["duration_s"], 1), fmt_int(r["num_packets"]),
        fmt_int(r["num_packets_lost"]), fmt_int(r["num_packets_out_of_order"]),
        fmt_int(r["num_frames"]), f"{fmt(r['num_bytes'] / 1024 / 1024, 1)} MiB",
    ] for r in sfu_finals]
    stressed = top_cells(cells, "reliable", 8) + top_cells(cells, "lossy", 8)
    detail_rows = [[
        label_of(c.key), fmt_int(c.attempted()), fmt_int(c.sent()), fmt_int(c.failed()),
        fmt_int(c.unique_received()), pct(c.delivery_ratio()), ms(c.avg_latency()),
        ms(c.p95_latency()), ms(c.p99_latency()), ms(c.max_send_wait()), mibps(c.actual_mibps()),
    ] for c in stressed]

    reliable_summary = next((a for a in aggregate if a["mode"] == "reliable"), None)
    lossy_summary = next((a for a in aggregate if a["mode"] == "lossy"), None)
    partial_reliable = [
        c for c in cells.values()
        if c.reliability == "reliable" and (c.delivery_ratio() < 0.999 or c.failed() > 0)
    ]
    sfu_reliable_lost = sum(r["num_packets_lost"] for r in sfu_finals if r["reliability"] == "reliable")
    sfu_loss_note = (
        "SFU packet-loss time series is present from periodic server samples."
        if sfu_ts else
        "SFU packet-loss over time was not available; report includes cumulative SFU packet loss and subscriber-observed missing-message time series."
    )

    notes = [
        f"Cells: {len(cells)}; latency samples: {sum(len(c.latency_rows) for c in cells.values()):,}; publisher samples: {sum(len(c.publish_rows) for c in cells.values()):,}.",
        f"Reliable unique delivery: {pct(reliable_summary['delivery']) if reliable_summary else '-'}; lossy unique delivery: {pct(lossy_summary['delivery']) if lossy_summary else '-'}.",
        f"Reliable SFU cumulative packet loss: {fmt_int(sfu_reliable_lost)}.",
        sfu_loss_note,
        "Subscriber loss charts are missing sent message sequences over time, excluding publisher send failures.",
    ]

    md_lines = [
        "# LiveKit data-track benchmark report",
        "",
        "## Executive summary",
        "",
        *[f"- {n}" for n in notes],
        "",
        markdown_table(["mode", "cells", "sent", "failed", "unique", "delivery", "avg latency", "p95", "p99", "max throughput row"], aggregate_rows),
        "",
        "## SFU data-track stats",
        "",
        markdown_table(["mode", "duration s", "packets", "lost", "out-of-order", "frames", "bytes"], sfu_rows) if sfu_rows else "No SFU data-track stats found.",
        "",
        "## Stressed cells",
        "",
        markdown_table(["cell", "attempted", "sent", "failed", "unique", "delivery", "avg", "p95", "p99", "max send wait", "avg throughput"], detail_rows),
        "",
        "## Reliable rows with partial delivery or send failures",
        "",
    ]
    if partial_reliable:
        md_lines.append(markdown_table(
            ["cell", "sent", "failed", "unique", "delivery", "p99"],
            [[label_of(c.key), fmt_int(c.sent()), fmt_int(c.failed()), fmt_int(c.unique_received()), pct(c.delivery_ratio()), ms(c.p99_latency())] for c in partial_reliable],
        ))
    else:
        md_lines.append("No reliable rows had partial delivery or send failures.")
    md_lines.extend([
        "",
        "## Artifacts",
        "",
        f"- Summary CSV: {os.path.join(input_dir, 'results.csv')}",
        f"- Latency CSV: {os.path.join(input_dir, 'latency.csv')}",
        f"- Publish CSV: {os.path.join(input_dir, 'publish.csv')}",
        f"- Time series CSV: {os.path.join(output_dir, 'timeseries.csv')}",
        f"- SFU time series CSV: {os.path.join(output_dir, 'sfu_timeseries.csv')}",
        f"- HTML report: {os.path.join(output_dir, 'report.html')}",
        f"- PDF report: {os.path.join(output_dir, 'report.pdf')}",
        "",
    ])
    with open(os.path.join(output_dir, "report.md"), "w") as f:
        f.write("\n".join(md_lines))

    chart_cells = top_cells(cells, "reliable", 3) + top_cells(cells, "lossy", 2)
    html_charts = []
    for c in chart_cells:
        html_charts.append(f"<figure><figcaption>{esc(label_of(c.key))} latency p95 over time</figcaption>{svg_line_chart([{'label': label_of(c.key), 'points': cell_series(c, 'latency'), 'color': '#175cd3'}], y_label='p95 latency ms')}</figure>")
        html_charts.append(f"<figure><figcaption>{esc(label_of(c.key))} throughput over time</figcaption>{svg_line_chart([{'label': 'sent MiB/s', 'points': cell_series(c, 'send'), 'color': '#7c3aed'}, {'label': 'received MiB/s', 'points': cell_series(c, 'throughput'), 'color': '#0f766e'}], y_label='MiB/s')}</figure>")
        html_charts.append(f"<figure><figcaption>{esc(label_of(c.key))} missing messages over time</figcaption>{svg_line_chart([{'label': 'missing messages', 'points': cell_series(c, 'loss'), 'color': '#b42318'}], y_label='messages / bucket')}</figure>")

    html_doc = f"""<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>LiveKit data-track benchmark report</title>
  <style>
    body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 34px; color: #172033; line-height: 1.35; }}
    h1 {{ margin: 0 0 6px; font-size: 30px; }} h2 {{ margin-top: 30px; font-size: 20px; }}
    .summary {{ display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; margin: 18px 0; }}
    .metric {{ border: 1px solid #d8dee9; border-radius: 6px; background: #f8fafc; padding: 10px 12px; }}
    .metric .label {{ color: #5b6472; font-size: 11px; text-transform: uppercase; letter-spacing: .04em; }}
    .metric .value {{ font-size: 22px; font-weight: 700; margin-top: 4px; }}
    table {{ border-collapse: collapse; width: 100%; margin: 10px 0 18px; font-size: 12px; }}
    th, td {{ border: 1px solid #d8dee9; padding: 6px 7px; text-align: right; vertical-align: top; }}
    th:first-child, td:first-child {{ text-align: left; }} th {{ background: #eef2f7; }}
    .callout {{ border-left: 4px solid #175cd3; background: #f6f9ff; padding: 10px 12px; margin: 16px 0; }}
    figure {{ margin: 16px 0 24px; break-inside: avoid; }} figcaption {{ font-weight: 700; margin-bottom: 6px; }}
    .chart-svg, .heatmap-svg {{ width: 100%; border: 1px solid #d8dee9; border-radius: 6px; background: white; }}
    .grid {{ stroke: #e5e9f0; stroke-width: 1; }} .axis {{ fill: #5b6472; font-size: 11px; }} .celltext {{ fill: #111827; font-size: 12px; font-weight: 650; }}
    code {{ background: #f1f5f9; padding: 1px 4px; border-radius: 3px; }}
  </style>
</head>
<body>
  <h1>LiveKit data-track benchmark report</h1>
  <div class="summary">
    <div class="metric"><div class="label">cells</div><div class="value">{fmt_int(len(cells))}</div></div>
    <div class="metric"><div class="label">reliable delivery</div><div class="value">{esc(pct(reliable_summary['delivery']) if reliable_summary else '-')}</div></div>
    <div class="metric"><div class="label">lossy delivery</div><div class="value">{esc(pct(lossy_summary['delivery']) if lossy_summary else '-')}</div></div>
    <div class="metric"><div class="label">reliable SFU loss</div><div class="value">{fmt_int(sfu_reliable_lost)}</div></div>
  </div>
  <div class="callout">{esc(' '.join(notes))}</div>
  <h2>Aggregate results</h2>{table_html(["mode", "cells", "sent", "failed", "unique", "delivery", "avg latency", "p95", "p99", "max throughput row"], aggregate_rows)}
  <h2>SFU data-track stats</h2>{table_html(["mode", "duration s", "packets", "lost", "out-of-order", "frames", "bytes"], sfu_rows) if sfu_rows else "<p>No SFU data-track stats found.</p>"}
  <h2>P99 latency heatmaps</h2>
  <figure><figcaption>Lossy p99 latency (ms)</figcaption>{svg_heatmap(cells, "lossy", "p99")}</figure>
  <figure><figcaption>Reliable p99 latency (ms)</figcaption>{svg_heatmap(cells, "reliable", "p99")}</figure>
  <h2>Delivery heatmaps</h2>
  <figure><figcaption>Lossy delivery (%)</figcaption>{svg_heatmap(cells, "lossy", "delivery")}</figure>
  <figure><figcaption>Reliable delivery (%)</figcaption>{svg_heatmap(cells, "reliable", "delivery")}</figure>
  <h2>Latency, throughput, and loss over time</h2>
  {''.join(html_charts)}
  <h2>Stressed cells</h2>{table_html(["cell", "attempted", "sent", "failed", "unique", "delivery", "avg", "p95", "p99", "max send wait", "avg throughput"], detail_rows)}
</body>
</html>
"""
    with open(os.path.join(output_dir, "report.html"), "w") as f:
        f.write(html_doc)

    write_pdf(os.path.join(output_dir, "report.pdf"), cells, aggregate_rows, sfu_rows, detail_rows, notes, chart_cells)
    return {
        "cells": len(cells),
        "latency_samples": sum(len(c.latency_rows) for c in cells.values()),
        "publish_samples": sum(len(c.publish_rows) for c in cells.values()),
        "reliable_delivery": reliable_summary["delivery"] if reliable_summary else 0.0,
        "output_dir": output_dir,
    }


class Pdf:
    def __init__(self, path):
        self.path = path
        self.w = 792
        self.h = 612
        self.pages = []
        self.ops = []
        self.new_page()

    def new_page(self):
        if self.ops:
            self.pages.append(self.ops)
        self.ops = []

    def color(self, rgb, stroke=False):
        cmd = "RG" if stroke else "rg"
        self.ops.append(f"{rgb[0]:.3f} {rgb[1]:.3f} {rgb[2]:.3f} {cmd}")

    def text(self, x, y, text, size=10, bold=False, color=PALETTE["ink"]):
        font = "F2" if bold else "F1"
        self.color(color)
        safe = str(text).replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
        self.ops.append(f"BT /{font} {size} Tf {x:.1f} {y:.1f} Td ({safe}) Tj ET")

    def line(self, x1, y1, x2, y2, color=PALETTE["grid"], width=0.8):
        self.color(color, stroke=True)
        self.ops.append(f"{width:.2f} w {x1:.1f} {y1:.1f} m {x2:.1f} {y2:.1f} l S")

    def rect(self, x, y, w, h, fill=PALETTE["light"], stroke=PALETTE["grid"]):
        self.color(fill)
        self.ops.append(f"{x:.1f} {y:.1f} {w:.1f} {h:.1f} re f")
        if stroke:
            self.color(stroke, stroke=True)
            self.ops.append(f"0.6 w {x:.1f} {y:.1f} {w:.1f} {h:.1f} re S")

    def polyline(self, points, color=PALETTE["blue"], width=1.5):
        if len(points) < 2:
            return
        self.color(color, stroke=True)
        cmds = [f"{width:.2f} w {points[0][0]:.1f} {points[0][1]:.1f} m"]
        cmds.extend(f"{x:.1f} {y:.1f} l" for x, y in points[1:])
        cmds.append("S")
        self.ops.append(" ".join(cmds))

    def save(self):
        if self.ops:
            self.pages.append(self.ops)
            self.ops = []
        objs = [None]
        objs.append("<< /Type /Catalog /Pages 2 0 R >>")
        objs.append("")
        objs.append("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
        objs.append("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>")
        page_ids = []
        next_id = 5
        for ops in self.pages:
            stream = "\n".join(ops) + "\n"
            content_id = next_id
            page_id = next_id + 1
            next_id += 2
            objs.append(f"<< /Length {len(stream.encode())} >>\nstream\n{stream}endstream")
            objs.append(f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {self.w} {self.h}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {content_id} 0 R >>")
            page_ids.append(page_id)
        objs[2] = f"<< /Type /Pages /Kids [{' '.join(f'{p} 0 R' for p in page_ids)}] /Count {len(page_ids)} >>"
        pdf = "%PDF-1.4\n"
        offsets = [0]
        for i in range(1, len(objs)):
            offsets.append(len(pdf.encode()))
            pdf += f"{i} 0 obj\n{objs[i]}\nendobj\n"
        xref = len(pdf.encode())
        pdf += f"xref\n0 {len(objs)}\n0000000000 65535 f \n"
        for off in offsets[1:]:
            pdf += f"{off:010d} 00000 n \n"
        pdf += f"trailer\n<< /Size {len(objs)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        with open(self.path, "w") as f:
            f.write(pdf)


def pdf_table(pdf, x, y, headers, rows, col_widths, row_h=20, font_size=8):
    pdf.rect(x, y - row_h, sum(col_widths), row_h, fill=(0.17, 0.37, 0.54), stroke=None)
    cx = x
    for h, w in zip(headers, col_widths):
        pdf.text(cx + 4, y - 13, h, size=font_size, bold=True, color=(1, 1, 1))
        cx += w
    cy = y - row_h
    for i, row in enumerate(rows):
        fill = (1, 1, 1) if i % 2 == 0 else (0.94, 0.96, 0.98)
        pdf.rect(x, cy - row_h, sum(col_widths), row_h, fill=fill)
        cx = x
        for cell, w in zip(row, col_widths):
            txt = str(cell)
            if len(txt) > int(w / 4.8):
                txt = txt[: max(3, int(w / 4.8) - 1)] + "."
            pdf.text(cx + 4, cy - 13, txt, size=font_size)
            cx += w
        cy -= row_h
    return cy


def pdf_line_chart(pdf, x, y, w, h, title, series, y_label):
    pdf.text(x, y, title, size=11, bold=True)
    top = y - 16
    bottom = top - h
    left = x + 44
    right = x + w - 10
    all_x = [p[0] for s in series for p in s["points"]]
    all_y = [p[1] for s in series for p in s["points"]]
    max_x = max(all_x) if all_x else 1.0
    max_y = max(all_y) if all_y else 1.0
    max_x = max(max_x, 1.0)
    max_y = max(max_y, 1.0)
    for t in [0, .25, .5, .75, 1]:
        yy = bottom + t * h
        xx = left + t * (right - left)
        pdf.line(left, yy, right, yy)
        pdf.line(xx, bottom, xx, top)
        pdf.text(x, yy - 3, fmt(max_y * t, 0), size=7, color=PALETTE["gray"])
        pdf.text(xx - 8, bottom - 12, fmt(max_x * t, 0), size=7, color=PALETTE["gray"])
    for s in series:
        pts = []
        for tx, vy in downsample(s["points"], 220):
            px = left + tx / max_x * (right - left)
            py = bottom + vy / max_y * h
            pts.append((px, py))
        pdf.polyline(pts, color=s["rgb"], width=1.6)
    pdf.text(left, top - 10, y_label, size=7, color=PALETTE["gray"])


def pdf_heatmap(pdf, x, y, w, h, title, cells, reliability, metric):
    pdf.text(x, y, title, size=11, bold=True)
    group = [c for c in cells.values() if c.reliability == reliability]
    sizes = sorted({c.size_kb for c in group})
    freqs = sorted({c.freq_hz for c in group})
    values = {}
    for c in group:
        values[(c.size_kb, c.freq_hz)] = c.p99_latency() if metric == "p99" else c.delivery_ratio() * 100.0
    max_v = max(values.values(), default=1.0)
    left = x + 48
    top = y - 24
    cw = (w - 58) / max(1, len(freqs))
    ch = (h - 28) / max(1, len(sizes))
    for i, f in enumerate(freqs):
        pdf.text(left + i * cw + 4, top + 6, f"{f}Hz", size=7, color=PALETTE["gray"])
    for j, s in enumerate(sizes):
        pdf.text(x, top - (j + 1) * ch + ch / 2 - 3, f"{s}K", size=7, color=PALETTE["gray"])
        for i, f in enumerate(freqs):
            v = values.get((s, f))
            if v is None:
                fill = PALETTE["light"]
                text = "-"
            else:
                t = min(1.0, v / max_v)
                if metric == "delivery":
                    t = 1.0 - min(1.0, v / 100.0)
                fill = (0.96 - .35 * t, 0.98 - .42 * t, 1.0 - .48 * t)
                text = fmt(v, 0) if metric == "p99" else f"{fmt(v, 0)}%"
            px = left + i * cw
            py = top - (j + 1) * ch
            pdf.rect(px, py, cw - 2, ch - 2, fill=fill)
            pdf.text(px + 4, py + ch / 2 - 3, text, size=7)


def write_pdf(path, cells, aggregate_rows, sfu_rows, detail_rows, notes, chart_cells):
    pdf = Pdf(path)
    pdf.text(44, 566, "LiveKit data-track benchmark report", size=22, bold=True)
    y = 536
    for note in notes:
        pdf.text(52, y, "- " + note[:132], size=9)
        y -= 14
    y -= 8
    y = pdf_table(pdf, 44, y, ["mode", "cells", "sent", "failed", "unique", "delivery", "avg", "p95", "p99", "max throughput"], aggregate_rows, [58, 42, 58, 48, 58, 58, 70, 70, 70, 210], row_h=22, font_size=8)
    y -= 18
    if sfu_rows:
        pdf.text(44, y, "SFU data-track stats", size=13, bold=True)
        y -= 12
        pdf_table(pdf, 44, y, ["mode", "duration", "packets", "lost", "ooo", "frames", "bytes"], sfu_rows, [70, 65, 80, 58, 58, 70, 76], row_h=20, font_size=8)

    pdf.new_page()
    pdf.text(44, 566, "Latency and delivery by matrix cell", size=18, bold=True)
    pdf_heatmap(pdf, 44, 532, 340, 210, "Lossy p99 latency (ms)", cells, "lossy", "p99")
    pdf_heatmap(pdf, 410, 532, 340, 210, "Reliable p99 latency (ms)", cells, "reliable", "p99")
    pdf_heatmap(pdf, 44, 286, 340, 210, "Lossy delivery (%)", cells, "lossy", "delivery")
    pdf_heatmap(pdf, 410, 286, 340, 210, "Reliable delivery (%)", cells, "reliable", "delivery")

    pdf.new_page()
    pdf.text(44, 566, "Latency over time", size=18, bold=True)
    y = 528
    for c in chart_cells[:4]:
        pdf_line_chart(pdf, 44, y, 330, 115, label_of(c.key), [{"points": cell_series(c, "latency"), "rgb": PALETTE["blue"]}], "p95 ms")
        pdf_line_chart(pdf, 410, y, 330, 115, "throughput " + label_of(c.key), [
            {"points": cell_series(c, "send"), "rgb": PALETTE["purple"]},
            {"points": cell_series(c, "throughput"), "rgb": PALETTE["green"]},
        ], "MiB/s")
        y -= 142
        if y < 120:
            pdf.new_page()
            y = 528

    pdf.new_page()
    pdf.text(44, 566, "Missing messages over time", size=18, bold=True)
    y = 528
    for c in chart_cells[:6]:
        pdf_line_chart(pdf, 44, y, 700, 100, label_of(c.key), [{"points": cell_series(c, "loss"), "rgb": PALETTE["red"]}], "messages/bucket")
        y -= 125
        if y < 100:
            pdf.new_page()
            y = 528

    pdf.new_page()
    pdf.text(44, 566, "Stressed cells", size=18, bold=True)
    pdf_table(pdf, 44, 536, ["cell", "attempt", "sent", "fail", "unique", "delivery", "avg", "p95", "p99", "wait", "MiB/s"], detail_rows[:18], [170, 50, 45, 38, 48, 54, 62, 62, 62, 62, 58], row_h=20, font_size=7.4)
    pdf.save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input_dir", help="directory containing benchmark CSVs")
    parser.add_argument("--output-dir", default=None, help="report output directory; defaults to input_dir")
    parser.add_argument("--bucket-ms", type=int, default=1000, help="time-series bucket size")
    args = parser.parse_args()
    output_dir = args.output_dir or args.input_dir
    summary = build_report(args.input_dir, output_dir, args.bucket_ms)
    print(f"wrote {os.path.join(output_dir, 'report.pdf')}")
    print(f"wrote {os.path.join(output_dir, 'report.html')}")
    print(f"wrote {os.path.join(output_dir, 'timeseries.csv')}")
    print(f"cells={summary['cells']} latency_samples={summary['latency_samples']} publish_samples={summary['publish_samples']} reliable_delivery={pct(summary['reliable_delivery'])}")


if __name__ == "__main__":
    main()
