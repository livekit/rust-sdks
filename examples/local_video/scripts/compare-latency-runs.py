#!/usr/bin/env python3
"""Compare local_video latency benchmark summaries."""

from __future__ import annotations

import argparse
import csv
import json
import math
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


CSV_FIELDS = [
    "name",
    "analysis_schema_version",
    "benchmark_status",
    "smoothness_status",
    "coverage_status",
    "host_load_status",
    "latency_budget_status",
    "latency_budget_violations",
    "valid",
    "codec",
    "encoder",
    "decoder",
    "width",
    "height",
    "fps",
    "duration_seconds",
    "headless",
    "render_path",
    "test_pattern",
    "total_stutters",
    "visual_frame_drop_signals",
    "total_smoothness_signals",
    "smoothness_signal_windows",
    "smoothness_detail_log_rows",
    "smoothness_detail_log_rows_ignored",
    "post_signal_clean_tail_seconds",
    "sink_gap_p95_window_max_ms",
    "sink_gap_max_ms",
    "paint_gap_p95_window_max_ms",
    "paint_gap_max_ms",
    "e2e_p95_window_max_ms",
    "e2e_max_ms",
    "receive_to_decode_p95_window_max_ms",
    "receive_to_paint_p95_window_max_ms",
    "capture_to_packetize_p95_window_max_ms",
    "encoder_upload_to_output_p95_window_max_ms",
    "minimum_frame_coverage_pct",
    "minimum_time_coverage_pct",
    "host_busy_snapshots",
    "host_sample_snapshots",
    "host_max_external_total_cpu_pct",
    "host_max_external_process_name",
    "host_max_external_process_cpu_pct",
    "directory",
]

STATUS_RANK = {
    "PASS": 0,
    "PASS_HOST_BUSY": 10,
    "PASS_HOST_UNKNOWN": 20,
    "PASS_SHORT_RUN": 30,
    "PASS_SHORT_RUN_HOST_BUSY": 40,
    "LATENCY_BUDGET_EXCEEDED": 80,
    "LATENCY_BUDGET_EXCEEDED_HOST_BUSY": 90,
    "LATENCY_BUDGET_UNKNOWN": 95,
    "LATENCY_BUDGET_UNKNOWN_HOST_BUSY": 96,
    "STUTTERS_DETECTED": 100,
    "STUTTERS_DETECTED_HOST_BUSY": 110,
    "INCOMPLETE": 200,
    "INCOMPLETE_HOST_BUSY": 210,
    "INVALID": 300,
}


def repo_root() -> Path:
    return Path(__file__).resolve().parents[3]


def result_root() -> Path:
    return repo_root() / "target" / "local_video_latency"


def analyzer_path() -> Path:
    return Path(__file__).resolve().with_name("analyze-latency-log.py")


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


def nested_get(data: dict[str, Any], path: tuple[str, ...]) -> Any:
    value: Any = data
    for key in path:
        if not isinstance(value, dict):
            return None
        value = value.get(key)
        if value is None:
            return None
    return value


def as_float(value: Any) -> float | None:
    if value is None or value == "":
        return None
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        return None
    if math.isnan(parsed):
        return None
    return parsed


def as_int(value: Any) -> int | None:
    if value is None or value == "":
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def display(value: Any) -> str:
    if value is None:
        return "NA"
    if isinstance(value, float):
        return f"{value:.1f}"
    return str(value)


def load_summary(path: Path) -> dict[str, Any] | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"warning: unable to read {path}: {exc}", file=sys.stderr)
        return None


def summary_paths(root: Path, names: list[str], include_all: bool) -> list[Path]:
    if include_all:
        return sorted(root.glob("*/summary.json"))
    if names:
        return [root / name / "summary.json" for name in names]
    return sorted(root.glob("*/summary.json"))


