//! Tokio façade. No sync `sample`; the Unix scan runs in `spawn_blocking`.
//! [`Job::split`] so a supervisor can wait in one task and sample from another.

use std::cell::Cell;
use std::io;
use std::time::{Duration, Instant};

use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

use crate::sys::{self, kill_group};
use crate::{Exit, JobOptions, Sample};

/// Cloneable sampler. `sample` is always async on this type.
#[derive(Clone, Debug)]
pub struct Monitor {
    inner: crate::Monitor,
}

impl Monitor {
    pub fn pgid(&self) -> i32 {
        self.inner.pgid()
    }

    pub async fn sample(&self) -> io::Result<Sample> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.sample())
            .await
            .map_err(io::Error::other)?
    }
}

pub trait CommandJobExt {
    fn spawn_job(&mut self) -> io::Result<Job>;
    fn spawn_job_with(&mut self, opts: &JobOptions) -> io::Result<Job>;
}

impl CommandJobExt for Command {
    fn spawn_job(&mut self) -> io::Result<Job> {
        self.spawn_job_with(&JobOptions::default())
    }

    fn spawn_job_with(&mut self, opts: &JobOptions) -> io::Result<Job> {
        Job::spawn(self, opts)
    }
}

pub struct Job {
    child: Option<Child>,
    mon: Monitor,
    start: Instant,
    opts: JobOptions,
    finished: Cell<bool>,
}

pub struct Waiter {
    child: Child,
    mon: Monitor,
    start: Instant,
    opts: JobOptions,
    finished: Cell<bool>,
}

impl Job {
    pub fn spawn(cmd: &mut Command, opts: &JobOptions) -> io::Result<Self> {
        cmd.process_group(0);
        let child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0) as i32;
        let pgid = sys::getpgid(pid).unwrap_or(pid);
        Ok(Self {
            child: Some(child),
            mon: Monitor {
                inner: crate::Monitor::new(pgid),
            },
            start: Instant::now(),
            opts: opts.clone(),
            finished: Cell::new(false),
        })
    }

    pub fn id(&self) -> Option<u32> {
        self.child.as_ref().and_then(|c| c.id())
    }

    pub fn pgid(&self) -> i32 {
        self.mon.pgid()
    }

    pub fn stdin(&mut self) -> Option<&mut ChildStdin> {
        self.child.as_mut().and_then(|c| c.stdin.as_mut())
    }

    pub fn stdout(&mut self) -> Option<&mut ChildStdout> {
        self.child.as_mut().and_then(|c| c.stdout.as_mut())
    }

    pub fn stderr(&mut self) -> Option<&mut ChildStderr> {
        self.child.as_mut().and_then(|c| c.stderr.as_mut())
    }

    pub fn monitor(&self) -> Monitor {
        self.mon.clone()
    }

    pub async fn sample(&self) -> io::Result<Sample> {
        self.mon.sample().await
    }

    pub fn split(mut self) -> (Waiter, Monitor) {
        self.finished.set(true);
        self.opts.kill_on_drop = false;
        let child = self.child.take().expect("job already split");
        let mon = self.mon.clone();
        (
            Waiter {
                child,
                mon: self.mon.clone(),
                start: self.start,
                opts: self.opts.clone(),
                finished: Cell::new(false),
            },
            mon,
        )
    }

    pub fn kill(&self) -> io::Result<()> {
        kill_group(self.mon.pgid())
    }

    pub async fn wait(&mut self) -> io::Result<Exit> {
        self.wait_polling(self.opts.poll).await
    }

    /// Sample, then `try_wait`, then sleep — same order as the std loop.
    pub async fn wait_polling(&mut self, every: Duration) -> io::Result<Exit> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("job already split"))?;
        wait_polling_inner(child, &self.mon, self.start, every, &self.finished).await
    }

    pub fn into_inner(mut self) -> Child {
        self.finished.set(true);
        self.opts.kill_on_drop = false;
        self.child.take().expect("job already split")
    }
}

impl Waiter {
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    pub fn stdin(&mut self) -> Option<&mut ChildStdin> {
        self.child.stdin.as_mut()
    }

    pub fn stdout(&mut self) -> Option<&mut ChildStdout> {
        self.child.stdout.as_mut()
    }

    pub fn stderr(&mut self) -> Option<&mut ChildStderr> {
        self.child.stderr.as_mut()
    }

    pub async fn sample(&self) -> io::Result<Sample> {
        self.mon.sample().await
    }

    pub fn kill(&self) -> io::Result<()> {
        kill_group(self.mon.pgid())
    }

    pub async fn wait(&mut self) -> io::Result<Exit> {
        self.wait_polling(self.opts.poll).await
    }

    pub async fn wait_polling(&mut self, every: Duration) -> io::Result<Exit> {
        wait_polling_inner(
            &mut self.child,
            &self.mon,
            self.start,
            every,
            &self.finished,
        )
        .await
    }
}

async fn wait_polling_inner(
    child: &mut Child,
    mon: &Monitor,
    start: Instant,
    every: Duration,
    finished: &Cell<bool>,
) -> io::Result<Exit> {
    loop {
        let last = mon.sample().await?;
        match child.try_wait()? {
            Some(status) => {
                finished.set(true);
                return Ok(Exit {
                    status,
                    sample: last,
                    wall: start.elapsed(),
                });
            }
            None => tokio::time::sleep(every).await,
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if self.opts.kill_on_drop && !self.finished.get() {
            let _ = kill_group(self.mon.pgid());
        }
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        if self.opts.kill_on_drop && !self.finished.get() {
            let _ = kill_group(self.mon.pgid());
        }
    }
}
