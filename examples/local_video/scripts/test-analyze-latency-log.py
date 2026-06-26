#!/usr/bin/env python3
"""Regression checks for local_video latency log analysis."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
ANALYZER_PATH = SCRIPT_DIR / "analyze-latency-log.py"


def load_analyzer():
    spec = importlib.util.spec_from_file_location("analyze_latency_log", ANALYZER_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Unable to load {ANALYZER_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def window(module, source: str, timestamp: str, stutters: int = 0, **values):
    return module.WindowRow(
        source=source,
        timestamp=timestamp,
        frames=30,
        stutters_over_threshold=stutters,
        values={key: str(value) for key, value in values.items()},
    )


def stutter(module, timestamp: str):
    return module.StutterRow(
        timestamp=timestamp,
        values={"paint_gap": "80.0ms"},
        dominant_stage="prepare_to_paint",
    )


def assert_headless_ignores_render_only_signals(module) -> None:
    result = module.smoothness_signal_distribution(
        subscriber_windows=[
            window(module, "subscriber", "2026-06-26T00:00:01Z", stutters=3)
        ],
        subscriber_sink_windows=[
            window(
                module,
                "subscriber_sink",
                "2026-06-26T00:00:00Z",
                replaced_before_render=5,
                dropped_late_before_render=2,
            )
        ],
        stutters=[
            stutter(module, "2026-06-26T00:00:01Z"),
            stutter(module, "2026-06-26T00:00:02Z"),
        ],
        include_render_signals=False,
    )

    assert result["smoothness_signal_log_rows"] == 0
    assert result["smoothness_signal_windows"] == 0
    assert result["smoothness_signal_sources"] == {}
    assert result["smoothness_detail_log_rows"] == 0
    assert result["smoothness_detail_log_rows_ignored"] == 2
    assert result["first_smoothness_signal_timestamp"] is None
    assert result["post_signal_clean_tail_seconds"] is None


def assert_visible_counts_render_and_visual_drop_signals(module) -> None:
    result = module.smoothness_signal_distribution(
        subscriber_windows=[
            window(module, "subscriber", "2026-06-26T00:00:01Z", stutters=3)
        ],
        subscriber_sink_windows=[
            window(
                module,
                "subscriber_sink",
                "2026-06-26T00:00:00Z",
                replaced_before_render=5,
                dropped_late_before_render=2,
            )
        ],
        stutters=[
            stutter(module, "2026-06-26T00:00:01Z"),
            stutter(module, "2026-06-26T00:00:02Z"),
        ],
        include_render_signals=True,
    )

    assert result["smoothness_signal_log_rows"] == 2
    assert result["smoothness_signal_windows"] == 2
    assert result["smoothness_signal_sources"] == {
        "subscriber_render": 3,
        "subscriber_sink": 7,
    }
    assert result["smoothness_detail_log_rows"] == 2
    assert result["smoothness_detail_log_rows_ignored"] == 0
    assert result["first_smoothness_signal_timestamp"] == "2026-06-26T00:00:00Z"
    assert result["last_smoothness_signal_timestamp"] == "2026-06-26T00:00:01Z"
    assert result["post_signal_clean_tail_seconds"] == 0.0


def assert_headless_render_budgets_are_inapplicable(module) -> None:
    result = module.latency_budget_summary(
        {
            "coverage": {"subscriber_render_coverage_required": False},
            "publisher": {"capture_to_packetize_p95_window_max_ms": 12.0},
            "subscriber": {"sink_gap_p95_window_max_ms": 42.0},
        },
        {
            "max_sink_gap_p95_ms": 100.0,
            "max_e2e_p95_ms": 75.0,
            "max_capture_to_packetize_p95_ms": 25.0,
        },
    )

    assert result["status"] == "OK"
    assert result["violations"] == []
    assert result["missing"] == []
    assert result["observed_ms"] == {
        "max_sink_gap_p95_ms": 42.0,
        "max_e2e_p95_ms": None,
        "max_capture_to_packetize_p95_ms": 12.0,
    }
    assert result["inapplicable"] == [
        {
            "key": "max_e2e_p95_ms",
            "label": "e2e p95 window max",
            "threshold_ms": 75.0,
            "reason": "headless run does not emit subscriber render latency windows",
        }
    ]


def assert_visible_missing_render_budgets_are_unknown(module) -> None:
    result = module.latency_budget_summary(
        {"coverage": {"subscriber_render_coverage_required": True}, "subscriber": {}},
        {"max_e2e_p95_ms": 75.0},
    )

    assert result["status"] == "UNKNOWN"
    assert result["violations"] == []
    assert result["missing"] == [
        {
            "key": "max_e2e_p95_ms",
            "label": "e2e p95 window max",
            "threshold_ms": 75.0,
        }
    ]
    assert result["inapplicable"] == []


def main() -> int:
    module = load_analyzer()
    assert_headless_ignores_render_only_signals(module)
    assert_visible_counts_render_and_visual_drop_signals(module)
    assert_headless_render_budgets_are_inapplicable(module)
    assert_visible_missing_render_budgets_are_unknown(module)
    print("analyze-latency-log regression checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
