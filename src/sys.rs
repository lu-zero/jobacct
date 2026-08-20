use std::time::Duration;

#[cfg(target_os = "linux")]
#[path = "sys_linux.rs"]
mod imp;

#[cfg(target_os = "macos")]
#[path = "sys_macos.rs"]
mod imp;

#[cfg(target_os = "freebsd")]
#[path = "sys_freebsd.rs"]
mod imp;

#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub pid: u32,
    pub cpu: Duration,
    pub rss: u64,
}

pub(crate) fn members(pgid: i32) -> std::io::Result<Vec<Member>> {
    imp::members(pgid)
}

pub(crate) fn kill_group(pgid: i32) -> std::io::Result<()> {
    let rc = unsafe { libc::killpg(pgid, libc::SIGKILL) };
    if rc == 0 {
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(err)
        }
    }
}

pub(crate) fn getpgid(pid: i32) -> std::io::Result<i32> {
    let rc = unsafe { libc::getpgid(pid) };
    if rc < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(rc)
    }
}

#[cfg(target_os = "freebsd")]
pub(crate) fn timeval_to_duration(tv: libc::timeval) -> Duration {
    let sec = Duration::from_secs(tv.tv_sec.max(0) as u64);
    let usec = Duration::from_micros(tv.tv_usec.max(0) as u64);
    sec.saturating_add(usec)
}
