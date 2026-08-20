use std::cell::Cell;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use std::thread;
use std::time::{Duration, Instant};

use crate::sys::{self, kill_group};
use crate::{Event, Exit, JobOptions, Monitor, Sample};

/// Extension trait: [`Command::spawn_job`](CommandJobExt::spawn_job).
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

/// Leader child + group monitor. Does not `Deref` to [`Child`].
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
        // Child's pid becomes its pgid, without stealing a user `pre_exec`.
        cmd.process_group(0);
        let child = cmd.spawn()?;
        let pid = child.id() as i32;
        let pgid = sys::getpgid(pid).unwrap_or(pid);
        Ok(Self {
            child: Some(child),
            mon: Monitor::new(pgid),
            start: Instant::now(),
            opts: opts.clone(),
            finished: Cell::new(false),
        })
    }

    pub fn id(&self) -> u32 {
        self.child.as_ref().map(|c| c.id()).unwrap_or(0)
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

    pub fn sample(&self) -> io::Result<Sample> {
        self.mon.sample()
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

    pub fn try_wait(&mut self) -> io::Result<Option<Exit>> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("job already split"))?;
        try_wait_inner(child, &self.mon, self.start, &self.finished)
    }

    /// Poll-sample then wait. Equivalent to [`Self::wait_polling`] at
    /// [`JobOptions::poll`].
    pub fn wait(&mut self) -> io::Result<Exit> {
        self.wait_polling(self.opts.poll)
    }

    pub fn wait_polling(&mut self, every: Duration) -> io::Result<Exit> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("job already split"))?;
        wait_polling_inner(child, &self.mon, self.start, every, &self.finished)
    }

    pub fn watch(self, every: Duration) -> Watch {
        let (waiter, _) = self.split();
        Watch {
            waiter,
            every,
            done: false,
        }
    }

    /// Give up accounting. The process group is not killed.
    pub fn into_inner(mut self) -> Child {
        self.finished.set(true);
        self.opts.kill_on_drop = false;
        self.child.take().expect("job already split")
    }
}

impl Waiter {
    pub fn id(&self) -> u32 {
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

    pub fn sample(&self) -> io::Result<Sample> {
        self.mon.sample()
    }

    pub fn kill(&self) -> io::Result<()> {
        kill_group(self.mon.pgid())
    }

    pub fn try_wait(&mut self) -> io::Result<Option<Exit>> {
        try_wait_inner(&mut self.child, &self.mon, self.start, &self.finished)
    }

    fn try_wait_with(&mut self, last: Sample) -> io::Result<Option<Exit>> {
        match self.child.try_wait()? {
            None => Ok(None),
            Some(status) => {
                self.finished.set(true);
                Ok(Some(Exit {
                    status,
                    sample: last,
                    wall: self.start.elapsed(),
                }))
            }
        }
    }

    pub fn wait(&mut self) -> io::Result<Exit> {
        self.wait_polling(self.opts.poll)
    }

    pub fn wait_polling(&mut self, every: Duration) -> io::Result<Exit> {
        wait_polling_inner(
            &mut self.child,
            &self.mon,
            self.start,
            every,
            &self.finished,
        )
    }
}

pub struct Watch {
    waiter: Waiter,
    every: Duration,
    done: bool,
}

impl Iterator for Watch {
    type Item = io::Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        // Sample first so a zombie leader is still visible, then reap it.
        match self.waiter.sample() {
            Ok(s) => match self.waiter.try_wait_with(s) {
                Ok(Some(exit)) => {
                    self.done = true;
                    Some(Ok(Event::Exited(exit)))
                }
                Ok(None) => {
                    thread::sleep(self.every);
                    Some(Ok(Event::Tick(s)))
                }
                Err(e) => {
                    self.done = true;
                    Some(Err(e))
                }
            },
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

fn try_wait_inner(
    child: &mut Child,
    mon: &Monitor,
    start: Instant,
    finished: &Cell<bool>,
) -> io::Result<Option<Exit>> {
    let last = mon.sample()?;
    match child.try_wait()? {
        None => Ok(None),
        Some(status) => {
            finished.set(true);
            Ok(Some(Exit {
                status,
                sample: last,
                wall: start.elapsed(),
            }))
        }
    }
}

fn wait_polling_inner(
    child: &mut Child,
    mon: &Monitor,
    start: Instant,
    every: Duration,
    finished: &Cell<bool>,
) -> io::Result<Exit> {
    loop {
        if let Some(exit) = try_wait_inner(child, mon, start, finished)? {
            return Ok(exit);
        }
        thread::sleep(every);
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
