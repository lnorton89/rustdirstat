// ============================================================================
// Module:       platform
// Description:  The few operations that genuinely need a platform syscall: on-
//               disk (physical) file size, and volume free/total space.
//
// Dependencies: windows-sys (GetDiskFreeSpaceExW) on Windows; std::os::unix
//               (st_blocks, statvfs) elsewhere
// ============================================================================

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
    // actual block size (POSIX convention, not configurable). Saturating
    // because a corrupt or hostile filesystem can report a block count
    // that overflows when scaled, and a wrong size is a better outcome
    // than a panic in a release build or a wrapped one in a debug build.
    meta.blocks().saturating_mul(512)
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
    use std::ffi::{CStr, CString};
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    /// Safe leaf wrapper over `statvfs(2)`, and the only `unsafe` in this
    /// module. Everything a caller needs to reason about is here: it takes
    /// a borrowed C string, and either returns a fully initialized struct
    /// or nothing. No caller can misuse it, so no caller has to be
    /// `unsafe` itself.
    fn statvfs_for(path: &CStr) -> Option<libc::statvfs> {
        let mut stat = MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: `path` is a valid NUL-terminated C string that outlives
        // the call, and `stat` points to writable memory of exactly the
        // size and alignment `statvfs` expects for its out-parameter.
        let rc = unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) };
        if rc != 0 {
            return None;
        }
        // SAFETY: `statvfs` returned 0, which is documented to mean it
        // filled in the whole struct. This is the only path that reads it.
        Some(unsafe { stat.assume_init() })
    }

    /// Widens one of `statvfs`'s integer fields to `u64`.
    ///
    /// These fields have no fixed width across Unix targets: `fsblkcnt_t`
    /// is 32-bit on macOS and on 32-bit Linux but 64-bit on 64-bit Linux,
    /// and `f_frsize` differs again. That makes every *literal* spelling
    /// of the conversion wrong somewhere -- `as u64` trips
    /// `unnecessary_cast` where the field is already 64-bit, and
    /// `u64::from` trips `useless_conversion` in the same place. Going
    /// through a generic bound states the actual requirement ("whatever
    /// this is, it fits in a u64") once, and is a no-op at runtime.
    fn widen<T: Into<u64>>(value: T) -> u64 {
        value.into()
    }

    pub(super) fn volume_space(path: &Path) -> (Option<u64>, Option<u64>) {
        // A path containing an interior NUL cannot name a real file, so
        // there is nothing to report.
        let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
            return (None, None);
        };
        let Some(stat) = statvfs_for(&c_path) else {
            return (None, None);
        };
        let block_size = widen(stat.f_frsize);
        let free = widen(stat.f_bavail).saturating_mul(block_size);
        let total = widen(stat.f_blocks).saturating_mul(block_size);
        (Some(free), Some(total))
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    /// Safe leaf wrapper over `GetDiskFreeSpaceExW`, and the only `unsafe`
    /// in this module. Returns `(free, total)` or nothing.
    fn disk_free_space(wide_path: &[u16]) -> Option<(u64, u64)> {
        let mut free_available: u64 = 0;
        let mut total_bytes: u64 = 0;
        // SAFETY: `wide_path` is NUL-terminated (the caller pushes the
        // terminator) and valid for the duration of the call; both
        // out-parameters are live `u64`s for the same duration, and the
        // fourth is documented as optional and passed as null.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide_path.as_ptr(),
                &mut free_available,
                &mut total_bytes,
                std::ptr::null_mut(),
            )
        };
        // The out-parameters are only meaningful on success.
        (ok != 0).then_some((free_available, total_bytes))
    }

    pub(super) fn volume_space(path: &Path) -> (Option<u64>, Option<u64>) {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);
        match disk_free_space(&wide) {
            Some((free, total)) => (Some(free), Some(total)),
            None => (None, None),
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    pub(super) fn volume_space(_path: &std::path::Path) -> (Option<u64>, Option<u64>) {
        (None, None)
    }
}