def refresh_summary(path: Path) -> bool:
    directory = path.parent
    if not directory.exists():
        print(f"warning: benchmark directory does not exist: {directory}", file=sys.stderr)
        return False

    metadata = parse_metadata(directory)
    warmup_seconds = metadata.get("warmup_seconds", "0")
    command = [
        sys.executable,
        str(analyzer_path()),
        "--name",
        directory.name,
        "--warmup-seconds",
        warmup_seconds,
    ]
    result = subprocess.run(
        command,
        cwd=repo_root(),
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    if not path.exists():
        print(
            f"warning: refresh did not create summary for {directory.name}: "
            f"exit {result.returncode}",
            file=sys.stderr,
        )
        if result.stderr:
            print(result.stderr.strip(), file=sys.stderr)
        return False
    if result.returncode not in (0, 3):
        print(
            f"warning: refreshed {directory.name} with analyzer exit {result.returncode}",
            file=sys.stderr,
        )
        if result.stderr:
            print(result.stderr.strip(), file=sys.stderr)
    return True


def refresh_summaries(paths: list[Path]) -> None:
    refreshed = 0
    for path in paths:
        if refresh_summary(path):
            refreshed += 1
    print(f"Refreshed {refreshed} summaries", file=sys.stderr)


def summarize_run(path: Path) -> dict[str, Any] | None:
    data = load_summary(path)
    if data is None:
        return None

    directory = path.parent
    metadata = parse_metadata(directory)
    subscriber = data.get("subscriber", {})
    publisher = data.get("publisher", {})
    coverage = data.get("coverage", {})
    host_load = data.get("host_load", {})
    latency_budget = data.get("latency_budget", {})

    return {
        "name": data.get("name") or directory.name,
        "analysis_schema_version": data.get("analysis_schema_version"),
        "benchmark_status": data.get("benchmark_status") or "UNKNOWN",
        "smoothness_status": data.get("smoothness_status") or "UNKNOWN",
        "coverage_status": coverage.get("status") or "UNKNOWN",
        "host_load_status": host_load.get("status") or "UNKNOWN",
        "latency_budget_status": latency_budget.get("status") or "UNKNOWN",
        "latency_budget_violations": len(latency_budget.get("violations", [])),
        "valid": data.get("valid"),
        "codec": metadata.get("codec", "unknown"),
        "encoder": metadata.get("encoder", "unknown"),
        "decoder": metadata.get("decoder", "unknown"),
        "width": as_int(metadata.get("width")),
        "height": as_int(metadata.get("height")),
        "fps": as_float(metadata.get("fps") or coverage.get("requested_fps")),
        "duration_seconds": as_float(
            metadata.get("duration_seconds") or coverage.get("requested_duration_seconds")
        ),
        "headless": metadata.get("headless", "unknown"),
        "render_path": metadata.get("render_path", "unknown"),
        "test_pattern": metadata.get("test_pattern", "unknown"),
        "total_stutters": as_int(subscriber.get("total_stutters_over_threshold")),
        "visual_frame_drop_signals": as_int(subscriber.get("visual_frame_drop_signals")),
        "total_smoothness_signals": as_int(subscriber.get("total_smoothness_signals")),
        "smoothness_signal_windows": as_int(subscriber.get("smoothness_signal_windows")),
        "smoothness_detail_log_rows": as_int(subscriber.get("smoothness_detail_log_rows")),
        "smoothness_detail_log_rows_ignored": as_int(
            subscriber.get("smoothness_detail_log_rows_ignored")
        ),
        "post_signal_clean_tail_seconds": as_float(
            subscriber.get("post_signal_clean_tail_seconds")
        ),
        "sink_gap_p95_window_max_ms": as_float(
            subscriber.get("sink_gap_p95_window_max_ms")
        ),
        "sink_gap_max_ms": as_float(subscriber.get("sink_gap_max_ms")),
        "paint_gap_p95_window_max_ms": as_float(
            subscriber.get("paint_gap_p95_window_max_ms")
        ),
        "paint_gap_max_ms": as_float(subscriber.get("paint_gap_max_ms")),
        "e2e_p95_window_max_ms": as_float(subscriber.get("e2e_p95_window_max_ms")),
        "e2e_max_ms": as_float(subscriber.get("e2e_max_ms")),
        "receive_to_decode_p95_window_max_ms": as_float(
            subscriber.get("receive_to_decode_p95_window_max_ms")
        ),
        "receive_to_paint_p95_window_max_ms": as_float(
            subscriber.get("receive_to_paint_p95_window_max_ms")
        ),
        "capture_to_packetize_p95_window_max_ms": as_float(
            publisher.get("capture_to_packetize_p95_window_max_ms")
        ),
        "encoder_upload_to_output_p95_window_max_ms": as_float(
            publisher.get("encoder_upload_to_output_p95_window_max_ms")
        ),
        "minimum_frame_coverage_pct": as_float(
            coverage.get("minimum_frame_coverage_pct")
        ),
        "minimum_time_coverage_pct": as_float(
            coverage.get("minimum_time_coverage_pct")
        ),
        "host_busy_snapshots": as_int(host_load.get("busy_snapshots")),
        "host_sample_snapshots": as_int(host_load.get("sample_snapshots")),
        "host_max_external_total_cpu_pct": as_float(
            host_load.get("max_external_total_cpu_pct")
        ),
        "host_max_external_process_name": host_load.get("max_external_process_name"),
        "host_max_external_process_cpu_pct": as_float(
            host_load.get("max_external_process_cpu_pct")
        ),
        "directory": str(directory),
    }


def sort_key(row: dict[str, Any]) -> tuple[Any, ...]:
    return (
        STATUS_RANK.get(str(row["benchmark_status"]), 250),
        row.get("total_stutters") if row.get("total_stutters") is not None else 10**9,
        row.get("visual_frame_drop_signals")
        if row.get("visual_frame_drop_signals") is not None
        else 10**9,
        row.get("smoothness_signal_windows")
        if row.get("smoothness_signal_windows") is not None
        else 10**9,
        row.get("sink_gap_p95_window_max_ms")
        if row.get("sink_gap_p95_window_max_ms") is not None
        else float("inf"),
        row.get("e2e_p95_window_max_ms")
        if row.get("e2e_p95_window_max_ms") is not None
        else float("inf"),
        row["name"],
    )


def write_csv(path: Path, rows: list[dict[str, Any]]) -> None:
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=CSV_FIELDS)
        writer.writeheader()
        writer.writerows(rows)


