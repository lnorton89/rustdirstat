//! The handful of things that genuinely need a platform-specific syscall:
//! on-disk (physical) file size, and volume free/total space.
//!
//! Physical size is only computed for real on Unix, where it comes for
//! free out of the `stat()` call the scanner already makes (`st_blocks`).
//! On Windows, the true on-disk size (accounting for NTFS compression or
//! sparse files) requires a *separate* per-file syscall
//! (`GetCompressedFileSizeW`) — paying that for every file would meaningfully
//! slow down scanning on exactly the large trees this tool is tuned for, so
//! it deliberately isn't paid by default. Windows physical size falls back
//! to the logical size instead of guessing.

#[cfg(unix)]
pub fn physical_size(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    // st_blocks is always in 512-byte units regardless of the filesystem's
    // actual block size (POSIX convention, not configurable).
    meta.blocks() * 512
}

#[cfg(not(unix))]
pub fn physical_size(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

/// Free and total bytes on the volume containing `path`, if determinable.
pub fn volume_space(path: &std::path::Path) -> (Option<u64>, Option<u64>) {
    imp::volume_space(path)
}

#[cfg(unix)]
mod imp {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    // `statvfs`'s block-count fields aren't a fixed width across Unix
    // targets (narrower on some 32-bit/musl/macOS configurations), so the
    // `as u64` widening below is genuinely needed for portability even
    // though it's a same-type no-op on this particular platform (x86_64
    // glibc, where clippy is analyzing it from).
    #[allow(clippy::unnecessary_cast)]
    pub fn volume_space(path: &Path) -> (Option<u64>, Option<u64>) {
        let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
            return (None, None);
        };
        // SAFETY: `c_path` is a valid, NUL-terminated C string for the
        // duration of this call; `stat` is a plain out-parameter struct
        // fully initialized by a successful `statvfs` call before we read
        // any field from it.
        unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
                return (None, None);
            }
            let block_size = stat.f_frsize as u64;
            let free = stat.f_bavail as u64 * block_size;
            let total = stat.f_blocks as u64 * block_size;
            (Some(free), Some(total))
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    pub(super) fn volume_space(path: &Path) -> (Option<u64>, Option<u64>) {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);

        let mut free_available: u64 = 0;
        let mut total_bytes: u64 = 0;
        // SAFETY: `wide` is a NUL-terminated UTF-16 string valid for the
        // call; the two `u64` out-parameters are always written by
        // `GetDiskFreeSpaceExW` on success, and left unread on failure.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_available,
                &mut total_bytes,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            (None, None)
        } else {
            (Some(free_available), Some(total_bytes))
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    pub fn volume_space(_path: &std::path::Path) -> (Option<u64>, Option<u64>) {
        (None, None)
    }
}
