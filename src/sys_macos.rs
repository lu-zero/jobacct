use std::io;
use std::time::Duration;

use libproc::processes::{pids_by_type, ProcFilter};
use libproc::task_info::TaskAllInfo;

use super::Member;

pub(crate) fn members(pgid: i32) -> io::Result<Vec<Member>> {
    let pgrpid = u32::try_from(pgid).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "pgid out of range")
    })?;
    let pids = pids_by_type(ProcFilter::ByProgramGroup { pgrpid }).map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("proc_listpids: {e}"))
    })?;

    let mut out = Vec::with_capacity(pids.len());
    for pid in pids {
        let info = match libproc::proc_pid::pidinfo::<TaskAllInfo>(pid as i32, 0) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let pt = info.ptinfo;
        // pti_total_{user,system} are nanoseconds of CPU time.
        let cpu_ns = pt.pti_total_user.saturating_add(pt.pti_total_system);
        out.push(Member {
            pid,
            cpu: Duration::from_nanos(cpu_ns),
            rss: pt.pti_resident_size,
        });
    }
    Ok(out)
}
