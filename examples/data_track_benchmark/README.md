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
  --duration 10 \
  --output results.csv
```

Or via environment variables:

```sh
export LIVEKIT_URL="wss://your-server.livekit.cloud"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"

cargo run -p data_track_benchmark -- -s 1,4,16,64 -f 1,5,10,25,1000 -d 10
```

### CLI flags

See help page:
```
cargo run -p data_track_benchmark -- --help
```

## Output

CSV is written to stdout by default, or to `--output` when specified, with one row per (size, frequency) combination.
