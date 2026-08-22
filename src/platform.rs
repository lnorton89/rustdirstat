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
/// syscall per file, so the *scan* leaves it `None` there — and
/// duplicate detection recovers the identity later through
/// [`file_id_from_handle`], from the handle the hasher already holds
/// open, so hard-link awareness works on Windows exactly where it
/// matters.
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

/// The identity of an already-open file, read from its handle.
///
/// This is how Windows gets hard-link identity without the scanner
/// paying a per-file open: duplicate hashing *already holds* the file
/// open to read it, and the volume serial + file index pair is one
/// metadata query on that same handle. Safe leaf wrapper over
/// `GetFileInformationByHandle`; the `unsafe` blocks hold one call
/// each and nothing else.
#[cfg(windows)]
pub fn file_id_from_handle(file: &std::fs::File) -> Option<FileId> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut info = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: the handle comes from a live `&File`, so it is a valid,
    // open file handle for the duration of the call, and `info` is a
    // live out-parameter the callee writes only on success.
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) };
    if ok == 0 {
        return None;
    }
    // SAFETY: the callee reported success, so the struct is initialized.
    let info = unsafe { info.assume_init() };
    Some(FileId {
        device: info.dwVolumeSerialNumber as u64,
        inode: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
    })
}

/// Unix never needs the handle form: the scan-time `stat()` already
/// captured every file's identity for free.
#[cfg(not(windows))]
pub fn file_id_from_handle(_file: &std::fs::File) -> Option<FileId> {
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
/// A place a scan can be pointed at, as offered by the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    /// What to scan: the volume's root path.
    pub path: std::path::PathBuf,
    /// What to call it. The drive letter on Windows, the mount point
    /// elsewhere — deliberately not a friendly name fetched from the
    /// filesystem, which costs a syscall per volume for a string the
    /// path already carries.
    pub label: String,
    pub free: Option<u64>,
    pub total: Option<u64>,
}

/// The volumes worth offering to scan.
///
/// Deliberately conservative on every platform: a picker that lists
/// pseudo-filesystems is worse than one that misses an exotic mount,
/// because the user can always pick a folder by hand, whereas a list
/// full of `/proc`, `/sys` and snap loopbacks makes the real entries
/// hard to find.
pub fn volumes() -> Vec<Volume> {
    imp::volumes()
}

/// One directory entry, as the filesystem itself describes it.
///
/// This is what a Windows directory listing hands over in the same
/// enumeration `read_dir` performs, and it is strictly more than `std`
/// exposes: the size actually occupied on disk, and the file's identity.
/// Collecting it costs one directory handle — the same handle
/// `read_dir` opens internally — rather than a handle per file, which is
/// why the scanner can afford numbers that used to be out of reach.
#[derive(Clone, Debug)]
pub struct DirEntryInfo {
    pub name: std::ffi::OsString,
    pub is_dir: bool,
    /// True for a *name-surrogate* reparse point — a symlink or a
    /// mount point — and false for the many reparse points that are not
    /// links at all (cloud placeholders, dedup stubs). Resolved against
    /// the filesystem for the rare entry that carries the attribute, so
    /// this means exactly what `std`'s `is_symlink` means.
    pub is_symlink: bool,
    pub len: u64,
    /// Size on disk as the filesystem reports it. Cluster-rounded for a
    /// file large enough to need clusters; NTFS reports a small resident
    /// file, which lives inside its MFT record and occupies no clusters
    /// at all, as its data rounded to eight bytes.
    pub allocation: u64,
    pub modified: Option<std::time::SystemTime>,
    pub file_id: Option<FileId>,
}

