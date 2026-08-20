# jobacct

CPU-time and RSS of a **process group**, for supervising parallel builds
(and anything else that is `Command` + children).

Linux, macOS, FreeBSD. Not Windows. Not `sysinfo`.

The child is spawned as a new process-group leader. `sample()` sums live
members and **latches CPU of pids that vanish**, so `cc1`/`as`/`ld` still
count. Peak RSS is the max of those live sums (shared pages overcount).

```rust
use std::process::Command;
use std::time::Duration;
use jobacct::{CommandJobExt, JobOptions};

let mut job = Command::new("rustc")
    .arg("lib.rs")
    .spawn_job_with(&JobOptions::new().kill_on_drop(true))?;

// parallel supervisor: wait in one place, sample from a timer
let (mut waiter, mon) = job.split();
let snap = mon.sample()?;
println!("cpu={:?} rss={} peak={}", snap.cpu, snap.rss, snap.rss_peak);
let exit = waiter.wait_polling(Duration::from_millis(200))?;
```

Tokio (`--features tokio`): same shape, but `jobacct::tokio::Monitor::sample`
is **async** (`spawn_blocking`). There is no sync `sample` on that type.

See [PLAN.md](PLAN.md) for the v1 contract (backends, what `wait()` means,
why `wait4` is not the group total).

```text
cargo test
cargo test --features tokio
cargo run --example time -- sh -c 'echo hello'
```
