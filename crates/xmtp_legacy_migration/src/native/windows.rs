//! Windows file identity and SQLite byte-range lock checks.
use super::*;
use std::os::windows::io::AsRawHandle;
use windows_sys::Win32::{
    Foundation::ERROR_LOCK_VIOLATION,
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle, LOCKFILE_EXCLUSIVE_LOCK,
        LOCKFILE_FAIL_IMMEDIATELY, LockFileEx, UnlockFileEx,
    },
    System::IO::OVERLAPPED,
};

fn identity(file: &fs::File) -> io::Result<(u32, u32, u32)> {
    // SAFETY: the structure contains integer fields and the file handle is live.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
    ))
}

pub(super) fn same_file(source: &fs::File, target: &Path) -> Result<bool, MigrationError> {
    match fs::File::open(target) {
        Ok(target) => Ok(identity(source).map_err(input)? == identity(&target).map_err(input)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(input(e)),
    }
}

/// Windows exposes no lock query. Acquire and release the same byte range used
/// by SQLite. This operation does not change the file's data.
pub(super) fn check_locks(file: &fs::File, start: i64, len: i64) -> Result<(), MigrationError> {
    // SAFETY: zero initializes the unused event and internal fields.
    let mut position: OVERLAPPED = unsafe { std::mem::zeroed() };
    // SAFETY: use the offset member of OVERLAPPED, with no event handle.
    unsafe {
        position.Anonymous.Anonymous.Offset = start as u32;
        position.Anonymous.Anonymous.OffsetHigh = (start as u64 >> 32) as u32;
    }
    // SAFETY: the handle and structure are live for this synchronous call.
    if unsafe {
        LockFileEx(
            file.as_raw_handle(),
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            len as u32,
            0,
            &mut position,
        )
    } == 0
    {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) {
            Err(MigrationError::SourceBusy)
        } else {
            Err(input(error))
        };
    }
    // SAFETY: this releases exactly the range acquired above.
    if unsafe { UnlockFileEx(file.as_raw_handle(), 0, len as u32, 0, &mut position) } == 0 {
        return Err(input(io::Error::last_os_error()));
    }
    Ok(())
}
