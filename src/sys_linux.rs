use std::io;
use std::time::Duration;

use procfs::process::Process;

use super::Member;

pub(crate) fn members(pgid: i32) -> io::Result<Vec<Member>> {
    let ticks = procfs::ticks_per_second() as f64;
    if ticks <= 0.0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "CLK_TCK is zero",
        ));
    }
    let page = procfs::page_size();

    let iter = procfs::process::all_processes().map_err(proc_err)?;
    let mut out = Vec::new();
    for proc in iter {
        let proc: Process = match proc {
            Ok(p) => p,
            Err(_) => continue,
        };
        let stat = match proc.stat() {
            Ok(s) => s,
            Err(_) => continue,
        };
        if stat.pgrp != pgid {
            continue;
        }
        let cpu_secs = (stat.utime + stat.stime) as f64 / ticks;
        out.push(Member {
            pid: stat.pid.max(0) as u32,
            cpu: Duration::from_secs_f64(cpu_secs.max(0.0)),
            rss: stat.rss.saturating_mul(page),
        });
    }
    Ok(out)
}

fn proc_err(e: procfs::ProcError) -> io::Error {
    match e {
        procfs::ProcError::Io(err, _) => err,
        other => io::Error::new(io::ErrorKind::Other, other),
    }
}
