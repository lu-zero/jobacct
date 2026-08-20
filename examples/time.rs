//! Tiny `/usr/bin/time`-shaped frontend: spawn a process group, poll, print.
//!
//! ```text
//! cargo run --example time -- sh -c 'dd if=/dev/zero of=/dev/null bs=1M count=50'
//! ```

use std::env;
use std::process::{Command, ExitCode};
use std::time::Duration;

use jobacct::{CommandJobExt, Event, JobOptions};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(prog) = args.next() else {
        eprintln!("usage: time <command> [args...]");
        return ExitCode::from(2);
    };
    let rest: Vec<String> = args.collect();

    let mut cmd = Command::new(prog);
    cmd.args(&rest);

    let job = match cmd.spawn_job_with(&JobOptions::new().poll(Duration::from_millis(100))) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("spawn: {e}");
            return ExitCode::from(127);
        }
    };

    let mut last = None;
    let mut exit = None;
    for ev in job.watch(Duration::from_millis(100)) {
        match ev {
            Ok(Event::Tick(s)) => last = Some(s),
            Ok(Event::Exited(e)) => {
                last = Some(e.sample.clone());
                exit = Some(e);
                break;
            }
            Err(err) => {
                eprintln!("sample: {err}");
                return ExitCode::from(1);
            }
        }
    }

    if let (Some(e), Some(s)) = (exit, last) {
        eprintln!(
            "wall {wall:.3}s  cpu {cpu:.3}s  rss {rss:.1} MB  peak {peak:.1} MB  nproc {n}  status {status}",
            wall = e.wall.as_secs_f64(),
            cpu = s.cpu.as_secs_f64(),
            rss = s.rss as f64 / 1_048_576.0,
            peak = s.rss_peak as f64 / 1_048_576.0,
            n = s.nprocs,
            status = e.status,
        );
        return match e.status.code() {
            Some(c) => ExitCode::from(c as u8),
            None => ExitCode::from(1),
        };
    }
    ExitCode::from(1)
}
