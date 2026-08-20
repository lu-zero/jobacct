# AGENTS.md

## Commands

Before committing, run all of the following and ensure each is clean:

```text
cargo fmt --check
cargo clippy --all-features
cargo doc --no-deps --all-features
cargo test --features tokio
```

## Notes

- Supports Linux, macOS, FreeBSD only. There is no Windows backend.
- `procfs` (Linux), `libproc` + `mach2` (macOS) are target-gated deps.
- macOS CPU comes from `libproc` `pti_total_{user,system}`, which are Mach
  absolute-time ticks — convert with `mach_timebase_info` before use
  (`src/sys_macos.rs`).