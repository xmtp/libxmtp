//! A benchmark-only SQLite VFS that wraps the platform default VFS and injects
//! a configurable, per-operation delay on `xSync` (fsync), `xWrite`, and
//! `xRead`, plus per-operation counters.
//!
//! This models network-attached storage (e.g. EFS), where every I/O operation
//! pays a round-trip latency, so we can measure how libxmtp's fresh-DB creation
//! (the ~63 Diesel migrations) scales with disk / fsync latency.
//!
//! It is a faithful pass-through wrapper in the style of SQLite's `vfstrace.c`:
//! every VFS- and file-level method delegates to the real default VFS, and the
//! wrapper only adds a `thread::sleep` before the delegated call on the three
//! I/O methods we care about. Because SQLCipher is a codec layered *above* the
//! pager, real I/O still flows through this VFS, so its numbers are
//! representative of the encrypted store.
//!
//! WAL mode requires the shared-memory (`xShm*`) and mmap (`xFetch`/`xUnfetch`)
//! file methods (io_methods iVersion >= 2/3), so those are forwarded too.
//!
//! Enabled only under the `bench` feature on native targets. It is NOT for
//! production use: it registers itself as the process-wide default VFS and
//! deliberately makes I/O slow.

// Every function in this module is a thin FFI trampoline whose entire body is
// unsafe pointer work; per-op `unsafe {}` blocks would only add noise.
#![allow(unsafe_op_in_unsafe_fn)]
// The wrapped VFS/io_methods function pointers are `Option`s that SQLite
// guarantees are populated for the platform default VFS we delegate to, so
// `unwrap()` on them cannot fire in practice.
#![allow(clippy::unwrap_used)]

use libsqlite3_sys as ffi;
use parking_lot::RwLock;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const VFS_NAME: &CStr = c"xmtp-latency";

// Per-op injected delay, in nanoseconds. Read on every matching I/O op.
static SYNC_DELAY_NS: AtomicU64 = AtomicU64::new(0);
static WRITE_DELAY_NS: AtomicU64 = AtomicU64::new(0);
static READ_DELAY_NS: AtomicU64 = AtomicU64::new(0);

// Total xOpen calls seen, regardless of scope. Diagnostic: distinguishes "VFS
// not used at all" (0) from "used but nothing in scope" (>0 while open==0).
static TOTAL_OPEN: AtomicU64 = AtomicU64::new(0);

// Per-op counters (only counted for in-scope files).
static OPEN_COUNT: AtomicU64 = AtomicU64::new(0);
static SYNC_COUNT: AtomicU64 = AtomicU64::new(0);
static WRITE_COUNT: AtomicU64 = AtomicU64::new(0);
static READ_COUNT: AtomicU64 = AtomicU64::new(0);

static SCOPE_PREFIX: RwLock<Option<String>> = RwLock::new(None);

static REGISTER: Once = Once::new();

/// Snapshot of the in-scope I/O op counters.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counts {
    /// xOpen calls for in-scope files.
    pub open: u64,
    /// xOpen calls across all files (diagnostic — is the VFS being used at all?).
    pub total_open: u64,
    pub sync: u64,
    pub write: u64,
    pub read: u64,
}

/// Register the latency VFS as the process-wide default. Idempotent.
///
/// Must be called before any SQLite connection is opened for the delay to take
/// effect (Diesel's `establish` uses the default VFS).
pub fn register() -> Result<(), c_int> {
    let mut result = ffi::SQLITE_OK;
    REGISTER.call_once(|| unsafe {
        // SAFETY: `sqlite3_vfs_find(NULL)` returns the current default VFS.
        let real = ffi::sqlite3_vfs_find(std::ptr::null());
        if real.is_null() {
            result = ffi::SQLITE_ERROR;
            return;
        }
        // Copy every field (including all the delegate function pointers) from
        // the real VFS, then override the ones we wrap.
        let mut vfs: ffi::sqlite3_vfs = *real;
        vfs.pNext = std::ptr::null_mut();
        vfs.zName = VFS_NAME.as_ptr();
        // Stash the real VFS so our wrappers can delegate to it with the
        // pointer the real implementation expects.
        vfs.pAppData = real as *mut c_void;
        // Each wrapped file embeds the real file's storage immediately after
        // our header (see `LatencyFile`).
        vfs.szOsFile = (*real).szOsFile + std::mem::size_of::<LatencyFile>() as c_int;

        vfs.xOpen = Some(x_open);
        vfs.xDelete = Some(x_delete);
        vfs.xAccess = Some(x_access);
        vfs.xFullPathname = Some(x_full_pathname);
        vfs.xRandomness = Some(x_randomness);
        vfs.xSleep = Some(x_sleep);
        vfs.xCurrentTime = Some(x_current_time);
        vfs.xGetLastError = Some(x_get_last_error);
        if (*real).iVersion >= 2 {
            vfs.xCurrentTimeInt64 = Some(x_current_time_int64);
        }
        // Dynamic-loading (xDl*) and system-call (v3) hooks ignore the VFS
        // pointer in every platform implementation, so the copied pointers are
        // safe to keep as-is.

        // Leak the VFS so it lives for the process lifetime, as SQLite requires.
        let vfs = Box::leak(Box::new(vfs));
        result = ffi::sqlite3_vfs_register(vfs, 1 /* make default */);
    });
    if result == ffi::SQLITE_OK {
        Ok(())
    } else {
        Err(result)
    }
}

