use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use load_tester::{
    media::Pump,
    record::{now_ms, HealthRecord, Record, ThreadLoad},
};
use tokio::{
    sync::{mpsc, watch},
    time::{interval, MissedTickBehavior},
};

use crate::WINDOW;

const PROBE: Duration = Duration::from_millis(100);

pub async fn run(
    worker: u16,
    out: mpsc::Sender<Record>,
    mut stop: watch::Receiver<bool>,
    pump: Arc<Pump>,
) {
    let mut probe = interval(PROBE);
    probe.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut baseline = Baseline::take().await;
    let mut lag_max = Duration::ZERO;
    loop {
        let due = tokio::select! {
            _ = stop.changed() => return,
            due = probe.tick() => due,
        };
        lag_max = lag_max.max(due.elapsed());
        if baseline.at.elapsed() < WINDOW {
            continue;
        }
        let now = Baseline::take().await;
        let record = baseline.until(&now, worker, lag_max.max(pump.lateness()));
        if out.send(Record::Health(record)).await.is_err() {
            return;
        }
        baseline = now;
        lag_max = Duration::ZERO;
    }
}

struct Baseline {
    at: Instant,
    cpu: Duration,
    threads: HashMap<u32, ThreadTime>,
    udp_drops: Option<HashMap<u64, u64>>,
}

struct ThreadTime {
    name: String,
    cpu: Duration,
}

impl Baseline {
    async fn take() -> Self {
        let scan = || (thread_times(), own_udp_drops());
        let (threads, udp_drops) = tokio::task::spawn_blocking(scan).await.unwrap_or_default();
        Self { at: Instant::now(), cpu: process_cpu(), threads, udp_drops }
    }

    fn until(&self, now: &Self, worker: u16, lag_max: Duration) -> HealthRecord {
        let wall = now.at.saturating_duration_since(self.at).max(Duration::from_millis(1));
        let util = |cpu: Duration| cpu.as_secs_f32() / wall.as_secs_f32();
        let hottest_thread = now
            .threads
            .iter()
            .filter_map(|(tid, t)| {
                let before = self.threads.get(tid)?;
                Some(ThreadLoad {
                    name: t.name.clone(),
                    util: util(t.cpu.saturating_sub(before.cpu)),
                })
            })
            .max_by(|a, b| a.util.total_cmp(&b.util));
        let udp_drops = match (&self.udp_drops, &now.udp_drops) {
            (Some(before), Some(after)) => Some(
                after
                    .iter()
                    .map(|(inode, d)| d.saturating_sub(*before.get(inode).unwrap_or(&0)))
                    .sum(),
            ),
            _ => None,
        };
        HealthRecord {
            at: now_ms(),
            dur_ms: wall.as_millis() as u32,
            worker,
            cpu_cores: util(now.cpu.saturating_sub(self.cpu)),
            lag_max_ms: lag_max.as_secs_f32() * 1000.0,
            hottest_thread,
            udp_drops,
        }
    }
}

fn process_cpu() -> Duration {
    // SAFETY: an all-zero rusage is a valid value, and getrusage only writes into it.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: the pointer is to a live, writable rusage; RUSAGE_SELF has no other preconditions.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return Duration::ZERO;
    }
    let secs = |t: libc::timeval| {
        Duration::new(t.tv_sec.max(0) as u64, t.tv_usec.clamp(0, 999_999) as u32 * 1000)
    };
    secs(usage.ru_utime) + secs(usage.ru_stime)
}

#[cfg(target_os = "linux")]
fn thread_times() -> HashMap<u32, ThreadTime> {
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return HashMap::new();
    };
    tasks
        .flatten()
        .filter_map(|task| {
            let tid = task.file_name().to_str()?.parse().ok()?;
            let name = std::fs::read_to_string(task.path().join("comm")).ok()?;
            let schedstat = std::fs::read_to_string(task.path().join("schedstat")).ok()?;
            let on_cpu_ns = schedstat.split_whitespace().next()?.parse().ok()?;
            let time = ThreadTime {
                name: name.trim_end().to_string(),
                cpu: Duration::from_nanos(on_cpu_ns),
            };
            Some((tid, time))
        })
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn thread_times() -> HashMap<u32, ThreadTime> {
    HashMap::new()
}

// the tables list every socket in the network namespace, so keep the ones this process holds
#[cfg(target_os = "linux")]
fn own_udp_drops() -> Option<HashMap<u64, u64>> {
    let own: std::collections::HashSet<u64> = std::fs::read_dir("/proc/self/fd")
        .ok()?
        .flatten()
        .filter_map(|fd| {
            let target = std::fs::read_link(fd.path()).ok()?;
            target.to_str()?.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
        })
        .collect();
    let mut drops = HashMap::new();
    for table in ["/proc/self/net/udp", "/proc/self/net/udp6"] {
        let text = std::fs::read_to_string(table).unwrap_or_default();
        drops.extend(drops_by_inode(&text).filter(|(inode, _)| own.contains(inode)));
    }
    Some(drops)
}

#[cfg(not(target_os = "linux"))]
fn own_udp_drops() -> Option<HashMap<u64, u64>> {
    None
}

#[cfg(any(target_os = "linux", test))]
fn drops_by_inode(table: &str) -> impl Iterator<Item = (u64, u64)> + '_ {
    table.lines().skip(1).filter_map(|row| {
        let mut columns = row.split_whitespace();
        let inode = columns.nth(9)?.parse().ok()?;
        let drops = columns.nth(2)?.parse().ok()?;
        Some((inode, drops))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_column_is_read_per_socket_inode() {
        let udp = "\
   sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops
 1090: 0100007F:9291 00000000:0000 07 00000000:00034500 00:00000000 00000000     0        0 1548516 2 000000005412b34b 19907
 2734: 0100007F:E8FD 00000000:0000 07 00000000:00000000 00:00000000 00000000     0        0 1548517 2 0000000016fa28f7 0
";
        let rows: Vec<(u64, u64)> = drops_by_inode(udp).collect();
        assert_eq!(rows, [(1548516, 19907), (1548517, 0)]);
    }
}