/// Every entry in `dir`, or `None` if this platform or this filesystem
/// cannot list one this way.
///
/// `None` is a normal answer, not an error: some network redirectors and
/// non-NTFS volumes do not implement the info class this uses, and every
/// caller falls back to `std::fs::read_dir`. A scan with less precise
/// numbers is enormously better than no scan.
#[cfg(windows)]
pub fn directory_listing(dir: &std::path::Path) -> Option<Vec<DirEntryInfo>> {
    // A documented way back to the `std` walk, for a filesystem where
    // this path misbehaves and for measuring what it costs. Read per
    // directory, which is a cheap environment lookup against the
    // syscalls either path is about to make.
    if std::env::var_os("RUSTDIRSTAT_STD_LISTING").is_some() {
        return None;
    }
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };
    let handle = win::open_directory(dir)?;
    // A volume serial is a property of the volume, so it is asked for
    // once per volume rather than once per directory. On a
    // million-directory scan that is a million syscalls not made.
    let volume = win::cached_volume_serial(dir, &handle)?;
    let mut entries = Vec::new();
    win::for_each_entry(&handle, |wide, entry| {
        // `.` and `..` are the directory itself and its parent; `read_dir`
        // never yields them and neither may this. Compared as UTF-16 so
        // the check costs nothing before the name is even built.
        if wide.is_empty() || wide == [b'.' as u16] || wide == [b'.' as u16, b'.' as u16] {
            return;
        }
        // Straight from UTF-16 into the OS string, with one allocation
        // and no re-encoding. Going via `String::from_utf16_lossy` first
        // allocates twice and converts twice, which on a tree of any size
        // is most of what this function costs — it measured as a 40%
        // slowdown against `read_dir` on a 13,000-file scan, where doing
        // it this way is a wash.
        let name = std::os::windows::ffi::OsStringExt::from_wide(wide);
        let name: std::ffi::OsString = name;
        let is_dir = entry.attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        // A reparse point is not necessarily a link. `std` calls an
        // entry a symlink only for the name-surrogate tags, and a cloud
        // placeholder or a dedup stub is an ordinary file that happens to
        // carry the attribute — treating those as links would report a
        // OneDrive folder as empty. The listing does not carry the tag,
        // so the rare entry that has the attribute is resolved against
        // the filesystem, which is what `std` would have done for *every*
        // entry.
        let is_symlink = if entry.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            std::fs::symlink_metadata(dir.join(&name))
                .map(|meta| meta.file_type().is_symlink())
                .unwrap_or(true)
        } else {
            false
        };
        entries.push(DirEntryInfo {
            name,
            is_dir,
            is_symlink,
            len: entry.len,
            allocation: entry.allocation,
            modified: win::system_time(entry.modified),
            file_id: Some(FileId {
                device: volume,
                inode: entry.file_id,
            }),
        });
    })?;
    Some(entries)
}

#[cfg(not(windows))]
pub fn directory_listing(_dir: &std::path::Path) -> Option<Vec<DirEntryInfo>> {
    // Unix already gets everything this carries out of the `stat` the
    // walk performs: `st_blocks` is the allocated size and
    // `(st_dev, st_ino)` is the identity.
    None
}

/// The Win32 half of [`directory_listing`], kept to the crate rule that an
/// `unsafe` block holds one FFI call and nothing else.
#[cfg(windows)]
mod win {
    use std::fs::File;
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    /// One entry, reduced to the two things worth carrying out of the
    /// FFI layer.
    pub(super) struct Entry {
        pub len: u64,
        pub allocation: u64,
        pub file_id: u64,
        pub attributes: u32,
        /// `LastWriteTime`, in the Win32 epoch. Converted by
        /// [`system_time`] rather than here, so this stays a plain
        /// transcription of the record.
        pub modified: i64,
    }

    /// A Win32 file time as a [`SystemTime`].
    ///
    /// Win32 counts 100-nanosecond ticks from 1601-01-01; the Unix epoch
    /// is a fixed number of those later. Everything is checked: a
    /// filesystem reporting a nonsense timestamp should leave a node with
    /// no modification time, not overflow one.
    pub(super) fn system_time(ticks: i64) -> Option<std::time::SystemTime> {
        const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
        let since_unix = ticks.checked_sub(UNIX_EPOCH_TICKS)?;
        let magnitude =
            std::time::Duration::from_nanos(since_unix.unsigned_abs().checked_mul(100)?);
        if since_unix >= 0 {
            std::time::SystemTime::UNIX_EPOCH.checked_add(magnitude)
        } else {
            std::time::SystemTime::UNIX_EPOCH.checked_sub(magnitude)
        }
    }