/// Only files whose name *contains* `needle` incur the injected delay and are
/// counted. Pass a unique DB filename token (e.g. `inbox-7.db3`); its
/// `-wal`/`-shm`/`-journal` siblings contain it too. A substring (not prefix)
/// match is used deliberately: SQLite canonicalizes paths before `xOpen` (on
/// macOS `/var` -> `/private/var`), so a leading directory prefix would not
/// match. `None` clears scoping (nothing is delayed).
pub fn set_scope_prefix(needle: Option<String>) {
    *SCOPE_PREFIX.write() = needle;
}

/// Set the per-operation injected delay for each of the three I/O ops.
pub fn set_delays(sync: Duration, write: Duration, read: Duration) {
    SYNC_DELAY_NS.store(sync.as_nanos() as u64, Ordering::Relaxed);
    WRITE_DELAY_NS.store(write.as_nanos() as u64, Ordering::Relaxed);
    READ_DELAY_NS.store(read.as_nanos() as u64, Ordering::Relaxed);
}

/// Reset all in-scope op counters to zero.
pub fn reset_counts() {
    TOTAL_OPEN.store(0, Ordering::Relaxed);
    OPEN_COUNT.store(0, Ordering::Relaxed);
    SYNC_COUNT.store(0, Ordering::Relaxed);
    WRITE_COUNT.store(0, Ordering::Relaxed);
    READ_COUNT.store(0, Ordering::Relaxed);
}

/// Snapshot the in-scope op counters.
pub fn counts() -> Counts {
    Counts {
        open: OPEN_COUNT.load(Ordering::Relaxed),
        total_open: TOTAL_OPEN.load(Ordering::Relaxed),
        sync: SYNC_COUNT.load(Ordering::Relaxed),
        write: WRITE_COUNT.load(Ordering::Relaxed),
        read: READ_COUNT.load(Ordering::Relaxed),
    }
}

#[inline]
fn sleep_ns(ns: u64) {
    if ns > 0 {
        std::thread::sleep(Duration::from_nanos(ns));
    }
}

/// Our wrapping `sqlite3_file`. SQLite allocates `szOsFile` bytes for the file
/// object; we lay `LatencyFile` at the front and the real VFS's file storage
/// immediately after it (reachable via `real`), mirroring `vfstrace.c`.
#[repr(C)]
struct LatencyFile {
    base: ffi::sqlite3_file,
    real: *mut ffi::sqlite3_file,
    in_scope: c_int,
}

#[inline]
unsafe fn real_vfs(vfs: *mut ffi::sqlite3_vfs) -> *mut ffi::sqlite3_vfs {
    (*vfs).pAppData as *mut ffi::sqlite3_vfs
}

#[inline]
unsafe fn real_methods(file: *mut ffi::sqlite3_file) -> *const ffi::sqlite3_io_methods {
    let p = file as *mut LatencyFile;
    (*(*p).real).pMethods
}

unsafe fn name_in_scope(z_name: *const c_char) -> bool {
    if z_name.is_null() {
        return false;
    }
    let guard = SCOPE_PREFIX.read();
    let Some(needle) = guard.as_deref() else {
        return false;
    };
    match CStr::from_ptr(z_name).to_str() {
        Ok(name) => name.contains(needle),
        Err(_) => false,
    }
}

// ---- VFS-level wrappers (delegate to the real VFS pointer) ----

