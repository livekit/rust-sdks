#!/usr/bin/env python3
"""Regression checks for the local_video latency suite runner."""

from __future__ import annotations

import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
from argparse import Namespace
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
SUITE_PATH = SCRIPT_DIR / "run-latency-suite.py"


def load_suite():
    spec = importlib.util.spec_from_file_location("run_latency_suite", SUITE_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Unable to load {SUITE_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def assert_case_summary_fields(module) -> None:
    assert module.case_summary_fields(None) == {
        "benchmark_status": None,
        "smoothness_status": None,
        "coverage_status": None,
        "host_load_status": None,
        "latency_budget_status": None,
    }

    fields = module.case_summary_fields(
        {
            "benchmark_status": "PASS",
            "smoothness_status": "PASS",
            "coverage": {"status": "OK"},
            "host_load": {"status": "OK"},
            "latency_budget": {"status": "OK"},
        }
    )
    assert fields == {
        "benchmark_status": "PASS",
        "smoothness_status": "PASS",
        "coverage_status": "OK",
        "host_load_status": "OK",
        "latency_budget_status": "OK",
    }


def assert_suite_statuses(module) -> None:
    passing_result = {
        "name": "pass",
        "returncode": 0,
        "summary_exists": True,
        "benchmark_status": "PASS",
        "included_in_comparison": True,
    }

    assert (
        module.suite_status(
            [passing_result],
            failed=[],
            missing_summaries=[],
            non_pass_summaries=[],
            comparable=["pass"],
            compare_status=0,
            no_compare=False,
        )
        == "PASS"
    )
    assert (
        module.suite_status(
            [passing_result],
            failed=[],
            missing_summaries=[],
            non_pass_summaries=[],
            comparable=["pass"],
            compare_status=0,
            no_compare=True,
        )
        == "PASS_NO_COMPARE"
    )
    assert (
        module.suite_status(
            [passing_result],
            failed=[("failed", 2)],
            missing_summaries=[],
            non_pass_summaries=[],
            comparable=["pass"],
            compare_status=0,
            no_compare=False,
        )
        == "FAILED"
    )
    assert (
        module.suite_status(
            [passing_result],
            failed=[("stutter", 4)],
            missing_summaries=[],
            non_pass_summaries=[{"name": "stutter", "benchmark_status": "STUTTERS_DETECTED"}],
            comparable=["pass", "stutter"],
            compare_status=0,
            no_compare=False,
        )
        == "BENCHMARK_FAILED"
    )
    assert (
        module.suite_status(
            [passing_result],
            failed=[("missing", 4)],
            missing_summaries=["missing"],
            non_pass_summaries=[],
            comparable=["pass"],
            compare_status=0,
            no_compare=False,
        )
        == "MISSING_SUMMARY"
    )
    assert (
        module.suite_status(
            [passing_result],
            failed=[],
            missing_summaries=["missing"],
            non_pass_summaries=[],
            comparable=["pass"],
            compare_status=0,
            no_compare=False,
        )
        == "MISSING_SUMMARY"
    )
    assert (
        module.suite_status(
            [passing_result],
            failed=[],
            missing_summaries=[],
            non_pass_summaries=[{"name": "stutter", "benchmark_status": "STUTTERS_DETECTED"}],
            comparable=["pass", "stutter"],
            compare_status=0,
            no_compare=False,
        )
        == "BENCHMARK_FAILED"
    )
    assert (
        module.suite_status(
            [passing_result],
            failed=[],
            missing_summaries=[],
            non_pass_summaries=[],
            comparable=[],
            compare_status=0,
            no_compare=False,
        )
        == "NO_COMPARABLE_RUNS"
    )
    assert (
        module.suite_status(
            [passing_result],
            failed=[],
            missing_summaries=[],
            non_pass_summaries=[],
            comparable=["pass"],
            compare_status=1,
            no_compare=False,
        )
        == "COMPARISON_FAILED"
    )


def assert_write_suite_summary(module) -> None:
    with tempfile.TemporaryDirectory(prefix="local-video-suite-test.") as tmp:
        tmp_path = Path(tmp)
        args = Namespace(
            cases=tmp_path / "cases.csv",
            comparison_csv=tmp_path / "comparison.csv",
            comparison_markdown=tmp_path / "comparison.md",
            comparison_pdf=tmp_path / "comparison.pdf",
            no_compare=False,
        )
        output = tmp_path / "suite-summary.json"
        module.write_suite_summary(
            output,
            args,
            cases=[{"name": "missing"}],
            suite_results=[
                {
                    "name": "missing",
                    "returncode": 0,
                    "summary_exists": False,
                    "included_in_comparison": False,
                }
            ],
            failed=[],
            missing_summaries=["missing"],
            non_pass_summaries=[],
            comparable=[],
            compare_status=0,
            suite_results_csv=tmp_path / "suite-results.csv",
        )

        data = json.loads(output.read_text(encoding="utf-8"))
        assert data["schema_version"] == 1
        assert data["suite_status"] == "MISSING_SUMMARY"
        assert data["cases_missing_summary"] == 1
        assert data["missing_summary_cases"] == ["missing"]
        assert data["comparison_artifacts"]["pdf"].endswith("comparison.pdf")


def assert_subscriber_passthrough_rejected(module) -> None:
    with tempfile.TemporaryDirectory(prefix="local-video-suite-test.") as tmp:
        cases = Path(tmp) / "cases.csv"
        cases.write_text(
            "name,subscriber_args\ncase,--low-latency-receiver\n",
            encoding="utf-8",
        )

        try:
            module.read_cases(cases)
        except ValueError as error:
            assert "subscriber_args" in str(error)
        else:
            raise AssertionError("subscriber_args should not be a supported suite column")


def write_fake_harness(path: Path, result_root: Path) -> None:
    path.write_text(
        f"""#!/usr/bin/env python3
import json
import sys
from pathlib import Path

name = None
for index, arg in enumerate(sys.argv):
    if arg == "--name" and index + 1 < len(sys.argv):
        name = sys.argv[index + 1]
        break
if name is None:
    raise SystemExit(64)

result_root = Path({str(result_root)!r})
run_dir = result_root / name
run_dir.mkdir(parents=True, exist_ok=True)
if name.endswith("-missing"):
    raise SystemExit(0)

benchmark_status = "STUTTERS_DETECTED" if name.endswith("-fail") else "PASS"
payload = {{
    "name": name,
    "benchmark_status": benchmark_status,
    "smoothness_status": "PASS" if benchmark_status == "PASS" else "STUTTERS_DETECTED",
    "coverage": {{"status": "OK"}},
    "host_load": {{"status": "OK"}},
    "latency_budget": {{"status": "OK"}},
}}
(run_dir / "summary.json").write_text(json.dumps(payload), encoding="utf-8")
if name.endswith("-fail"):
    raise SystemExit(4)
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


def run_suite_cli(
    tmp_path: Path,
    harness: Path,
    case_name: str,
) -> tuple[int, dict[str, object]]:
    cases = tmp_path / f"{case_name}.csv"
    cases.write_text(f"name\n{case_name}\n", encoding="utf-8")
    summary = tmp_path / f"{case_name}-summary.json"
    result = subprocess.run(
        [
            sys.executable,
            str(SUITE_PATH),
            "--cases",
            str(cases),
            "--harness",
            str(harness),
            "--no-compare",
            "--suite-results-csv",
            str(tmp_path / f"{case_name}-results.csv"),
            "--suite-summary-json",
            str(summary),
        ],
        cwd=SCRIPT_DIR.parents[2],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env={**os.environ, "PYTHONUNBUFFERED": "1"},
    )
    return result.returncode, json.loads(summary.read_text(encoding="utf-8"))


def assert_suite_cli_verdicts(module) -> None:
    with tempfile.TemporaryDirectory(prefix="local-video-suite-cli-test.") as tmp:
        tmp_path = Path(tmp)
        result_root = module.result_root()
        names = [
            "suite-regression-pass",
            "suite-regression-fail",
            "suite-regression-missing",
        ]
        for name in names:
            run_dir = result_root / name
            if run_dir.exists():
                shutil.rmtree(run_dir)

        harness = tmp_path / "fake-harness.py"
        write_fake_harness(harness, result_root)

        pass_rc, pass_summary = run_suite_cli(
            tmp_path, harness, "suite-regression-pass"
        )
        fail_rc, fail_summary = run_suite_cli(
            tmp_path, harness, "suite-regression-fail"
        )
        missing_rc, missing_summary = run_suite_cli(
            tmp_path, harness, "suite-regression-missing"
        )

        assert pass_rc == 0
        assert pass_summary["suite_status"] == "PASS_NO_COMPARE"
        assert pass_summary["cases_non_pass"] == 0
        assert pass_summary["cases_missing_summary"] == 0

        assert fail_rc == 4
        assert fail_summary["suite_status"] == "BENCHMARK_FAILED"
        assert fail_summary["cases_non_pass"] == 1
        assert fail_summary["non_pass_cases"][0]["benchmark_status"] == "STUTTERS_DETECTED"

        assert missing_rc == 5
        assert missing_summary["suite_status"] == "MISSING_SUMMARY"
        assert missing_summary["cases_missing_summary"] == 1
        assert missing_summary["missing_summary_cases"] == ["suite-regression-missing"]

        for name in names:
            run_dir = result_root / name
            if run_dir.exists():
                shutil.rmtree(run_dir)


def main() -> int:
    module = load_suite()
    assert_case_summary_fields(module)
    assert_suite_statuses(module)
    assert_write_suite_summary(module)
    assert_subscriber_passthrough_rejected(module)
    assert_suite_cli_verdicts(module)
    print("run-latency-suite regression checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