    /// A directory as an open handle.
    ///
    /// `File` rather than a hand-written owning wrapper: it already
    /// closes on drop, which is the whole reason the crate insists on
    /// owning wrappers, and `FILE_FLAG_BACKUP_SEMANTICS` is the
    /// documented way to make `CreateFileW` open a directory rather than
    /// fail. So there is no `unsafe` here at all.
    pub(super) fn open_directory(dir: &Path) -> Option<File> {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, SYNCHRONIZE,
        };
        // Exactly the access the enumeration needs, which is also what
        // `FindFirstFileW` asks for. `read(true)` would request
        // `GENERIC_READ`, a wider right that a directory ACL is more
        // likely to refuse — and refusing here would silently drop the
        // whole directory to the fallback path.
        std::fs::OpenOptions::new()
            .access_mode(FILE_LIST_DIRECTORY | SYNCHRONIZE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(dir)
            .ok()
    }

    /// The volume serial number behind an open handle.
    ///
    /// Half of a file's identity on Windows: the index alone is only
    /// unique within one volume.
    pub(super) fn volume_serial(handle: &File) -> Option<u64> {
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // SAFETY: the handle comes from a live `&File`, so it is open for
        // the duration of the call, and `info` is a live out-parameter
        // the callee writes only on success.
        let ok = unsafe { GetFileInformationByHandle(handle.as_raw_handle(), info.as_mut_ptr()) };
        if ok == 0 {
            return None;
        }
        // SAFETY: the callee reported success, so the struct is
        // initialized.
        let info = unsafe { info.assume_init() };
        Some(info.dwVolumeSerialNumber as u64)
    }

    /// The volume serial for the volume `dir` sits on.
    ///
    /// Keyed by the path's prefix (`C:`, a UNC share), which is what a
    /// volume boundary looks like from a path. A scan stays on one
    /// volume by default, so this is a single entry read a million
    /// times: an `RwLock` read is a few nanoseconds against the ~5µs the
    /// syscall costs, and the write happens once.
    ///
    /// A path with no prefix — nothing this scanner produces, but the
    /// type allows it — simply asks every time.
    pub(super) fn cached_volume_serial(dir: &Path, handle: &File) -> Option<u64> {
        use std::sync::RwLock;
        static CACHE: RwLock<Vec<(std::ffi::OsString, u64)>> = RwLock::new(Vec::new());

        let Some(std::path::Component::Prefix(prefix)) = dir.components().next() else {
            return volume_serial(handle);
        };
        let key = prefix.as_os_str();
        if let Ok(cache) = CACHE.read() {
            if let Some((_, serial)) = cache.iter().find(|(seen, _)| seen == key) {
                return Some(*serial);
            }
        }
        let serial = volume_serial(handle)?;
        if let Ok(mut cache) = CACHE.write() {
            if !cache.iter().any(|(seen, _)| seen == key) {
                cache.push((key.to_os_string(), serial));
            }
        }
        Some(serial)
    }

    /// Bytes per enumeration call.
    ///
    /// Each record is around 100 bytes plus the name, so this holds a few
    /// hundred entries per syscall — enough that a directory of any
    /// ordinary size is one or two calls, and small enough that a scan
    /// with a worker per core is not holding megabytes of buffer per
    /// thread.
    const BUFFER_BYTES: usize = 64 * 1024;

