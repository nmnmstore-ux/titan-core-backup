#![allow(dead_code)]
use std::sync::atomic::{AtomicBool, Ordering};

static MEMORY_LOCKED: AtomicBool = AtomicBool::new(false);

pub fn lock_memory() -> Result<(), String> {
    if MEMORY_LOCKED.load(Ordering::SeqCst) {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    unsafe {
        let ret = libc::mlockall(libc::MCL_CURRENT as libc::c_int | libc::MCL_FUTURE as libc::c_int);
        if ret != 0 {
            return Err(format!("mlockall failed: errno {}", *libc::__errno_location()));
        }
    }
    MEMORY_LOCKED.store(true, Ordering::SeqCst);
    Ok(())
}

pub fn unlock_memory() {
    if MEMORY_LOCKED.load(Ordering::SeqCst) {
        #[cfg(target_os = "linux")]
        unsafe {
            libc::munlockall();
        }
        MEMORY_LOCKED.store(false, Ordering::SeqCst);
    }
}

pub fn secure_scrub(data: &mut [u8]) {
    for byte in data.iter_mut() {
        *byte = 0;
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

pub struct SecureBuffer {
    data: Vec<u8>,
    locked: bool,
}

impl SecureBuffer {
    pub fn new(size: usize) -> Self {
        let data = vec![0u8; size];
        #[cfg(target_os = "linux")]
        unsafe {
            libc::mlock(data.as_ptr() as *const libc::c_void, data.len() as libc::size_t);
        }
        Self { data, locked: true }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

impl Drop for SecureBuffer {
    fn drop(&mut self) {
        secure_scrub(&mut self.data);
        #[cfg(target_os = "linux")]
        unsafe {
            libc::munlock(self.data.as_ptr() as *const libc::c_void, self.data.len() as libc::size_t);
        }
    }
}
