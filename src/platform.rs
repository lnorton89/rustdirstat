// ============================================================================
// Module:       platform
// Description:  The few operations that genuinely need a platform syscall: on-
//               disk (physical) file size, and volume free/total space.
//
// Dependencies: windows-sys (GetDiskFreeSpaceExW, GetCompressedFileSizeW)
//               on Windows; std::os::unix (st_blocks, statvfs) elsewhere
// ============================================================================

//! The handful of things that genuinely need a platform-specific syscall:
//! on-disk (physical) file size, file identity, and volume free/total
//! space.
//!
//! Physical size comes for free on Unix out of the `stat()` the scanner
//! already makes (`st_blocks`). On Windows it needs a *separate* per-file
//! syscall (`GetCompressedFileSizeW`), so that call is paid only for the
//! files whose attributes say the answer can differ from the logical
//! size — NTFS-compressed and sparse files — and everything else reports
//! its logical size, which is what the syscall would have said anyway.
//! Note what "physical" means here: compression- and sparse-aware, not
//! rounded up to the allocation cluster.

/// The filesystem identity of a file — every hard link to the same
/// object shares one.
///
/// Used to keep hard links from being reported as duplicate *copies*
/// (deleting an alias frees nothing until the last link disappears) and,
/// eventually, from being double-counted in on-disk accounting.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FileId {
    pub device: u64,
    pub inode: u64,
}

/// The identity of the file `meta` describes, if the platform captures
/// one for free. Unix gets `(st_dev, st_ino)` out of the `stat()` the
/// scanner already makes; Windows would need a separate handle-based
/// syscall per file, so it is `None` there and hard-link detection is
/// simply unavailable — each pathname counts as its own file, as it
/// always has.
#[cfg(unix)]
pub fn file_id(meta: &std::fs::Metadata) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    Some(FileId {
        device: meta.dev(),
        inode: meta.ino(),
    })
}

#[cfg(not(unix))]
pub fn file_id(_meta: &std::fs::Metadata) -> Option<FileId> {
    None
}

#[cfg(unix)]
pub fn physical_size(meta: &std::fs::Metadata, _path: &std::path::Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    // st_blocks is always in 512-byte units regardless of the filesystem's
    // actual block size (POSIX convention, not configurable). Saturating
    // because a corrupt or hostile filesystem can report a block count
    // that overflows when scaled, and a wrong size is a better outcome
    // than a panic in a release build or a wrapped one in a debug build.
    meta.blocks().saturating_mul(512)
}

#[cfg(windows)]
pub fn physical_size(meta: &std::fs::Metadata, path: &std::path::Path) -> u64 {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_SPARSE_FILE,
    };
    // The directory listing already delivered the attributes for free,
    // and only a compressed or sparse file can occupy less than its
    // logical size — so the extra syscall is paid exactly for the files
    // where the answer can differ, not once per file on the whole scan.
    // For everything else `GetCompressedFileSizeW` would return the
    // logical size anyway. (Which is also this platform's definition of
    // "physical": sparse- and compression-aware, not rounded up to the
    // cluster — a 1-byte plain file reports 1 byte, not one 4 KB
    // cluster. True allocation size would need
    // `GetFileInformationByHandleEx(FileStandardInfo)` and a handle per
    // file.)
    if meta.file_attributes() & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) == 0 {
        return meta.len();
    }
    // Falls back to the logical size when the call fails (a file that
    // cannot be opened, a path past the legacy length limit) — the
    // failure path may not be exact, but the normal path no longer lies
    // about being physical.
    win_compressed_size(path).unwrap_or(meta.len())
}

/// Safe leaf wrapper over `SetLastError(NO_ERROR)`, so the check after
/// `GetCompressedFileSizeW` reads a value that call actually produced
/// rather than a stale error from something earlier on the thread.
#[cfg(windows)]
fn clear_last_error() {
    use windows_sys::Win32::Foundation::{SetLastError, NO_ERROR};
    // SAFETY: `SetLastError` writes the calling thread's own last-error
    // slot and reads nothing; there are no argument-validity conditions.
    unsafe { SetLastError(NO_ERROR) };
}

