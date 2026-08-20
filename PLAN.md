# jobacct v1

Unix process-group CPU + RSS for a spawned `Command`. Live sample while
running, final totals at wait. Linux, macOS, FreeBSD. No Windows.

This is the crate sketched in the design thread: not `sysinfo`, not `wait4`
as a group total, not cgroups.

## Goals

- Extension of `std::process::Command` (and `tokio::process::Command`).
- The unit of accounting is a **process group**, not one pid.
- `Monitor::sample` reports Σ CPU (user+sys) and Σ RSS of live members,
  **latching CPU of pids that have since vanished**.
- Peak RSS is `max` of those live sums (shared pages overcount; documented).
- A parallel-build supervisor can `split()` a job, `wait` in one task, and
  `sample` the rest from a timer.

## Non-goals (v1)

- Windows job objects
- cgroup / PSS
- `%` CPU (that's a delta of `Sample.cpu` / wall; callers compute it)
- `sysinfo`, `procps`, `procs`
- `wait4` as the group total
- Generic async runtimes other than tokio
- Replacing a user `pre_exec` (we use `CommandExt::process_group(0)`)

## API

```text
Command::spawn_job() / spawn_job_with(&JobOptions)
Job { id, stdio, sample, monitor, split, kill, wait, wait_polling, watch }
split() -> (Waiter, Monitor)

Monitor  sample()              // std: sync &self
                               // tokio: async &self via spawn_blocking
Waiter   try_wait / wait / wait_polling
Watch    Iterator (std) / async wait_polling (tokio)

Sample { cpu, rss, rss_peak, nprocs }
Exit   { status, sample, wall }
Event  { Tick(Sample), Exited(Exit) }
```

`wait()` **is** `wait_polling(opts.poll)`. A blocking `Child::wait` without
sampling misses CPU/RSS after the last snapshot and reaps the leader so
`/proc` is gone. The poll loop is `sample` then `try_wait` then sleep, so a
zombie leader is still visible for the last sample.

Do not `Deref` to `Child`. Do not call `Child::wait` yourself.

## Backends

| OS | Group members | CPU | RSS |
|---|---|---|---|
| Linux | `/proc` scan, `stat.pgrp` | `utime+stime` / `CLK_TCK` | `rss * page_size` |
| macOS | `libproc` `ProcFilter::ByProgramGroup` | `pti_total_user+system` (Mach ticks → ns via timebase) | `pti_resident_size` bytes |
| FreeBSD | `sysctl KERN_PROC_PGRP` | `ki_rusage` timeval | `ki_rssize * page` |

Kill is `killpg(pgid, SIGKILL)`. Wait waits the **leader** only, then one
last sample of whoever is still in the group.

## Async

There is no sync `sample` on `jobacct::tokio::*`. Tokio `Monitor::sample`
is `async` and runs the same scan in `spawn_blocking` (one task per group
sample, not per pid). Darwin/FreeBSD have no non-blocking group scan.

A parallel supervisor should `split()`, not `select!` on `&mut Job`.

## Files

```text
src/lib.rs         Sample, Exit, Event, JobOptions, compile_error
src/acc.rs         dead-pid CPU latch + rss_peak
src/sys.rs         members(pgid) → Vec<Member>
src/sys_linux.rs
src/sys_macos.rs
src/sys_freebsd.rs
src/std_job.rs     std Command / Child
src/tokio_job.rs   feature = "tokio"
examples/time.rs   /usr/bin/time-shaped CLI
```
