use std::io;
use std::mem::{size_of, MaybeUninit};
use std::ptr;

use super::{timeval_to_duration, Member};

pub(crate) fn members(pgid: i32) -> io::Result<Vec<Member>> {
    unsafe {
        let mut mib = [
            libc::CTL_KERN,
            libc::KERN_PROC,
            libc::KERN_PROC_PGRP,
            pgid,
        ];
        let mut len = 0usize;
        let rc = libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            ptr::null_mut(),
            &mut len,
            ptr::null_mut(),
            0,
        );
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        if len == 0 {
            return Ok(Vec::new());
        }

        let mut buf = vec![0u8; len];
        let rc = libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut len,
            ptr::null_mut(),
            0,
        );
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }

        let sz = size_of::<libc::kinfo_proc>();
        if sz == 0 {
            return Ok(Vec::new());
        }
        let n = len / sz;
        let page = libc::sysconf(libc::_SC_PAGESIZE);
        let page = if page > 0 { page as u64 } else { 4096 };

        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let kp = ptr::read_unaligned(
                buf.as_ptr().add(i * sz) as *const libc::kinfo_proc,
            );
            let _ = MaybeUninit::new(kp); // keep read
            let cpu = timeval_to_duration(kp.ki_rusage.ru_utime)
                .saturating_add(timeval_to_duration(kp.ki_rusage.ru_stime));
            let rss = (kp.ki_rssize.max(0) as u64).saturating_mul(page);
            out.push(Member {
                pid: kp.ki_pid as u32,
                cpu,
                rss,
            });
        }
        Ok(out)
    }
}
