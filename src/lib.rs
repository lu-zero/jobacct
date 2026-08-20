//! CPU-time and RSS accounting for a Unix **process group**.
//!
//! Spawn via [`CommandJobExt::spawn_job`]: the child is made a new
//! process-group leader (`CommandExt::process_group(0)`). [`Monitor::sample`]
//! sums live members and latches CPU of pids that have disappeared, so short
//! helpers (cc1, as, ld) are not lost.
//!
//! The crate-root types (`Job`, `Waiter`, `Monitor`) wrap `std::process`.
//! Enable `tokio` for the `jobacct::tokio` façade — it has **no sync
//! `sample`**.
//!
//! Supported: Linux, macOS, FreeBSD. There is no Windows backend.
//!
//! ```no_run
//! use std::process::Command;
//! use std::time::Duration;
//! use jobacct::{CommandJobExt, Event};
//!
//! let job = Command::new("sh").arg("-c").arg("echo ok").spawn_job()?;
//! for ev in job.watch(Duration::from_millis(200)) {
//!     match ev? {
//!         Event::Tick(s) => println!("cpu={:?} rss={}", s.cpu, s.rss),
//!         Event::Exited(e) => println!("done wall={:?} cpu={:?}", e.wall, e.sample.cpu),
//!     }
//! }
//! # Ok::<(), std::io::Error>(())
//! ```

#![cfg_attr(
    not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")),
    allow(unused)
)]

#[cfg(not(unix))]
compile_error!("jobacct supports Linux, macOS, and FreeBSD only (Unix process groups)");

#[cfg(all(
    unix,
    not(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))
))]
compile_error!("jobacct supports Linux, macOS, and FreeBSD only");

use std::time::Duration;

mod acc;
mod std_job;
mod sys;

#[cfg(feature = "tokio")]
mod tokio_job;

/// Tokio `Command` / `Child` façade. `Monitor::sample` is async (`spawn_blocking`).
#[cfg(feature = "tokio")]
pub mod tokio {
    pub use crate::tokio_job::*;
}

pub use acc::Monitor;
pub use std_job::{CommandJobExt, Job, Waiter, Watch};

/// One snapshot of a process group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    /// Σ user+sys of every pid ever seen, latched on death. Can exceed wall time.
    pub cpu: Duration,
    /// Σ RSS of live members, in bytes. Shared pages overcount.
    pub rss: u64,
    /// Max `rss` observed on this handle (bytes).
    pub rss_peak: u64,
    /// Live members in the last scan.
    pub nprocs: u32,
}

/// Final accounting after the leader is reaped.
#[derive(Clone, Debug)]
pub struct Exit {
    pub status: std::process::ExitStatus,
    pub sample: Sample,
    /// Spawn → wait.
    pub wall: Duration,
}

/// Item yielded by [`Job::watch`].
#[derive(Clone, Debug)]
pub enum Event {
    Tick(Sample),
    Exited(Exit),
}

/// Options for [`CommandJobExt::spawn_job_with`].
#[derive(Clone, Debug)]
pub struct JobOptions {
    /// `killpg` the group if `Job`/`Waiter` is dropped before wait. Default `false`.
    pub kill_on_drop: bool,
    /// Poll interval for `wait`/`wait_polling`/`watch`. Default 200ms.
    pub poll: Duration,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            kill_on_drop: false,
            poll: Duration::from_millis(200),
        }
    }
}

impl JobOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn kill_on_drop(mut self, yes: bool) -> Self {
        self.kill_on_drop = yes;
        self
    }

    pub fn poll(mut self, d: Duration) -> Self {
        self.poll = d;
        self
    }
}