unsafe extern "C" fn x_open(
    vfs: *mut ffi::sqlite3_vfs,
    z_name: *const c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out_flags: *mut c_int,
) -> c_int {
    TOTAL_OPEN.fetch_add(1, Ordering::Relaxed);
    let real = real_vfs(vfs);
    let p = file as *mut LatencyFile;
    // Real file storage lives immediately after our header.
    (*p).real = (p as *mut u8).add(std::mem::size_of::<LatencyFile>()) as *mut ffi::sqlite3_file;
    let in_scope = name_in_scope(z_name);
    (*p).in_scope = in_scope as c_int;

    let rc = ((*real).xOpen.unwrap())(real, z_name, (*p).real, flags, out_flags);
    if rc != ffi::SQLITE_OK {
        (*p).base.pMethods = std::ptr::null();
        return rc;
    }
    if in_scope {
        OPEN_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    // Pick the methods table matching the real file's iVersion so SQLite never
    // calls a shm/fetch method the underlying file does not implement.
    let real_iversion = (*(*p).real).pMethods;
    (*p).base.pMethods = if !real_iversion.is_null() && (*real_iversion).iVersion >= 3 {
        &METHODS_V3
    } else {
        &METHODS_V1
    };
    ffi::SQLITE_OK
}

unsafe extern "C" fn x_delete(
    vfs: *mut ffi::sqlite3_vfs,
    z_name: *const c_char,
    sync_dir: c_int,
) -> c_int {
    let real = real_vfs(vfs);
    ((*real).xDelete.unwrap())(real, z_name, sync_dir)
}

unsafe extern "C" fn x_access(
    vfs: *mut ffi::sqlite3_vfs,
    z_name: *const c_char,
    flags: c_int,
    res_out: *mut c_int,
) -> c_int {
    let real = real_vfs(vfs);
    ((*real).xAccess.unwrap())(real, z_name, flags, res_out)
}

unsafe extern "C" fn x_full_pathname(
    vfs: *mut ffi::sqlite3_vfs,
    z_name: *const c_char,
    n_out: c_int,
    z_out: *mut c_char,
) -> c_int {
    let real = real_vfs(vfs);
    ((*real).xFullPathname.unwrap())(real, z_name, n_out, z_out)
}

unsafe extern "C" fn x_randomness(
    vfs: *mut ffi::sqlite3_vfs,
    n_byte: c_int,
    z_out: *mut c_char,
) -> c_int {
    let real = real_vfs(vfs);
    ((*real).xRandomness.unwrap())(real, n_byte, z_out)
}

unsafe extern "C" fn x_sleep(vfs: *mut ffi::sqlite3_vfs, microseconds: c_int) -> c_int {
    let real = real_vfs(vfs);
    ((*real).xSleep.unwrap())(real, microseconds)
}

unsafe extern "C" fn x_current_time(vfs: *mut ffi::sqlite3_vfs, out: *mut f64) -> c_int {
    let real = real_vfs(vfs);
    ((*real).xCurrentTime.unwrap())(real, out)
}

unsafe extern "C" fn x_current_time_int64(
    vfs: *mut ffi::sqlite3_vfs,
    out: *mut ffi::sqlite3_int64,
) -> c_int {
    let real = real_vfs(vfs);
    ((*real).xCurrentTimeInt64.unwrap())(real, out)
}

unsafe extern "C" fn x_get_last_error(
    vfs: *mut ffi::sqlite3_vfs,
    n: c_int,
    z: *mut c_char,
) -> c_int {
    let real = real_vfs(vfs);
    ((*real).xGetLastError.unwrap())(real, n, z)
}

// ---- File-level wrappers (delegate to the real file's methods) ----

unsafe extern "C" fn f_close(file: *mut ffi::sqlite3_file) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    let rc = ((*m).xClose.unwrap())((*p).real);
    (*p).base.pMethods = std::ptr::null();
    rc
}

unsafe extern "C" fn f_read(
    file: *mut ffi::sqlite3_file,
    buf: *mut c_void,
    amt: c_int,
    ofst: ffi::sqlite3_int64,
) -> c_int {
    let p = file as *mut LatencyFile;
    if (*p).in_scope != 0 {
        READ_COUNT.fetch_add(1, Ordering::Relaxed);
        sleep_ns(READ_DELAY_NS.load(Ordering::Relaxed));
    }
    let m = real_methods(file);
    ((*m).xRead.unwrap())((*p).real, buf, amt, ofst)
}

unsafe extern "C" fn f_write(
    file: *mut ffi::sqlite3_file,
    buf: *const c_void,
    amt: c_int,
    ofst: ffi::sqlite3_int64,
) -> c_int {
    let p = file as *mut LatencyFile;
    if (*p).in_scope != 0 {
        WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
        sleep_ns(WRITE_DELAY_NS.load(Ordering::Relaxed));
    }
    let m = real_methods(file);
    ((*m).xWrite.unwrap())((*p).real, buf, amt, ofst)
}

unsafe extern "C" fn f_truncate(file: *mut ffi::sqlite3_file, size: ffi::sqlite3_int64) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xTruncate.unwrap())((*p).real, size)
}

unsafe extern "C" fn f_sync(file: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
    let p = file as *mut LatencyFile;
    if (*p).in_scope != 0 {
        SYNC_COUNT.fetch_add(1, Ordering::Relaxed);
        sleep_ns(SYNC_DELAY_NS.load(Ordering::Relaxed));
    }
    let m = real_methods(file);
    ((*m).xSync.unwrap())((*p).real, flags)
}

