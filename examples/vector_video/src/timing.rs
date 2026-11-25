//! Optional per-frame traces shared by the transport, camera and native viewer.
use anyhow::{Context, Result};
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Returns wall time in microseconds for joining publisher and subscriber traces.
pub(crate) fn now_us() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64
}

/// An optional buffered CSV writer. Only enabled traces acquire the file lock.
#[derive(Clone, Default)]
pub(crate) struct Trace(Option<Arc<Mutex<BufWriter<File>>>>);

impl Trace {
    /// Creates a new trace without replacing an existing benchmark.
    pub(crate) fn open(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else { return Ok(Self::default()) };
        let mut writer = BufWriter::new(
            File::create_new(path)
                .with_context(|| format!("Could not create latency trace {}", path.display()))?,
        );
        writeln!(writer, "frame,event,unix_us")?;
        Ok(Self(Some(Arc::new(Mutex::new(writer)))))
    }

    /// Writes a source-indexed stage timestamp, including timestamps from the camera worker.
    pub(crate) fn record(&self, frame: u32, event: &str, us: u64) -> Result<()> {
        if let Some(writer) = &self.0 {
            writeln!(writer.lock().expect("latency writer poisoned"), "{frame},{event},{us}")?;
        }
        Ok(())
    }

    /// Flushes a completed measurement, propagating disk errors to the caller.
    pub(crate) fn finish(&self) -> Result<()> {
        if let Some(writer) = &self.0 {
            writer.lock().expect("latency writer poisoned").flush()?;
        }
        Ok(())
    }
}

/// Timestamp exchange between the two application endpoints on the LiveKit RPC path.
pub(crate) const CLOCK_METHOD: &str = "vector-video.clock.v1";

/// Maps subscriber monotonic time to publisher Unix time using the midpoint of a peer round trip.
#[derive(Clone, Copy)]
pub(crate) struct ClockSample {
    midpoint: Instant,
    publisher_us: u64,
    rtt: Duration,
}

impl ClockSample {
    /// Ages out old offsets so a disconnected clock cannot continue producing latency readings.
    pub(crate) fn publisher_time(self, now: Instant) -> Option<u64> {
        let age = now.checked_duration_since(self.midpoint)?;
        if age > Duration::from_secs(10) {
            return None;
        }
        self.publisher_us.checked_add(u64::try_from(age.as_micros()).ok()?)
    }
}

/// Keeps recent midpoint samples so queueing on a single probe has limited influence.
#[derive(Default)]
struct ClockHistory(VecDeque<ClockSample>);

impl ClockHistory {
    /// Selects the shortest fresh round trip and restarts calibration if the peer clock steps.
    fn observe(&mut self, start: Instant, end: Instant, publisher_us: u64) -> ClockSample {
        let rtt = end - start;
        let midpoint = start + rtt / 2;
        let sample = ClockSample { midpoint, publisher_us, rtt };
        self.0.retain(|prior| prior.publisher_time(end).is_some());
        if let Some(prior) = self.0.back() {
            let uncertainty = (prior.rtt + rtt) / 2 + Duration::from_millis(5);
            if prior.publisher_time(midpoint).is_some_and(|predicted| {
                Duration::from_micros(predicted.abs_diff(publisher_us)) > uncertainty
            }) {
                self.0.clear();
            }
        }
        if self.0.len() == 5 {
            self.0.pop_front();
        }
        self.0.push_back(sample);
        *self.0.iter().min_by_key(|s| s.rtt).expect("just added a clock sample")
    }
}

/// Repeatedly measures one application RPC round trip without waiting in the video receive loop.
pub(crate) async fn probe_clock(
    local: &livekit::prelude::LocalParticipant,
    publisher: &str,
    mut report: impl FnMut(ClockSample, Duration),
) {
    use livekit::prelude::PerformRpcData;
    let mut samples = ClockHistory::default();
    let mut warned = false;
    loop {
        let request = PerformRpcData::new(publisher, CLOCK_METHOD)
            .with_response_timeout(Duration::from_secs(2))
            .with_max_round_trip_latency(Duration::from_secs(2));
        let start = Instant::now();
        let response = local.perform_rpc(request).await;
        let end = Instant::now();
        match response {
            Ok(reply) => {
                if let Ok(publisher_us) = reply.parse::<u64>() {
                    let rtt = end - start;
                    report(samples.observe(start, end, publisher_us), rtt);
                    warned = false;
                }
            }
            Err(error) if !warned => {
                eprintln!("Publisher clock probe unavailable: {error}");
                warned = true;
            }
            Err(_) => {}
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A slow probe cannot bias an existing fast estimate, while a clock step invalidates it.
    #[test]
    fn calibrates_peer_time_and_expires_old_samples() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let mut history = ClockHistory::default();
        let fast = history.observe(start, start + ms(8), 2_000_000);
        assert_eq!(fast.publisher_time(start + ms(20)), Some(2_016_000));
        let best = history.observe(start + ms(100), start + ms(180), 2_160_000);
        assert_eq!(best.rtt, ms(8));
        assert_eq!(best.publisher_time(start + ms(200)), Some(2_196_000));
        let stepped = history.observe(start + ms(300), start + ms(320), 9_000_000);
        assert_eq!(stepped.publisher_time(start + ms(320)), Some(9_010_000));
        assert_eq!(stepped.publisher_time(start + Duration::from_secs(11)), None);
    }
}
