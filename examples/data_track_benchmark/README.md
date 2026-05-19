# Data Track Benchmark

Measures data track delivery ratio and latency across a matrix of payload sizes and send frequencies.

A single binary connects to a LiveKit room as both publisher and subscriber (two separate participants), iterates through every (payload_size, frequency) combination, and prints results as CSV to stdout. Tokens are generated automatically from your API key and secret.

## Usage

All connection parameters can be passed as CLI flags or environment variables (flags take precedence).

```sh
cargo run -p data_track_benchmark -- \
  --url wss://your-server.livekit.cloud \
  --api-key your-api-key \
  --api-secret your-api-secret \
  --sizes 1,4,16,64,128,256,512 \
  --frequencies 1,5,10,25,50,100,200,500,1000 \
  --duration 10
```

Or via environment variables:

```sh
export LIVEKIT_URL="wss://your-server.livekit.cloud"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"

cargo run -p data_track_benchmark -- -s 1,4,16,64 -f 1,5,10,25,1000 -d 10
```

### CLI flags

| Flag | Short | Env var | Description | Default |
|------|-------|---------|-------------|---------|
| `--url` | | `LIVEKIT_URL` | LiveKit server URL | (required) |
| `--api-key` | | `LIVEKIT_API_KEY` | API key for token generation | (required) |
| `--api-secret` | | `LIVEKIT_API_SECRET` | API secret for token generation | (required) |
| `--sizes` | `-s` | | Comma-separated payload sizes in KiB | (required) |
| `--frequencies` | `-f` | | Comma-separated send frequencies in Hz | (required) |
| `--duration` | `-d` | | Seconds to send per combination | `10` |
| `--room` | `-r` | | LiveKit room name | `data-track-benchmark` |

## Output

CSV printed to stdout with one row per (size, frequency) combination:

```
payload_size_kb,frequency_hz,duration_s,sent,received,delivery_ratio,avg_latency_ms,min_latency_ms,max_latency_ms
1,1,10,10,10,1.00,12.30,8.00,18.00
1,5,10,50,50,1.00,11.50,7.00,20.00
64,1000,10,10000,3300,0.33,45.20,12.00,120.00
...
```

Pipe to a file for further analysis:

```sh
cargo run -p data_track_benchmark -- -s 1,4,16,64 -f 1,10,100,1000 -d 10 > results.csv
```
