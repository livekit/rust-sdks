#!/usr/bin/env python3
"""Run a CSV-defined suite of local_video latency benchmarks."""

from __future__ import annotations

import argparse
import csv
import json
import shlex
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path


BOOLEAN_COLUMNS = {
    "background_window": "--background-window",
    "caffeinate": "--caffeinate",
    "fail_on_stutter": "--fail-on-stutter",
    "headless": "--headless",
    "keep_window_front": "--keep-window-front",
    "no_caffeinate": "--no-caffeinate",
    "no_render_vsync": "--no-render-vsync",
    "overlay": "--overlay",
    "render_loop_diagnostics": "--render-loop-diagnostics",
    "render_vsync": "--render-vsync",
    "require_benchmark_pass": "--require-benchmark-pass",
    "stats": "--stats",
    "test_pattern": "--test-pattern",
}

VALUE_COLUMNS = {
    "camera_index": "--camera-index",
    "codec": "--codec",
    "decoder": "--decoder",
    "degradation_preference": "--degradation-preference",
    "drop_late_frames_ms": "--drop-late-frames-ms",
    "duration": "--duration",
    "encoder": "--encoder",
    "format": "--format",
    "fps": "--fps",
    "height": "--height",
    "host_busy_process_cpu_pct": "--host-busy-process-cpu-pct",
    "host_busy_total_cpu_pct": "--host-busy-total-cpu-pct",
    "host_load_interval": "--host-load-interval",
    "idle_confirmation_samples": "--idle-confirmation-samples",
    "max_capture_to_packetize_p95_ms": "--max-capture-to-packetize-p95-ms",
    "max_e2e_p95_ms": "--max-e2e-p95-ms",
    "max_encoder_upload_to_output_p95_ms": "--max-encoder-upload-to-output-p95-ms",
    "max_playout_delay": "--max-playout-delay",
    "max_paint_gap_p95_ms": "--max-paint-gap-p95-ms",
    "max_receive_to_decode_p95_ms": "--max-receive-to-decode-p95-ms",
    "max_receive_to_paint_p95_ms": "--max-receive-to-paint-p95-ms",
    "max_sink_gap_p95_ms": "--max-sink-gap-p95-ms",
    "min_frame_coverage_pct": "--min-frame-coverage-pct",
    "min_playout_delay": "--min-playout-delay",
    "min_time_coverage_pct": "--min-time-coverage-pct",
    "publisher_bin": "--publisher-bin",
    "publisher_identity": "--publisher-identity",
    "render_path": "--render-path",
    "source": "--source",
    "subscriber_bin": "--subscriber-bin",
    "subscriber_identity": "--subscriber-identity",
    "wait_for_idle_host": "--wait-for-idle-host",
    "warmup": "--warmup",
    "width": "--width",
}

RAW_ARG_COLUMNS = {
    "publisher_args": "--publisher-arg",
}

PASSTHROUGH_COLUMNS = {"name", "notes"}
SUPPORTED_COLUMNS = (
    set(BOOLEAN_COLUMNS)
    | set(VALUE_COLUMNS)
    | set(RAW_ARG_COLUMNS)
    | PASSTHROUGH_COLUMNS
)

TRUE_VALUES = {"1", "true", "yes", "y", "on"}
FALSE_VALUES = {"0", "false", "no", "n", "off", ""}
RESULT_FIELDS = [
    "name",
    "returncode",
    "summary_exists",
    "benchmark_status",
    "smoothness_status",
    "coverage_status",
    "host_load_status",
    "latency_budget_status",
    "included_in_comparison",
]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[3]


def script_dir() -> Path:
    return Path(__file__).resolve().parent


def result_root() -> Path:
    return repo_root() / "target" / "local_video_latency"


def parse_bool(value: str, column: str) -> bool:
    normalized = value.strip().lower()
    if normalized in TRUE_VALUES:
        return True
    if normalized in FALSE_VALUES:
        return False
    raise ValueError(f"{column} must be a boolean value, got {value!r}")


def read_cases(path: Path) -> list[dict[str, str]]:
    with path.open("r", newline="", encoding="utf-8-sig") as handle:
        reader = csv.DictReader(handle)
        if reader.fieldnames is None:
            raise ValueError("cases CSV must have a header row")

        unknown_columns = sorted(set(reader.fieldnames) - SUPPORTED_COLUMNS)
        if unknown_columns:
            raise ValueError(
                "unsupported suite column(s): " + ", ".join(unknown_columns)
            )

        cases = []
        for index, row in enumerate(reader, start=2):
            normalized = {key: (value or "").strip() for key, value in row.items()}
            if not any(normalized.values()):
                continue
            if not normalized.get("name"):
                raise ValueError(f"row {index}: name is required")
            if "/" in normalized["name"] or normalized["name"] in (".", ".."):
                raise ValueError(f"row {index}: name must be a directory name")
            cases.append(normalized)

    if not cases:
        raise ValueError("cases CSV did not contain any cases")
    return cases