def markdown_table(rows: list[dict[str, Any]], limit: int) -> str:
    fields = [
        "name",
        "analysis_schema_version",
        "benchmark_status",
        "codec",
        "encoder",
        "decoder",
        "latency_budget_status",
        "latency_budget_violations",
        "total_stutters",
        "visual_frame_drop_signals",
        "total_smoothness_signals",
        "smoothness_signal_windows",
        "smoothness_detail_log_rows",
        "smoothness_detail_log_rows_ignored",
        "post_signal_clean_tail_seconds",
        "sink_gap_p95_window_max_ms",
        "paint_gap_p95_window_max_ms",
        "e2e_p95_window_max_ms",
        "minimum_time_coverage_pct",
        "host_load_status",
        "host_max_external_process_name",
        "host_max_external_process_cpu_pct",
    ]
    lines = [
        "| " + " | ".join(fields) + " |",
        "| " + " | ".join("---" for _ in fields) + " |",
    ]
    for row in rows[:limit]:
        lines.append("| " + " | ".join(display(row.get(field)) for field in fields) + " |")
    return "\n".join(lines)


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
    bold_font_obj = add_object(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>"
    )
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
            b"<< /Length "
            + str(len(stream)).encode("ascii")
            + b" >>\nstream\n"
            + stream
            + b"\nendstream"
        )
        page_obj = add_object(
            (
                f"<< /Type /Page /Parent {{pages}} 0 R /MediaBox [0 0 {page_width} {page_height}] "
                f"/Resources << /Font << /F1 {font_obj} 0 R /F2 {bold_font_obj} 0 R >> >> "
                f"/Contents {content_obj} 0 R >>"
            ).encode("ascii")
        )
        page_objects.append(page_obj)

    kids = " ".join(f"{page_obj} 0 R" for page_obj in page_objects)
    pages_obj = add_object(
        f"<< /Type /Pages /Kids [{kids}] /Count {len(page_objects)} >>".encode(
            "ascii"
        )
    )

    for page_obj in page_objects:
        objects[page_obj - 1] = objects[page_obj - 1].replace(
            b"{pages}", str(pages_obj).encode("ascii")
        )

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


