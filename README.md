# jobacct

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![Build Status](https://github.com/lu-zero/jobacct/workflows/CI/badge.svg)](https://github.com/lu-zero/jobacct/actions?query=workflow:CI)
[![dependency status](https://deps.rs/repo/github/lu-zero/jobacct/status.svg)](https://deps.rs/repo/github/lu-zero/jobacct)

CPU-time and RSS accounting for a Unix **process group**, for supervising
parallel builds (and anything else that is `Command` + children). Linux,
macOS, FreeBSD. Not Windows.

The child is spawned as a new process-group leader. `sample()` sums live
members and **latches CPU of pids that vanish**, so `cc1`/`as`/`ld` still
count. Peak RSS is the max of those live sums (shared pages overcount).

> **Warning**: This codebase is currently mainly slop-coded and has not yet
> been thoroughly audited. Use at your own risk.

## Usage

```rust
use jobacct::{CommandJobExt, JobOptions};
use std::process::Command;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut job = Command::new("rustc")
        .arg("lib.rs")
        .spawn_job_with(&JobOptions::new().kill_on_drop(true))?;

    // parallel supervisor: wait in one place, sample from a timer
    let (mut waiter, mon) = job.split();
    let snap = mon.sample()?;
    println!("cpu={:?} rss={} peak={}", snap.cpu, snap.rss, snap.rss_peak);
    let exit = waiter.wait_polling(Duration::from_millis(200))?;
    Ok(())
}
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

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)

## Contributing

See [AGENTS.md](AGENTS.md) for project conventions (build commands, style,
checks).

## Author

Luca Barbato <lu_zero@gentoo.org>