def append_auth_args(command: list[str], args: argparse.Namespace) -> None:
    if args.url:
        command.extend(["--url", args.url])
    if args.api_key:
        command.extend(["--api-key", args.api_key])
    if args.api_secret:
        command.extend(["--api-secret", args.api_secret])


def append_case_args(command: list[str], case: dict[str, str]) -> None:
    command.extend(["--name", case["name"]])

    for column, flag in VALUE_COLUMNS.items():
        value = case.get(column, "")
        if value:
            command.extend([flag, value])

    for column, flag in BOOLEAN_COLUMNS.items():
        value = case.get(column, "")
        if value and parse_bool(value, column):
            command.append(flag)

    for column, flag in RAW_ARG_COLUMNS.items():
        value = case.get(column, "")
        if not value:
            continue
        for raw_arg in shlex.split(value):
            command.extend([flag, raw_arg])


def build_command(case: dict[str, str], args: argparse.Namespace) -> list[str]:
    command = [str(args.harness)]
    append_auth_args(command, args)
    if args.overwrite:
        command.append("--overwrite")
    append_case_args(command, case)
    return command


def print_command(command: list[str]) -> None:
    print(" ".join(shlex.quote(part) for part in command), flush=True)


def summary_path(name: str) -> Path:
    return result_root() / name / "summary.json"


def load_summary(name: str) -> dict[str, object] | None:
    path = summary_path(name)
    if not path.exists():
        return None
    try:
        summary = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return summary if isinstance(summary, dict) else None


def case_summary_fields(summary: dict[str, object] | None) -> dict[str, object | None]:
    if summary is None:
        return {
            "benchmark_status": None,
            "smoothness_status": None,
            "coverage_status": None,
            "host_load_status": None,
            "latency_budget_status": None,
        }

    coverage = summary.get("coverage")
    host_load = summary.get("host_load")
    latency_budget = summary.get("latency_budget")
    return {
        "benchmark_status": summary.get("benchmark_status"),
        "smoothness_status": summary.get("smoothness_status"),
        "coverage_status": (
            coverage.get("status") if isinstance(coverage, dict) else None
        ),
        "host_load_status": (
            host_load.get("status") if isinstance(host_load, dict) else None
        ),
        "latency_budget_status": (
            latency_budget.get("status") if isinstance(latency_budget, dict) else None
        ),
    }


def write_suite_results(path: Path, rows: list[dict[str, object]]) -> None:
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=RESULT_FIELDS)
        writer.writeheader()
        writer.writerows(rows)


def comparison_paths(args: argparse.Namespace) -> dict[str, Path]:
    return {
        "csv": args.comparison_csv
        or result_root() / f"{args.cases.stem}-comparison.csv",
        "markdown": args.comparison_markdown
        or result_root() / f"{args.cases.stem}-comparison.md",
        "pdf": args.comparison_pdf
        or result_root() / f"{args.cases.stem}-comparison.pdf",
    }


def compare_completed(names: list[str], args: argparse.Namespace) -> int:
    if args.no_compare or not names:
        return 0

    artifacts = comparison_paths(args)
    command = [
        str(script_dir() / "compare-latency-runs.py"),
        *names,
        "--csv",
        str(artifacts["csv"]),
        "--markdown",
        str(artifacts["markdown"]),
        "--pdf",
        str(artifacts["pdf"]),
        "--limit",
        str(args.limit),
    ]
    return subprocess.run(command, check=False).returncode


def suite_status(
    suite_results: list[dict[str, object]],
    failed: list[tuple[str, int]],
    missing_summaries: list[str],
    non_pass_summaries: list[dict[str, object]],
    comparable: list[str],
    compare_status: int,
    no_compare: bool,
) -> str:
    if missing_summaries:
        return "MISSING_SUMMARY"
    if non_pass_summaries:
        return "BENCHMARK_FAILED"
    if failed:
        return "FAILED"
    if not suite_results:
        return "NO_RUNS"
    if no_compare:
        return "PASS_NO_COMPARE"
    if not comparable:
        return "NO_COMPARABLE_RUNS"
    if compare_status != 0:
        return "COMPARISON_FAILED"
    return "PASS"


