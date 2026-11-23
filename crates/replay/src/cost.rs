//! What this run has cost the machine so far, from the ledger macOS keeps
//! for each process. Energy is not here: the ledger bills none to a tool
//! like this, and measuring it needs `sudo powermetrics`.

use std::time::Duration;

pub struct Used {
    /// The most memory the run has held at once, as Activity Monitor's
    /// Memory column counts it. Model weights mapped from their file are
    /// not in it.
    pub peak_footprint_bytes: u64,
    /// User and system CPU time. Work done on the GPU is not in it.
    pub cpu: Duration,
}

pub fn so_far() -> Result<Used, String> {
    let mut ledger: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    let read = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V4,
            (&raw mut ledger).cast(),
        )
    };
    if read != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // The ledger's CPU times are in Mach ticks; getrusage's are plain time.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &raw mut usage) };
    let time = |t: libc::timeval| {
        Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64)
    };
    Ok(Used {
        peak_footprint_bytes: ledger.ri_lifetime_max_phys_footprint,
        cpu: time(usage.ru_utime) + time(usage.ru_stime),
    })
}