    /// Calls `visit` for every entry in the directory behind `handle`.
    ///
    /// Returns `None` if the filesystem does not support the info class
    /// (some redirectors, some non-NTFS volumes) or the enumeration
    /// fails part way — a partial answer is worse than none here,
    /// because the caller would silently attribute default sizes to the
    /// entries that were missed.
    pub(super) fn for_each_entry(
        handle: &File,
        mut visit: impl FnMut(&[u16], Entry),
    ) -> Option<()> {
        use windows_sys::Win32::Foundation::ERROR_NO_MORE_FILES;
        use windows_sys::Win32::Storage::FileSystem::{
            FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, FILE_ID_BOTH_DIR_INFO,
        };

        // Backed by `u64` so the buffer is 8-byte aligned: the records
        // the callee writes are `#[repr(C)]` structs with 8-byte fields,
        // and each `NextEntryOffset` is a multiple of 8 from the start.
        let mut buffer = vec![0u64; BUFFER_BYTES / 8];
        let mut class = FileIdBothDirectoryRestartInfo;
        loop {
            let ok = get_file_information(handle, class, &mut buffer);
            class = FileIdBothDirectoryInfo;
            if !ok {
                // The documented end of the listing, and the only error
                // that is not a failure.
                return match std::io::Error::last_os_error().raw_os_error() {
                    Some(code) if code == ERROR_NO_MORE_FILES as i32 => Some(()),
                    _ => None,
                };
            }

            let base = buffer.as_ptr().cast::<u8>();
            let mut offset = 0usize;
            loop {
                // SAFETY: `offset` walks the callee-written chain from
                // the start of a buffer the callee filled, and every
                // record it wrote lies wholly within that buffer.
                // `read_unaligned` rather than a dereference because the
                // guarantee about record alignment is documented but not
                // enforceable here.
                let record: FILE_ID_BOTH_DIR_INFO = unsafe {
                    base.add(offset)
                        .cast::<FILE_ID_BOTH_DIR_INFO>()
                        .read_unaligned()
                };
                let name_bytes = record.FileNameLength as usize;
                let name_at = offset + std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
                if name_at + name_bytes > BUFFER_BYTES {
                    // A record claiming to run past the buffer is not
                    // something to reason about; abandon the listing.
                    return None;
                }
                // SAFETY: the bounds check above proves the name lies
                // inside the buffer, and `FileNameLength` is a byte
                // count of UTF-16 code units.
                let name = unsafe {
                    std::slice::from_raw_parts(base.add(name_at).cast::<u16>(), name_bytes / 2)
                };
                visit(
                    name,
                    Entry {
                        // The sizes are signed in the API and
                        // non-negative in practice; a negative value is a
                        // filesystem lying, and zero is the safe reading
                        // of it.
                        len: record.EndOfFile.max(0) as u64,
                        allocation: record.AllocationSize.max(0) as u64,
                        file_id: record.FileId as u64,
                        attributes: record.FileAttributes,
                        modified: record.LastWriteTime,
                    },
                );
                if record.NextEntryOffset == 0 {
                    break;
                }
                offset += record.NextEntryOffset as usize;
                if offset >= BUFFER_BYTES {
                    return None;
                }
            }
        }
    }

    /// Safe leaf wrapper over `GetFileInformationByHandleEx`.
    fn get_file_information(
        handle: &File,
        class: windows_sys::Win32::Storage::FileSystem::FILE_INFO_BY_HANDLE_CLASS,
        buffer: &mut [u64],
    ) -> bool {
        use windows_sys::Win32::Foundation::SetLastError;
        use windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandleEx;
        // Cleared so the end-of-listing check below reads this call's
        // own answer rather than a stale error from earlier on the
        // thread.
        // SAFETY: `SetLastError` writes the calling thread's own
        // last-error slot and reads nothing.
        unsafe { SetLastError(0) };
        let bytes = std::mem::size_of_val(buffer) as u32;
        // SAFETY: the handle is open for the duration of the call, and
        // the pointer and length describe a live, exclusively borrowed
        // buffer of exactly that many bytes.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                handle.as_raw_handle(),
                class,
                buffer.as_mut_ptr().cast(),
                bytes,
            )
        };
        ok != 0
    }
}

pub fn volume_space(path: &std::path::Path) -> (Option<u64>, Option<u64>) {
    imp::volume_space(path)
}

/// Whether the volume under `path` is spinning media, where the platform
/// can say. `Some(true)` = rotational (HDD), `Some(false)` = solid
/// state, `None` = no answer (macOS, query failure, exotic devices).
///
/// Duplicate hashing uses this to pick its concurrency: hashing is
/// storage-bound, and on spinning media many workers seeking between
/// large files is slower than one or two streaming sequentially, while
/// solid state devices want the parallelism. An uncertain answer keeps
/// the parallel default.
#[cfg(target_os = "linux")]
pub fn storage_is_rotational(path: &std::path::Path) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;
    let dev = std::fs::metadata(path).ok()?.dev();
    let (major, minor) = (libc::major(dev), libc::minor(dev));
    // A partition's sysfs node has no `queue/` of its own — the
    // whole-disk parent one level up does, so try both.
    let direct = format!("/sys/dev/block/{major}:{minor}/queue/rotational");
    let parent = format!("/sys/dev/block/{major}:{minor}/../queue/rotational");
    let text = std::fs::read_to_string(&direct)
        .or_else(|_| std::fs::read_to_string(&parent))
        .ok()?;
    Some(text.trim() == "1")
}

