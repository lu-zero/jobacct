use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use jobacct::{CommandJobExt, Event, JobOptions};

fn quiet_sh(script: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c")
        .arg(script)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c
}

#[test]
fn echo_exits_clean() {
    let mut job = quiet_sh("exit 0")
        .spawn_job_with(&JobOptions::new().poll(Duration::from_millis(50)))
        .unwrap();
    let exit = job.wait().unwrap();
    assert!(exit.status.success());
    assert!(exit.wall < Duration::from_secs(5));
}

#[test]
fn busy_loop_accumulates_cpu() {
    let mut job = quiet_sh("while true; do :; done")
        .spawn_job_with(
            &JobOptions::new()
                .poll(Duration::from_millis(50))
                .kill_on_drop(true),
        )
        .unwrap();
    thread::sleep(Duration::from_millis(400));
    let s = job.sample().unwrap();
    assert!(
        s.cpu >= Duration::from_millis(50),
        "cpu was only {:?}",
        s.cpu
    );
    assert!(s.rss > 0, "rss was 0");
    assert!(s.nprocs >= 1);
    job.kill().unwrap();
    let exit = job.wait().unwrap();
    assert!(exit.sample.rss_peak > 0);
}

#[test]
fn process_group_has_several_members() {
    // sh + two sleeps, all in the same pgrp.
    let mut job = quiet_sh("sleep 30 & sleep 30 & wait")
        .spawn_job_with(
            &JobOptions::new()
                .poll(Duration::from_millis(50))
                .kill_on_drop(true),
        )
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    let s = job.sample().unwrap();
    assert!(
        s.nprocs >= 3,
        "expected sh+2 sleeps in pgrp, got nprocs={}",
        s.nprocs
    );
    job.kill().unwrap();
    let _ = job.wait();
}

#[test]
fn watch_yields_exit() {
    let job = quiet_sh("exit 3").spawn_job().unwrap();
    let mut saw_exit = false;
    for ev in job.watch(Duration::from_millis(30)) {
        match ev.unwrap() {
            Event::Tick(_) => {}
            Event::Exited(e) => {
                saw_exit = true;
                assert_eq!(e.status.code(), Some(3));
            }
        }
    }
    assert!(saw_exit);
}

#[test]
fn split_monitor_survives_wait() {
    let job = quiet_sh("sleep 0.2")
        .spawn_job_with(&JobOptions::new().poll(Duration::from_millis(40)))
        .unwrap();
    let (mut waiter, mon) = job.split();
    let s = mon.sample().unwrap();
    assert!(s.nprocs >= 1);
    let exit = waiter.wait().unwrap();
    assert!(exit.status.success());
}

#[cfg(feature = "tokio")]
mod tokio_tests {
    use super::*;
    use jobacct::tokio::CommandJobExt as TokioExt;

    #[tokio::test]
    async fn tokio_busy_cpu() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg("while true; do :; done")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut job = cmd
            .spawn_job_with(
                &JobOptions::new()
                    .poll(Duration::from_millis(50))
                    .kill_on_drop(true),
            )
            .unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
        let s = job.sample().await.unwrap();
        assert!(s.cpu >= Duration::from_millis(50), "cpu {:?}", s.cpu);
        job.kill().unwrap();
        let _ = job.wait().await;
    }
}