def pdf_report_lines(rows: list[dict[str, Any]], limit: int) -> list[str]:
    lines = [
        "# local_video latency comparison",
        f"- Generated: {datetime.now(UTC).replace(microsecond=0).isoformat().replace('+00:00', 'Z')}",
        f"- Runs compared: {len(rows)}",
        f"- Rows shown: {min(limit, len(rows))}",
        "",
        "## Runs",
    ]
    for index, row in enumerate(rows[:limit], start=1):
        lines.extend(
            [
                f"- {index}. {display(row.get('name'))}",
                (
                    f"  status={display(row.get('benchmark_status'))}, "
                    f"smoothness={display(row.get('smoothness_status'))}, "
                    f"coverage={display(row.get('coverage_status'))}, "
                    f"host={display(row.get('host_load_status'))}, "
                    f"budget={display(row.get('latency_budget_status'))}"
                ),
                (
                    f"  codec={display(row.get('codec'))}, "
                    f"encoder={display(row.get('encoder'))}, "
                    f"decoder={display(row.get('decoder'))}, "
                    f"headless={display(row.get('headless'))}, "
                    f"duration={display(row.get('duration_seconds'))}s"
                ),
                (
                    f"  signals: stutters={display(row.get('total_stutters'))}, "
                    f"visual_drops={display(row.get('visual_frame_drop_signals'))}, "
                    f"total={display(row.get('total_smoothness_signals'))}, "
                    f"windows={display(row.get('smoothness_signal_windows'))}, "
                    f"ignored_details={display(row.get('smoothness_detail_log_rows_ignored'))}"
                ),
                (
                    f"  p95 window max: sink_gap={display(row.get('sink_gap_p95_window_max_ms'))}ms, "
                    f"paint_gap={display(row.get('paint_gap_p95_window_max_ms'))}ms, "
                    f"e2e={display(row.get('e2e_p95_window_max_ms'))}ms, "
                    f"receive_to_paint={display(row.get('receive_to_paint_p95_window_max_ms'))}ms"
                ),
                (
                    f"  coverage_min={display(row.get('minimum_time_coverage_pct'))}%, "
                    f"host_external={display(row.get('host_max_external_process_name'))} "
                    f"{display(row.get('host_max_external_process_cpu_pct'))}%"
                ),
                "",
            ]
        )
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("names", nargs="*", help="Benchmark names under target/local_video_latency")
    parser.add_argument(
        "--all",
        action="store_true",
        help="Compare every summary under target/local_video_latency. This is also the default when no names are supplied.",
    )
    parser.add_argument("--csv", type=Path, help="Write a CSV comparison to this path.")
    parser.add_argument(
        "--markdown",
        type=Path,
        help="Write a Markdown comparison table to this path.",
    )
    parser.add_argument("--pdf", type=Path, help="Write a PDF comparison report to this path.")
    parser.add_argument(
        "--limit",
        type=int,
        default=20,
        help="Number of rows to print in the Markdown table.",
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="Re-run analyze-latency-log.py for selected runs before comparing.",
    )
    args = parser.parse_args()

    paths = summary_paths(result_root(), args.names, args.all or not args.names)
    if args.refresh:
        refresh_summaries(paths)

    rows = [
        row
        for path in paths
        if path.exists()
        for row in [summarize_run(path)]
        if row is not None
    ]
    rows.sort(key=sort_key)

    if args.csv:
        write_csv(args.csv, rows)
    if args.markdown:
        args.markdown.write_text(markdown_table(rows, args.limit) + "\n", encoding="utf-8")
    if args.pdf:
        write_pdf(args.pdf, pdf_report_lines(rows, args.limit))

    print(markdown_table(rows, args.limit))
    if args.csv:
        print(f"\nWrote CSV comparison to {args.csv}")
    if args.markdown:
        print(f"Wrote Markdown comparison to {args.markdown}")
    if args.pdf:
        print(f"Wrote PDF comparison to {args.pdf}")
    return 0 if rows else 1


if __name__ == "__main__":
    raise SystemExit(main())
