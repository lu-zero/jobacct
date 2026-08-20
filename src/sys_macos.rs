use std::io;
use std::time::Duration;

use std::sync::OnceLock;

use libproc::processes::{pids_by_type, ProcFilter};
use libproc::task_info::TaskAllInfo;
use mach2::mach_time::{mach_timebase_info, mach_timebase_info_data_t};

use super::Member;

/// `proc_taskinfo.pti_total_{user,system}` are Mach absolute-time ticks, not
/// nanoseconds. Convert via the per-machine timebase (`numer/denom`).
fn ticks_to_ns(ticks: u64) -> u64 {
    static TIMEBASE: OnceLock<(u32, u32)> = OnceLock::new();
    let &(numer, denom) = TIMEBASE.get_or_init(|| {
        let mut info = mach_timebase_info_data_t { numer: 0, denom: 0 };
        let rc = unsafe { mach_timebase_info(&mut info) };
        if rc != 0 || info.numer == 0 || info.denom == 0 {
            (1, 1)
        } else {
            (info.numer, info.denom)
        }
    });
    ticks.saturating_mul(numer as u64) / denom.max(1) as u64
}

pub(crate) fn members(pgid: i32) -> io::Result<Vec<Member>> {
    let pgrpid = u32::try_from(pgid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "pgid out of range"))?;
    let pids = pids_by_type(ProcFilter::ByProgramGroup { pgrpid })
        .map_err(|e| io::Error::other(format!("proc_listpids: {e}")))?;

    let mut out = Vec::with_capacity(pids.len());
    for pid in pids {
        let info = match libproc::proc_pid::pidinfo::<TaskAllInfo>(pid as i32, 0) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let pt = info.ptinfo;
        let cpu_ns =
            ticks_to_ns(pt.pti_total_user).saturating_add(ticks_to_ns(pt.pti_total_system));
        out.push(Member {
            pid,
            cpu: Duration::from_nanos(cpu_ns),
            rss: pt.pti_resident_size,
        });
    }
    Ok(out)
}
