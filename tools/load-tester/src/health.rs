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

#[cfg(target_os = "linux")]
fn own_udp_drops() -> Option<HashMap<u64, u64>> {
    use std::os::unix::fs::MetadataExt;
    const DROPS: usize = libc::SK_MEMINFO_DROPS as usize;
    let fds = std::fs::read_dir("/proc/self/fd").ok()?;
    let drops = fds.flatten().filter_map(|entry| {
        let fd = entry.file_name().to_str()?.parse().ok()?;
        let [protocol] = socket_option(fd, SocketOption::Protocol)?;
        if protocol != libc::IPPROTO_UDP as u32 {
            return None;
        }
        let inode = std::fs::metadata(entry.path()).ok()?.ino();
        let meminfo: [u32; DROPS + 1] = socket_option(fd, SocketOption::MemInfo)?;
        Some((inode, meminfo[DROPS].into()))
    });
    Some(drops.collect())
}

#[cfg(not(target_os = "linux"))]
fn own_udp_drops() -> Option<HashMap<u64, u64>> {
    None
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum SocketOption {
    Protocol = libc::SO_PROTOCOL as isize,
    MemInfo = libc::SO_MEMINFO as isize,
}

#[cfg(target_os = "linux")]
fn socket_option<const N: usize>(fd: libc::c_int, option: SocketOption) -> Option<[u32; N]> {
    let mut value = [0u32; N];
    let mut len = std::mem::size_of_val(&value) as libc::socklen_t;
    let buf = value.as_mut_ptr().cast();
    let name = option as libc::c_int;
    // SAFETY: for SO_PROTOCOL and SO_MEMINFO the kernel copies at most `len` bytes into `value`,
    // and any bytes form valid u32s.
    let ok = unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, name, buf, &mut len) } == 0;
    ok.then_some(value)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{
        net::UdpSocket,
        os::{fd::AsRawFd, unix::fs::MetadataExt},
    };

    use super::*;

    #[test]
    fn a_full_receive_buffer_counts_each_further_datagram_as_a_drop() {
        let sink = UdpSocket::bind("127.0.0.1:0").unwrap();
        let tiny: libc::c_int = 4096;
        let opt = (&tiny as *const libc::c_int).cast();
        let len = std::mem::size_of_val(&tiny) as libc::socklen_t;
        // SAFETY: `opt` points at a live c_int of `len` bytes, which setsockopt only reads.
        let set = unsafe {
            libc::setsockopt(sink.as_raw_fd(), libc::SOL_SOCKET, libc::SO_RCVBUF, opt, len)
        };
        assert_eq!(set, 0);
        let path = format!("/proc/self/fd/{}", sink.as_raw_fd());
        let inode = std::fs::metadata(path).unwrap().ino();
        let source = UdpSocket::bind("127.0.0.1:0").unwrap();
        let to = sink.local_addr().unwrap();
        let send = |n| (0..n).for_each(|_| assert_eq!(source.send_to(&[7; 64], to).unwrap(), 64));
        let drops = || own_udp_drops().unwrap()[&inode];

        send(100);
        let full = drops();
        assert!(full > 0, "100 unread datagrams overflow a tiny receive buffer");
        send(100);
        assert_eq!(drops() - full, 100, "a full buffer drops every further datagram");
    }
}
