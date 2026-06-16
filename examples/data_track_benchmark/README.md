# Data Track Benchmark

Measures data track delivery ratio, send backpressure, sequence loss, and latency across a matrix of payload sizes, send frequencies, and reliability modes.

A single binary connects to a LiveKit room as both publisher and subscriber (two separate participants), iterates through every `(reliability, payload_size, frequency)` combination, and prints results as CSV to stdout. Tokens are generated automatically from your API key and secret.

## Usage

All connection parameters can be passed as CLI flags or environment variables (flags take precedence).

```sh
cargo run -p data_track_benchmark -- \
  --url wss://your-server.livekit.cloud \
  --api-key your-api-key \
  --api-secret your-api-secret \
  --sizes 1,4,16,64,128,256,512 \
  --frequencies 1,5,10,25,50,100,200,500,1000 \
  --reliability both \
  --max-expected-mibps 64 \
  --reliable-drain-ms 10000 \
  --duration 10 \
  --output results.csv \
  --latency-output latency.csv \
  --publish-output publish.csv
```

Or via environment variables:

```sh
export LIVEKIT_URL="wss://your-server.livekit.cloud"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"

cargo run -p data_track_benchmark -- -s 1,4,16,64 -f 1,5,10,25,1000 -d 10
```

`--reliability` accepts `lossy`, `reliable`, or `both`. The default is `both`.

Use `--max-expected-mibps` to skip cells whose requested payload throughput is too high to be useful. Expected throughput is calculated as `size_kib * frequency_hz / 1024`.

Use `--drain-ms` to control how long the subscriber waits after each send run before collecting delivery stats for every reliability mode. Without `--drain-ms`, lossy runs wait `max(500, 2000 / frequency_hz)` milliseconds and reliable runs wait `10,000` milliseconds by default. Use `--reliable-drain-ms` to tune only reliable runs. For reliable tracks under backpressure, a short drain measures timely delivery, not eventual delivery.

Use `--latency-output` to write one CSV row per received frame. The latency file includes the run parameters, sequence number, send/receive offsets, latency, duplicate marker, and out-of-order marker.

Use `--publish-output` to write one CSV row per publisher send attempt. The publish file includes the run parameters, sequence number, frame size, send offset, API wait time, and whether the send was accepted.

## Local smoke harness

The local harness starts a dev `livekit-server`, runs a small lossy+reliable matrix, and verifies that both modes produced CSV rows.

```sh
examples/data_track_benchmark/run_local_matrix.sh
```

By default it expects the SFU binary at `../livekit/bin/livekit-server` from the Rust SDK checkout. Build it first if needed:

```sh
(cd ../livekit && mage)
```

Common overrides:

```sh
SIZES=1,64 FREQUENCIES=1,25 DURATION=5 OUTPUT_DIR=/tmp/dt-bench \
  examples/data_track_benchmark/run_local_matrix.sh

SIZES=512 FREQUENCIES=100 RELIABILITY=reliable RELIABLE_DRAIN_MS=10000 \
  OUTPUT_DIR=/tmp/dt-bench-512-reliable \
  examples/data_track_benchmark/run_local_matrix.sh

START_LIVEKIT_SERVER=0 LIVEKIT_URL=ws://127.0.0.1:7880 \
  examples/data_track_benchmark/run_local_matrix.sh
```

## Wide matrix

The wide harness sweeps up to `512 KiB` frames and `50,000 Hz` where the requested payload bandwidth stays under the configured cap.

```sh
examples/data_track_benchmark/run_wide_matrix.sh
```

Defaults:

- `SIZES=1,4,16,64,128,256,512`
- `FREQUENCIES=1,10,100,1000,5000,10000,50000`
- `MAX_EXPECTED_MIBPS=64`
- `RELIABLE_DRAIN_MS=10000`
- `DURATION=3`
- `OUTPUT_DIR=/private/tmp/livekit-data-track-bench-wide`

With the default `64 MiB/s` cap, the highest-frequency cells include `1 KiB @ 50,000 Hz`; larger payloads are tested only while their requested payload bandwidth remains under the cap, for example `512 KiB @ 100 Hz`.

## Presentation matrix and report

The presentation harness runs the requested lossy and reliable matrices, captures latency/publish time series, parses SFU data-track stats, and generates `report.pdf`, `report.html`, `report.md`, `timeseries.csv`, and `sfu_timeseries.csv`.

```sh
examples/data_track_benchmark/run_presentation_matrix.sh
```

Defaults:

- Lossy: `LOSSY_SIZES=1,4,8,16,32,64`, `LOSSY_FREQUENCIES=5,10,100,1000,5000`
- Reliable: `RELIABLE_SIZES=16,32,64,128,256,512`, `RELIABLE_FREQUENCIES=10,10,100,500`
- `DURATION=3`
- `LOSSY_DRAIN_MS=500`
- `RELIABLE_DRAIN_MS=10000`
- `BUCKET_MS=1000`
- `OUTPUT_DIR=../data-track-benchmark-report`

The duplicate `10` in `RELIABLE_FREQUENCIES` is preserved as a repeated 10 Hz run and aggregated by the report. Set `MAX_EXPECTED_MIBPS` if you want to cap requested payload throughput during exploratory runs.

The report charts:

- avg/p95/p99 latency from all received messages
- latency over time, to show reliable backpressure buildup
- sent and received throughput over time
- subscriber-observed missing message sequences over time
- SFU cumulative data-track packet stats, plus SFU packet-loss time series when the server was built with `LIVEKIT_DATA_TRACK_STATS_INTERVAL_MS` support

### CLI flags

See help page:
```
cargo run -p data_track_benchmark -- --help
```

## Output

Summary CSV is written to stdout by default, or to `--output` when specified, with one row per `(reliability, size, frequency)` combination. Important columns:

- `attempted`, `sent`, `failed`: publisher-side send attempts and local SDK send failures.
- `avg_send_wait_ms`, `max_send_wait_ms`: time spent in the publisher API call, useful for reliable-track backpressure.
- `received`, `unique_received`, `duplicate`, `out_of_order`, `missing_sequence`: subscriber-side delivery and sequence accounting.
- `avg_latency_ms`, `p50_latency_ms`, `p95_latency_ms`, `p99_latency_ms`: subscriber-observed end-to-end latency from frame creation timestamp.
- `drain_ms`: post-send collection wait used for that row.

When `--latency-output` is set, latency samples are written as a second CSV with one row per received frame. Important columns:

- `run_id`, `reliability`, `size_kb`, `freq_hz`, `duration_s`: identifies the summary row the sample belongs to.
- `seq`, `receive_index`: frame sequence and subscriber receive order.
- `send_elapsed_ms`, `receive_elapsed_ms`, `latency_ms`: latency over time for backlog analysis.
- `duplicate`, `out_of_order`: per-sample sequence status.

The local and wide harnesses write `results.csv` and `latency.csv` into `OUTPUT_DIR`.
The presentation harness also writes `publish.csv`, `timeseries.csv`, `sfu_timeseries.csv`, and presentation reports.