/// Safe leaf wrapper over `GetCompressedFileSizeW`.
///
/// Returns the actual on-disk byte count for `path` — the compressed
/// size of an NTFS compressed file, the allocated size of a sparse one,
/// the logical size otherwise — or `None` when the call fails (a file
/// that cannot be opened, a directory). Marshalling is entirely outside
/// the `unsafe` blocks, which hold one call each and nothing else.
#[cfg(windows)]
fn win_compressed_size(path: &std::path::Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetCompressedFileSizeW;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut high: u32 = 0;
    clear_last_error();
    // SAFETY: `wide` is a NUL-terminated wide string valid for the
    // duration of the call, and `high` is a live `u32` the callee writes
    // only when the call succeeds.
    let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
    // `INVALID_FILE_SIZE` (0xFFFFFFFF) is ambiguous: it marks failure,
    // but it is also a legitimate low doubleword for a file whose size
    // happens to end in those 32 bits. The documented disambiguation is
    // the thread's last error — still `NO_ERROR` means the value is
    // real. The read must happen before any other syscall can overwrite
    // it, which is why nothing sits between the call and this check.
    if low == u32::MAX && std::io::Error::last_os_error().raw_os_error() != Some(0) {
        return None;
    }
    Some(((high as u64) << 32) | low as u64)
}

#[cfg(not(any(unix, windows)))]
pub fn physical_size(meta: &std::fs::Metadata, _path: &std::path::Path) -> u64 {
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

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    /// The FFI plumbing: a freshly written, uncompressed file reports
    /// its own logical size (the documented behavior of
    /// `GetCompressedFileSizeW` for files that are neither compressed
    /// nor sparse), and the wrapper comes back with a real number
    /// rather than `0` or garbage from a mistyped signature.
    #[test]
    fn a_normal_file_reports_its_logical_size_as_physical() -> std::io::Result<()> {
        let dir = crate::util::scratch_dir("physsize", "normal");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let file = dir.join("plain.bin");
        std::fs::write(&file, vec![0u8; 4096])?;
        let meta = std::fs::metadata(&file)?;
        assert_eq!(
            physical_size(&meta, &file),
            meta.len(),
            "a non-compressed, non-sparse file occupies its logical size"
        );
        assert!(win_compressed_size(&file).is_some());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    /// A sparse file reports less than its logical size — the assertion
    /// that actually distinguishes the real implementation from the old
    /// `meta.len()` one, which would report the full 64 MB either way.
    ///
    /// Best-effort: making a file sparse needs `fsutil` and an NTFS
    /// volume, which a CI temp dir is not guaranteed to be. If the
    /// sparse marking fails the test passes without asserting, so it
    /// cannot be flaky — only blind on non-NTFS.
    #[test]
    fn a_sparse_file_reports_less_than_its_logical_size() -> std::io::Result<()> {
        use std::process::Command;
        let dir = crate::util::scratch_dir("physsize", "sparse");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let file = dir.join("sparse.bin");
        let f = std::fs::File::create(&file)?;
        f.set_len(64 * 1024 * 1024)?;
        drop(f);
        let marked = Command::new("fsutil")
            .args(["sparse", "setflag"])
            .arg(&file)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if marked {
            let ranged = Command::new("fsutil")
                .args(["sparse", "setrange"])
                .arg(&file)
                .args(["0", "67108864"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ranged {
                let meta = std::fs::metadata(&file)?;
                let physical = physical_size(&meta, &file);
                assert!(
                    physical < meta.len(),
                    "a fully sparse file must occupy far less than its \
                     logical size: logical {}, physical {physical}",
                    meta.len()
                );
            }
        }
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