def write_suite_summary(
    path: Path,
    args: argparse.Namespace,
    cases: list[dict[str, str]],
    suite_results: list[dict[str, object]],
    failed: list[tuple[str, int]],
    missing_summaries: list[str],
    non_pass_summaries: list[dict[str, object]],
    comparable: list[str],
    compare_status: int,
    suite_results_csv: Path,
) -> None:
    artifacts = comparison_paths(args)
    status = suite_status(
        suite_results,
        failed,
        missing_summaries,
        non_pass_summaries,
        comparable,
        compare_status,
        args.no_compare,
    )
    payload = {
        "schema_version": 1,
        "generated_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "suite_status": status,
        "cases_csv": str(args.cases),
        "case_count": len(cases),
        "cases_run": len(suite_results),
        "cases_failed": len(failed),
        "cases_missing_summary": len(missing_summaries),
        "cases_non_pass": len(non_pass_summaries),
        "failed_cases": [
            {"name": name, "returncode": returncode}
            for name, returncode in failed
        ],
        "missing_summary_cases": missing_summaries,
        "non_pass_cases": non_pass_summaries,
        "comparable_cases": comparable,
        "compare_status": compare_status,
        "suite_results_csv": str(suite_results_csv),
        "comparison_artifacts": (
            {} if args.no_compare else {key: str(value) for key, value in artifacts.items()}
        ),
        "case_results": suite_results,
    }
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=Path, required=True, help="CSV file of cases to run.")
    parser.add_argument("--url", help="LiveKit server URL, or use LIVEKIT_URL.")
    parser.add_argument("--api-key", help="LiveKit API key, or use LIVEKIT_API_KEY.")
    parser.add_argument("--api-secret", help="LiveKit API secret, or use LIVEKIT_API_SECRET.")
    parser.add_argument(
        "--harness",
        type=Path,
        default=script_dir() / "run-latency-benchmark.sh",
        help="Benchmark harness command to run for each case.",
    )
    parser.add_argument("--overwrite", action="store_true", help="Overwrite existing case directories.")
    parser.add_argument("--dry-run", action="store_true", help="Print commands without running them.")
    parser.add_argument(
        "--continue-on-error",
        action="store_true",
        help="Continue running later cases after a case fails.",
    )
    parser.add_argument("--no-compare", action="store_true", help="Do not run compare-latency-runs.py after the suite.")
    parser.add_argument("--comparison-csv", type=Path, help="Path for aggregate comparison CSV.")
    parser.add_argument("--comparison-markdown", type=Path, help="Path for aggregate comparison Markdown.")
    parser.add_argument("--comparison-pdf", type=Path, help="Path for aggregate comparison PDF.")
    parser.add_argument("--suite-results-csv", type=Path, help="Path for per-case return-code CSV.")
    parser.add_argument("--suite-summary-json", type=Path, help="Path for aggregate suite summary JSON.")
    parser.add_argument("--limit", type=int, default=20, help="Rows to print in the aggregate comparison table.")
    args = parser.parse_args()

    try:
        cases = read_cases(args.cases)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    comparable = []
    failed = []
    missing_summaries: list[str] = []
    non_pass_summaries: list[dict[str, object]] = []
    suite_results: list[dict[str, object]] = []
    for case in cases:
        command = build_command(case, args)
        print(f"\n== {case['name']} ==", flush=True)
        print_command(command)
        if args.dry_run:
            continue

        result = subprocess.run(command, check=False)
        summary = load_summary(case["name"])
        summary_exists = summary is not None
        summary_fields = case_summary_fields(summary)
        included_in_comparison = summary_exists
        if included_in_comparison:
            comparable.append(case["name"])
        else:
            missing_summaries.append(case["name"])
        if (
            summary_exists
            and summary_fields["benchmark_status"] is not None
            and summary_fields["benchmark_status"] != "PASS"
        ):
            non_pass_summaries.append(
                {
                    "name": case["name"],
                    "benchmark_status": summary_fields["benchmark_status"],
                    "smoothness_status": summary_fields["smoothness_status"],
                    "coverage_status": summary_fields["coverage_status"],
                    "host_load_status": summary_fields["host_load_status"],
                    "latency_budget_status": summary_fields["latency_budget_status"],
                }
            )
        suite_results.append(
            {
                "name": case["name"],
                "returncode": result.returncode,
                "summary_exists": summary_exists,
                **summary_fields,
                "included_in_comparison": included_in_comparison,
            }
        )

        if result.returncode == 0:
            continue

        failed.append((case["name"], result.returncode))
        if not args.continue_on_error:
            break

    if args.dry_run:
        return 0

    suite_results_csv = args.suite_results_csv or (
        result_root() / f"{args.cases.stem}-suite-results.csv"
    )
    write_suite_results(suite_results_csv, suite_results)
    print(f"Wrote suite results to {suite_results_csv}", flush=True)

    compare_status = compare_completed(comparable, args)
    suite_summary_json = args.suite_summary_json or (
        result_root() / f"{args.cases.stem}-suite-summary.json"
    )
    write_suite_summary(
        suite_summary_json,
        args,
        cases,
        suite_results,
        failed,
        missing_summaries,
        non_pass_summaries,
        comparable,
        compare_status,
        suite_results_csv,
    )
    print(f"Wrote suite summary to {suite_summary_json}", flush=True)
    if missing_summaries:
        for name in missing_summaries:
            print(f"case did not produce a readable summary: {name}", file=sys.stderr)
        return 5
    if non_pass_summaries:
        for row in non_pass_summaries:
            print(
                "case benchmark did not pass: "
                f"{row['name']} status={row['benchmark_status']}",
                file=sys.stderr,
            )
        return 4
    if failed:
        for name, returncode in failed:
            print(f"case failed: {name} exited {returncode}", file=sys.stderr)
        return failed[0][1] or 1
    return compare_status


if __name__ == "__main__":
    raise SystemExit(main())