#[cfg(windows)]
pub fn storage_is_rotational(path: &std::path::Path) -> Option<bool> {
    let letter = drive_letter(path)?;
    let volume = format!(r"\\.\{}:", letter as char);
    let handle = open_volume_for_query(&volume)?;
    seek_penalty(&handle)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn storage_is_rotational(_path: &std::path::Path) -> Option<bool> {
    None
}

/// The drive letter of `path`'s prefix, when it has one. UNC and device
/// paths answer `None` — their storage is not queryable this way.
#[cfg(windows)]
fn drive_letter(path: &std::path::Path) -> Option<u8> {
    let std::path::Component::Prefix(prefix) = path.components().next()? else {
        return None;
    };
    match prefix.kind() {
        std::path::Prefix::Disk(letter) | std::path::Prefix::VerbatimDisk(letter) => Some(letter),
        std::path::Prefix::Verbatim(_)
        | std::path::Prefix::VerbatimUNC(..)
        | std::path::Prefix::DeviceNS(_)
        | std::path::Prefix::UNC(..) => None,
    }
}

/// Safe leaf wrapper over `CreateFileW`, opening a volume with zero
/// access rights — enough for metadata queries, nothing else. The
/// returned handle closes itself on drop.
#[cfg(windows)]
fn open_volume_for_query(volume: &str) -> Option<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let wide: Vec<u16> = volume.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a NUL-terminated wide string valid for the
    // duration of the call; every other argument is a plain flag, zero,
    // or null, all documented-valid for opening an existing volume.
    let raw = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: `raw` is the valid handle `CreateFileW` just returned and
    // nothing else owns it — wrapping hands ownership (and the eventual
    // `CloseHandle`) to the returned value, so early returns cannot
    // leak it.
    Some(unsafe { OwnedHandle::from_raw_handle(raw) })
}

/// Safe leaf wrapper over the `IOCTL_STORAGE_QUERY_PROPERTY` seek-
/// penalty query: `Some(true)` when the device incurs one (spinning
/// media), `Some(false)` when it does not, `None` when the query fails
/// (drivers are not obliged to implement it).
#[cfg(windows)]
fn seek_penalty(handle: &std::os::windows::io::OwnedHandle) -> Option<bool> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceSeekPenaltyProperty, DEVICE_SEEK_PENALTY_DESCRIPTOR,
        IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_PROPERTY_QUERY,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceSeekPenaltyProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut descriptor = MaybeUninit::<DEVICE_SEEK_PENALTY_DESCRIPTOR>::uninit();
    let mut written: u32 = 0;
    // SAFETY: the handle is open for the duration of the call, `query`
    // outlives it, `descriptor` is a live out-buffer of exactly the
    // size passed, and `written` is a live `u32` — all as the API
    // documents. The descriptor is read only on the success path.
    let ok = unsafe {
        DeviceIoControl(
            handle.as_raw_handle(),
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&query as *const STORAGE_PROPERTY_QUERY).cast(),
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            descriptor.as_mut_ptr().cast(),
            std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            &mut written,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || (written as usize) < std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() {
        return None;
    }
    // SAFETY: the callee reported success and wrote the whole
    // descriptor.
    let descriptor = unsafe { descriptor.assume_init() };
    Some(descriptor.IncursSeekPenalty)
}

#[cfg(unix)]
mod imp {
    use std::ffi::{CStr, CString};
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use std::path::PathBuf;

    /// The volumes a picker should offer.
    ///
    /// The root always, plus whatever is mounted somewhere a person
    /// would recognise. Linux is read from `/proc/mounts` and filtered to
    /// mounts backed by a real device — without that filter the list is
    /// mostly `proc`, `sysfs`, `cgroup` and a snap loopback per installed
    /// application. macOS has no `/proc/mounts`, and everything a user
    /// mounts appears under `/Volumes`, so that directory *is* the list.
    pub(super) fn volumes() -> Vec<super::Volume> {
        let mut out = vec![describe(PathBuf::from("/"), "/".to_string())];

        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            for line in mounts.lines() {
                let mut fields = line.split_whitespace();
                let (Some(source), Some(target)) = (fields.next(), fields.next()) else {
                    continue;
                };
                // A real block device, mounted somewhere that is not the
                // root we already have.
                if !source.starts_with("/dev/") || target == "/" {
                    continue;
                }
                let target = unescape_mount(target);
                let path = PathBuf::from(&target);
                if out.iter().any(|volume| volume.path == path) {
                    continue;
                }
                out.push(describe(path, target));
            }
        }

