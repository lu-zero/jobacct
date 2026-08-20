use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::sys::{self, Member};
use crate::Sample;

/// Cheap, `Clone` handle that samples a process group without borrowing the
/// leader [`std::process::Child`]. Always sync; the tokio façade wraps it in
/// `spawn_blocking`.
#[derive(Clone, Debug)]
pub struct Monitor {
    pub(crate) pgid: i32,
    acc: Arc<Mutex<Acc>>,
}

#[derive(Debug, Default)]
struct Acc {
    /// Last CPU per pid. Dead pids stay, so short-lived children count after
    /// they vanish from the kernel table.
    cpu_by_pid: HashMap<u32, Duration>,
    rss_peak: u64,
}

impl Monitor {
    pub(crate) fn new(pgid: i32) -> Self {
        Self {
            pgid,
            acc: Arc::new(Mutex::new(Acc::default())),
        }
    }

    pub fn pgid(&self) -> i32 {
        self.pgid
    }

    /// Scan the process group. Sync on purpose — see `jobacct::tokio`.
    pub fn sample(&self) -> std::io::Result<Sample> {
        let members = sys::members(self.pgid)?;
        let mut acc = self.acc.lock().unwrap_or_else(|e| e.into_inner());
        Ok(acc.observe(&members))
    }
}

impl Acc {
    fn observe(&mut self, live: &[Member]) -> Sample {
        let mut rss = 0u64;
        for m in live {
            self.cpu_by_pid.insert(m.pid, m.cpu);
            rss = rss.saturating_add(m.rss);
        }
        let cpu = self
            .cpu_by_pid
            .values()
            .fold(Duration::ZERO, |a, b| a.saturating_add(*b));
        self.rss_peak = self.rss_peak.max(rss);
        Sample {
            cpu,
            rss,
            rss_peak: self.rss_peak,
            nprocs: live.len() as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latches_dead_pid_cpu() {
        let mut acc = Acc::default();
        let s1 = acc.observe(&[Member {
            pid: 1,
            cpu: Duration::from_millis(10),
            rss: 100,
        }]);
        assert_eq!(s1.cpu, Duration::from_millis(10));
        assert_eq!(s1.nprocs, 1);

        let s2 = acc.observe(&[Member {
            pid: 2,
            cpu: Duration::from_millis(5),
            rss: 50,
        }]);
        assert_eq!(s2.cpu, Duration::from_millis(15));
        assert_eq!(s2.rss, 50);
        assert_eq!(s2.rss_peak, 100);
        assert_eq!(s2.nprocs, 1);
    }
}