unsafe extern "C" fn f_file_size(
    file: *mut ffi::sqlite3_file,
    size: *mut ffi::sqlite3_int64,
) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xFileSize.unwrap())((*p).real, size)
}

unsafe extern "C" fn f_lock(file: *mut ffi::sqlite3_file, lock: c_int) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xLock.unwrap())((*p).real, lock)
}

unsafe extern "C" fn f_unlock(file: *mut ffi::sqlite3_file, lock: c_int) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xUnlock.unwrap())((*p).real, lock)
}

unsafe extern "C" fn f_check_reserved_lock(
    file: *mut ffi::sqlite3_file,
    res_out: *mut c_int,
) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xCheckReservedLock.unwrap())((*p).real, res_out)
}

unsafe extern "C" fn f_file_control(
    file: *mut ffi::sqlite3_file,
    op: c_int,
    arg: *mut c_void,
) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xFileControl.unwrap())((*p).real, op, arg)
}

unsafe extern "C" fn f_sector_size(file: *mut ffi::sqlite3_file) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xSectorSize.unwrap())((*p).real)
}

unsafe extern "C" fn f_device_characteristics(file: *mut ffi::sqlite3_file) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xDeviceCharacteristics.unwrap())((*p).real)
}

unsafe extern "C" fn f_shm_map(
    file: *mut ffi::sqlite3_file,
    i_pg: c_int,
    pgsz: c_int,
    b_extend: c_int,
    pp: *mut *mut c_void,
) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xShmMap.unwrap())((*p).real, i_pg, pgsz, b_extend, pp)
}

unsafe extern "C" fn f_shm_lock(
    file: *mut ffi::sqlite3_file,
    offset: c_int,
    n: c_int,
    flags: c_int,
) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xShmLock.unwrap())((*p).real, offset, n, flags)
}

unsafe extern "C" fn f_shm_barrier(file: *mut ffi::sqlite3_file) {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xShmBarrier.unwrap())((*p).real)
}

unsafe extern "C" fn f_shm_unmap(file: *mut ffi::sqlite3_file, delete_flag: c_int) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xShmUnmap.unwrap())((*p).real, delete_flag)
}

unsafe extern "C" fn f_fetch(
    file: *mut ffi::sqlite3_file,
    ofst: ffi::sqlite3_int64,
    amt: c_int,
    pp: *mut *mut c_void,
) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xFetch.unwrap())((*p).real, ofst, amt, pp)
}

unsafe extern "C" fn f_unfetch(
    file: *mut ffi::sqlite3_file,
    ofst: ffi::sqlite3_int64,
    pt: *mut c_void,
) -> c_int {
    let p = file as *mut LatencyFile;
    let m = real_methods(file);
    ((*m).xUnfetch.unwrap())((*p).real, ofst, pt)
}

// iVersion 1 methods: no shared-memory or mmap support.
static METHODS_V1: ffi::sqlite3_io_methods = ffi::sqlite3_io_methods {
    iVersion: 1,
    xClose: Some(f_close),
    xRead: Some(f_read),
    xWrite: Some(f_write),
    xTruncate: Some(f_truncate),
    xSync: Some(f_sync),
    xFileSize: Some(f_file_size),
    xLock: Some(f_lock),
    xUnlock: Some(f_unlock),
    xCheckReservedLock: Some(f_check_reserved_lock),
    xFileControl: Some(f_file_control),
    xSectorSize: Some(f_sector_size),
    xDeviceCharacteristics: Some(f_device_characteristics),
    xShmMap: None,
    xShmLock: None,
    xShmBarrier: None,
    xShmUnmap: None,
    xFetch: None,
    xUnfetch: None,
};

// iVersion 3 methods: full shared-memory (WAL) and mmap support.
static METHODS_V3: ffi::sqlite3_io_methods = ffi::sqlite3_io_methods {
    iVersion: 3,
    xClose: Some(f_close),
    xRead: Some(f_read),
    xWrite: Some(f_write),
    xTruncate: Some(f_truncate),
    xSync: Some(f_sync),
    xFileSize: Some(f_file_size),
    xLock: Some(f_lock),
    xUnlock: Some(f_unlock),
    xCheckReservedLock: Some(f_check_reserved_lock),
    xFileControl: Some(f_file_control),
    xSectorSize: Some(f_sector_size),
    xDeviceCharacteristics: Some(f_device_characteristics),
    xShmMap: Some(f_shm_map),
    xShmLock: Some(f_shm_lock),
    xShmBarrier: Some(f_shm_barrier),
    xShmUnmap: Some(f_shm_unmap),
    xFetch: Some(f_fetch),
    xUnfetch: Some(f_unfetch),
};