        for parent in ["/Volumes", "/media", "/run/media"] {
            let Ok(listing) = std::fs::read_dir(parent) else {
                continue;
            };
            for entry in listing.flatten() {
                let path = entry.path();
                if !path.is_dir() || out.iter().any(|volume| volume.path == path) {
                    continue;
                }
                let label = path.to_string_lossy().to_string();
                out.push(describe(path, label));
            }
        }
        out
    }

    /// `/proc/mounts` escapes spaces and a few other characters as octal.
    /// A mount point called "My Disk" arrives as `My\040Disk`, and a
    /// picker offering that as a path would fail to scan it.
    fn unescape_mount(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            let digits: String = chars.clone().take(3).collect();
            match u8::from_str_radix(&digits, 8) {
                Ok(byte) if digits.len() == 3 => {
                    out.push(byte as char);
                    for _ in 0..3 {
                        chars.next();
                    }
                }
                _ => out.push(c),
            }
        }
        out
    }

    fn describe(path: PathBuf, label: String) -> super::Volume {
        let (free, total) = volume_space(&path);
        super::Volume {
            path,
            label,
            free,
            total,
        }
    }

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
    use std::path::PathBuf;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    use windows_sys::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE};

    /// The drives a picker should offer.
    ///
    /// Fixed and removable only. A network drive can be scanned by
    /// typing its path, but offering one here invites a scan across a
    /// link whose latency is measured in milliseconds per directory, and
    /// a CD-ROM or an empty card reader is a row that answers nothing.
    pub(super) fn volumes() -> Vec<super::Volume> {
        let mask = logical_drives();
        (0..26u32)
            .filter(|letter| mask & (1 << letter) != 0)
            .filter_map(|letter| {
                let letter = char::from_u32(u32::from(b'A') + letter)?;
                let label = format!("{letter}:");
                let path = PathBuf::from(format!("{label}\\"));
                let kind = drive_type(&path);
                if kind != DRIVE_FIXED && kind != DRIVE_REMOVABLE {
                    return None;
                }
                let (free, total) = volume_space(&path);
                // A removable drive with no media reports nothing; it is
                // a slot rather than a volume, so it is not offered.
                total?;
                Some(super::Volume {
                    path,
                    label,
                    free,
                    total,
                })
            })
            .collect()
    }

    /// Safe leaf wrapper over `GetLogicalDrives`: a bitmask of drive
    /// letters, bit 0 being `A:`.
    fn logical_drives() -> u32 {
        // SAFETY: the call takes no arguments and only reads process
        // state.
        unsafe { GetLogicalDrives() }
    }

    /// Safe leaf wrapper over `GetDriveTypeW`.
    fn drive_type(path: &Path) -> u32 {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);
        // SAFETY: `wide` is a NUL-terminated wide string that outlives
        // the call, which reads it and nothing else.
        unsafe { GetDriveTypeW(wide.as_ptr()) }
    }

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

    /// No volume model to enumerate, so the picker offers the folder
    /// button and nothing else.
    pub(super) fn volumes() -> Vec<super::Volume> {
        Vec::new()
    }
}

#[cfg(test)]
mod rotational_tests {
    use super::*;

    /// The FFI plumbing, not the hardware: whatever volume the temp dir
    /// sits on, the query must come back as a clean `Option` rather
    /// than failing — on Windows this drives the volume-open and IOCTL
    /// marshalling for real, on Linux the sysfs walk.
    #[test]
    fn the_seek_penalty_query_answers_cleanly() {
        let answer = storage_is_rotational(&std::env::temp_dir());
        assert!(
            matches!(answer, None | Some(true) | Some(false)),
            "any honest answer is fine; a panic or hang is the failure mode"
        );
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
