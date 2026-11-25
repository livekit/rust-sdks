"""Join vector_video timing CSVs by source frame and report latency drift."""

import argparse
import csv
import math
import statistics
from pathlib import Path


def read_trace(path):
    """Keep the earliest observation of each stage for a source frame."""
    frames = {}
    with path.open(newline="") as source:
        for row in csv.DictReader(source):
            frame = frames.setdefault(int(row["frame"]), {})
            event = row["event"]
            stamp = int(row["unix_us"])
            frame[event] = min(stamp, frame.get(event, stamp))
    return frames


def percentile(values, fraction):
    """Return the nearest-rank sample percentile."""
    return sorted(values)[math.ceil(len(values) * fraction) - 1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("publisher", type=Path)
    parser.add_argument("subscriber", type=Path)
    parser.add_argument("--frames", type=Path, help="write joined per-frame events to a new CSV")
    parser.add_argument("--warmup", type=float, default=1, help="exclude initial seconds from percentiles")
    args = parser.parse_args()
    publisher = read_trace(args.publisher)
    subscriber = read_trace(args.subscriber)
    stages = ("capture", "exposure", "send", "receive", "render", "paint", "gpu_publisher_clock", "file")
    rows = [
        dict(frame=index, **{**events, **subscriber.get(index, {})})
        for index, events in publisher.items() if "capture" in events
    ]
    if not rows or args.warmup < 0:
        parser.error("a nonempty capture trace and a nonnegative warmup are required")
    rows.sort(key=lambda r: r["capture"])
    start = rows[0]["capture"]
    samples = []
    endpoint = "paint" if any("paint" in row for row in rows) else "file"
    glass = []
    for row in rows:
        if "exposure" in row and "gpu_publisher_clock" in row:
            us = row["gpu_publisher_clock"] - row["exposure"]
            if us >= 0:
                row["g2g_ms"] = us / 1000
                if (row["capture"] - start) / 1e6 >= args.warmup:
                    glass.append(us / 1000)
        if endpoint in row:
            latency = (row[endpoint] - row["capture"]) / 1000
            row["latency_ms"] = latency
            if (row["capture"] - start) / 1e6 >= args.warmup:
                samples.append(((row["capture"] - start) / 1e6, latency))
    if args.frames:
        with args.frames.open("x", newline="") as out:
            writer = csv.DictWriter(out, ["frame", *stages, "latency_ms", "g2g_ms"])
            writer.writeheader()
            writer.writerows(rows)
    for stage in stages:
        count = sum(stage in row for row in rows)
        if count:
            print(f"{stage:8} {count:6} frames")
    if glass:
        print(f"Exposure to GPU completion: p50 {statistics.median(glass):.1f}, p95 {percentile(glass, .95):.1f} ms ({len(glass)} frames)")
    if not samples:
        parser.error("no matching output frames after warmup; check the trace pair and frame IDs")
    ages = [latency for _, latency in samples]
    times = [time for time, _ in samples]
    if min(ages) < 0:
        print("Negative latency detected. Check host clock synchronization and trace pairing.")
    print(f"Capture to {endpoint}: p50 {statistics.median(ages):.2f}, p95 {percentile(ages,.95):.2f}, "
          f"p99 {percentile(ages,.99):.2f}, max {max(ages):.2f} ms")
    window = min(10, (times[-1] - times[0]) / 2)
    early = [age for t, age in samples if t <= times[0] + window]
    late = [age for t, age in samples if t >= times[-1] - window]
    print(f"First and last {window:.1f}s medians: {statistics.median(early):.2f} and {statistics.median(late):.2f} ms")
    if times[-1] > times[0]:
        slope = statistics.linear_regression(times, ages).slope
        print(f"Linear age trend: {slope * 60:+.2f} ms/min")


if __name__ == "__main__":
    main()
