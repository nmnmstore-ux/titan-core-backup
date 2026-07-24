#![allow(dead_code)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static COMPROMISED: AtomicBool = AtomicBool::new(false);

pub fn is_compromised() -> bool {
    COMPROMISED.load(Ordering::Relaxed)
}

pub fn mark_compromised() {
    COMPROMISED.store(true, Ordering::SeqCst);
}

pub fn detect_debugger() -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("TracerPid:") {
                    let pid: i32 = line.trim_start_matches("TracerPid:").trim().parse().unwrap_or(0);
                    if pid != 0 {
                        return true;
                    }
                }
            }
        }

        unsafe {
            let ret: i64 = libc::ptrace(libc::PTRACE_TRACEME, 0, std::ptr::null_mut::<libc::c_void>(), std::ptr::null_mut::<libc::c_void>());
            if ret == -1 {
                return true;
            }
        }
    }
    false
}

pub fn detect_timing_anomaly() -> bool {
    let start = Instant::now();
    let mut x: u64 = 0;
    for i in 0..1_000_000 {
        x ^= i;
    }
    let elapsed = start.elapsed();
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    let _ = x;
    elapsed.as_micros() > 5000
}

pub fn verify_binary_integrity() -> Result<[u8; 64], String> {
    let binary_path = std::env::current_exe()
        .map_err(|e| format!("cannot get exe path: {}", e))?;
    let binary = std::fs::read(&binary_path)
        .map_err(|e| format!("cannot read binary: {}", e))?;
    use blake2::{Blake2b512, Digest};
    let hash: [u8; 64] = Blake2b512::digest(&binary).into();
    Ok(hash)
}

pub fn run_integrity_checks() -> Result<(), String> {
    if detect_debugger() {
        mark_compromised();
        return Err("debugger detected".into());
    }
    let _ = verify_binary_integrity()?;
    Ok(())
}
