use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
#[cfg(windows)]
use cap_std::fs::MetadataExt;
#[cfg(windows)]
use cap_std::fs::OpenOptionsExt as _;
use cap_std::fs::{Dir, OpenOptions};
#[cfg(unix)]
use cap_std::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};

use super::{
    LocalStore, StagedFile, StoreFile, StoreMoveError, StoreWriter, is_reconcile_dir,
    validate_relative, validate_temp,
};
use crate::{AttachmentDecoder, AttachmentError, AttachmentFailureCause as Cause, DecodedMeta};

/// Create missing directory components with private permissions.
/// Existing directories keep their permissions.
pub async fn create_private_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let path = path.to_path_buf();
        xmtp_common::task::spawn_blocking(move || create_private_directories(&path))
            .await
            .map_err(io::Error::other)?
    }
    #[cfg(not(unix))]
    {
        tokio::fs::create_dir_all(path).await
    }
}

/// Files below one native attachments directory.
#[derive(Clone, Debug)]
pub struct NativeStore {
    root: PathBuf,
    // Resolve every child from this handle, even if the root path changes.
    root_dir: Arc<Dir>,
    #[cfg(test)]
    forced_hard_link_error: Option<std::io::ErrorKind>,
    #[cfg(test)]
    forced_source_unlink_error: Option<std::io::ErrorKind>,
    #[cfg(test)]
    fallback_race_bytes: Option<Vec<u8>>,
    #[cfg(test)]
    forced_chmod_error: bool,
    #[cfg(test)]
    forced_foreign_owner: bool,
}

impl NativeStore {
    pub async fn new(root: impl AsRef<Path>) -> Result<Self, AttachmentError> {
        let root = std::path::absolute(root.as_ref())
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        #[cfg(unix)]
        {
            let path = root.clone();
            xmtp_common::task::spawn_blocking(move || create_private_directories(&path))
                .await
                .map_err(storage_error)?
                .map_err(storage_error)?;
        }
        #[cfg(not(unix))]
        tokio::fs::create_dir_all(&root)
            .await
            .map_err(storage_error)?;
        let root_dir =
            Dir::open_ambient_dir(&root, cap_std::ambient_authority()).map_err(storage_error)?;
        Ok(Self {
            root,
            root_dir: Arc::new(root_dir),
            #[cfg(test)]
            forced_hard_link_error: None,
            #[cfg(test)]
            forced_source_unlink_error: None,
            #[cfg(test)]
            fallback_race_bytes: None,
            #[cfg(test)]
            forced_chmod_error: false,
            #[cfg(test)]
            forced_foreign_owner: false,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_forced_hard_link_error(mut self, kind: std::io::ErrorKind) -> Self {
        self.forced_hard_link_error = Some(kind);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_forced_source_unlink_error(mut self, kind: std::io::ErrorKind) -> Self {
        self.forced_source_unlink_error = Some(kind);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_fallback_destination_race(mut self, bytes: Vec<u8>) -> Self {
        self.fallback_race_bytes = Some(bytes);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_forced_chmod_error(mut self) -> Self {
        self.forced_chmod_error = true;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_forced_foreign_owner(mut self) -> Self {
        self.forced_foreign_owner = true;
        self
    }

    fn force_foreign_owner(&self) -> bool {
        #[cfg(test)]
        {
            self.forced_foreign_owner
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    fn force_chmod_error(&self) -> bool {
        #[cfg(test)]
        {
            self.forced_chmod_error
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    #[cfg(unix)]
    fn check_existing_owner(&self, parent: &Dir, name: &str) -> io::Result<()> {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

        let child = open_child_mode_handle(parent, name)?;
        let metadata = child.metadata()?;
        let owner = metadata.uid();
        if self.force_foreign_owner() || owner != unsafe { libc::geteuid() } {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        if metadata.permissions().mode() & 0o7777 != 0o700 {
            if self.force_chmod_error() {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            set_private_child_mode(&child)?;
        }
        Ok(())
    }

    fn remove_source(&self, parent: &Dir, name: &str) -> io::Result<()> {
        #[cfg(test)]
        if let Some(kind) = self.forced_source_unlink_error {
            return Err(io::Error::from(kind));
        }
        parent.remove_file(name)
    }

    fn hard_link(
        &self,
        from_parent: &Dir,
        from_name: &str,
        to_parent: &Dir,
        to_name: &str,
    ) -> io::Result<()> {
        #[cfg(test)]
        if let Some(kind) = self.forced_hard_link_error {
            return Err(io::Error::from(kind));
        }
        from_parent.hard_link(from_name, to_parent, to_name)
    }

    fn parent(&self, relative: &str, create: bool) -> io::Result<(Dir, String)> {
        let (parent, name) = relative.rsplit_once('/').unwrap_or(("", relative));
        let mut directory = self.root_dir.try_clone()?;
        if !parent.is_empty() {
            for part in parent.split('/') {
                directory = match directory.open_dir_nofollow(part) {
                    Ok(child) => {
                        #[cfg(unix)]
                        self.check_existing_owner(&directory, part)?;
                        child
                    }
                    Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                        #[cfg(unix)]
                        let mut builder = cap_std::fs::DirBuilder::new();
                        #[cfg(unix)]
                        builder.mode(0o700);
                        #[cfg(unix)]
                        let create_result = directory.create_dir_with(part, &builder);
                        #[cfg(not(unix))]
                        let create_result = directory.create_dir(part);
                        let created = match create_result {
                            Ok(()) => true,
                            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => false,
                            Err(error) => return Err(error),
                        };
                        #[cfg(unix)]
                        if created {
                            if self.force_chmod_error() {
                                tracing::warn!(
                                    part,
                                    "could not set private attachment directory permissions"
                                );
                            } else {
                                repair_created_child_mode(&directory, part)?;
                            }
                        }
                        #[cfg(not(unix))]
                        let _ = created;
                        let child = directory.open_dir_nofollow(part)?;
                        #[cfg(unix)]
                        if !created {
                            self.check_existing_owner(&directory, part)?;
                        }
                        child
                    }
                    Err(error) => return Err(error),
                };
            }
        }
        Ok((directory, name.to_owned()))
    }
}

#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
fn open_child_mode_handle(parent: &Dir, name: &str) -> io::Result<std::fs::File> {
    let fd = rustix::fs::openat(
        parent,
        name,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    Ok(fd.into())
}

#[cfg(all(
    unix,
    not(any(target_vendor = "apple", target_os = "linux", target_os = "android"))
))]
fn open_child_mode_handle(parent: &Dir, name: &str) -> io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = parent.open_with(name, &options)?;
    if !file.metadata()?.is_dir() {
        return Err(io::Error::from(io::ErrorKind::NotADirectory));
    }
    Ok(file.into_std())
}

#[cfg(unix)]
fn repair_created_child_mode(parent: &Dir, name: &str) -> io::Result<()> {
    let file = open_child_mode_handle(parent, name)?;
    #[cfg(test)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        record_initial_mode(file.metadata()?.permissions().mode(), 0o700);
    }
    set_private_child_mode(&file)
}

#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
fn set_private_child_mode(file: &std::fs::File) -> io::Result<()> {
    rustix::fs::fchmod(file, rustix::fs::Mode::from_raw_mode(0o700))?;
    Ok(())
}

#[cfg(all(
    unix,
    not(any(target_vendor = "apple", target_os = "linux", target_os = "android"))
))]
fn set_private_child_mode(file: &std::fs::File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    file.set_permissions(std::fs::Permissions::from_mode(0o700))
}

#[cfg(unix)]
fn create_private_directories(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let mut builder = std::fs::DirBuilder::new();
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        match builder.create(&current) {
            Ok(()) => {
                let directory = open_created_directory(&current)?;
                #[cfg(test)]
                record_initial_mode(directory.metadata()?.permissions().mode(), 0o700);
                directory.set_permissions(std::fs::Permissions::from_mode(0o700))?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !std::fs::metadata(&current)?.is_dir() {
                    return Err(io::Error::from(io::ErrorKind::NotADirectory));
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
fn open_created_directory(path: &Path) -> io::Result<std::fs::File> {
    let fd = rustix::fs::openat(
        rustix::fs::CWD,
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )?;
    Ok(fd.into())
}

#[cfg(all(
    unix,
    not(any(target_vendor = "apple", target_os = "linux", target_os = "android"))
))]
fn open_created_directory(path: &Path) -> io::Result<std::fs::File> {
    std::fs::File::open(path)
}

#[cfg(all(test, unix))]
static INITIAL_MODES: std::sync::Mutex<Vec<(u32, u32)>> = std::sync::Mutex::new(Vec::new());

#[cfg(all(test, unix))]
fn record_initial_mode(mode: u32, expected: u32) {
    if std::env::var_os("XMTP_TEST_INITIAL_MODES").is_some() {
        INITIAL_MODES.lock().unwrap().push((mode & 0o777, expected));
    }
}

#[cfg(all(test, unix))]
pub(super) fn take_initial_modes() -> Vec<(u32, u32)> {
    std::mem::take(&mut INITIAL_MODES.lock().unwrap())
}

fn storage_error(_: impl Sized) -> AttachmentError {
    AttachmentError::new(Cause::LocalStorage)
}

#[cfg(windows)]
fn is_link(metadata: &cap_std::fs::Metadata) -> bool {
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link(metadata: &cap_std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn exists_nofollow(parent: &Dir, name: &str) -> io::Result<bool> {
    match parent.symlink_metadata(name) {
        Ok(metadata) if is_link(&metadata) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "attachment path is a symlink",
        )),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
fn rename_no_replace(
    from_parent: &Dir,
    from_name: &str,
    to_parent: &Dir,
    to_name: &str,
) -> io::Result<()> {
    rustix::fs::renameat_with(
        from_parent,
        from_name,
        to_parent,
        to_name,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(Into::into)
}

#[cfg(windows)]
fn rename_no_replace(
    from_parent: &Dir,
    from_name: &str,
    to_parent: &Dir,
    to_name: &str,
) -> io::Result<()> {
    use std::{
        ffi::OsStr,
        os::windows::{ffi::OsStrExt, io::AsRawHandle},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_ADD_FILE, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_RENAME_INFO,
        FileRenameInfo, SetFileInformationByHandle,
    };

    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(DELETE | FILE_READ_ATTRIBUTES)
        .follow(FollowSymlinks::No);
    let source = from_parent.open_with(from_name, &options)?;
    let mut directory_options = OpenOptions::new();
    directory_options
        .read(true)
        .access_mode(FILE_ADD_FILE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    let destination = to_parent.open_with(".", &directory_options)?;
    let name: Vec<u16> = OsStr::new(to_name).encode_wide().collect();
    let name_bytes = name
        .len()
        .checked_mul(2)
        .ok_or(io::ErrorKind::InvalidInput)?;
    let size = std::mem::offset_of!(FILE_RENAME_INFO, FileName)
        .checked_add(name_bytes)
        .ok_or(io::ErrorKind::InvalidInput)?
        .max(std::mem::size_of::<FILE_RENAME_INFO>());
    let size = u32::try_from(size).map_err(|_| io::ErrorKind::InvalidInput)?;
    let name_bytes = u32::try_from(name_bytes).map_err(|_| io::ErrorKind::InvalidInput)?;
    let words = (size as usize).div_ceil(std::mem::size_of::<usize>());
    let mut storage = vec![0_usize; words];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // The buffer is aligned for FILE_RENAME_INFO and has room for the UTF-16 name.
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = destination.as_raw_handle();
        (*info).FileNameLength = name_bytes;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            name.len(),
        );
        if SetFileInformationByHandle(source.as_raw_handle(), FileRenameInfo, info.cast(), size)
            == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(not(any(
    target_vendor = "apple",
    target_os = "linux",
    target_os = "android",
    windows
)))]
fn rename_no_replace(
    _from_parent: &Dir,
    _from_name: &str,
    _to_parent: &Dir,
    _to_name: &str,
) -> io::Result<()> {
    // Other native targets, including BSD and Solaris, have no fallback here.
    // Keep both files unchanged when hard links are unavailable.
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic no-replace rename is unavailable",
    ))
}

#[async_trait::async_trait]
impl LocalStore for NativeStore {
    async fn open_read(&self, path: &str) -> Result<StagedFile, AttachmentError> {
        validate_relative(path)?;
        let (parent, name) = self.parent(path, false).map_err(storage_error)?;
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = parent.open_with(&name, &options).map_err(storage_error)?;
        if !file.metadata().map_err(storage_error)?.is_file() {
            return Err(storage_error(()));
        }
        Ok(StagedFile {
            path: self.root.join(path),
            opened: Some(Arc::new(file.into_std())),
        })
    }

    async fn create_temp(&self, path: &str) -> Result<StoreWriter, AttachmentError> {
        validate_temp(path)?;
        let (parent, name) = self.parent(path, true).map_err(storage_error)?;
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        options.mode(0o600);
        let file = parent.open_with(&name, &options).map_err(storage_error)?;
        #[cfg(all(test, unix))]
        record_initial_mode(
            file.metadata().map_err(storage_error)?.permissions().mode(),
            0o600,
        );
        #[cfg(unix)]
        if let Err(error) = file.set_permissions(cap_std::fs::Permissions::from_mode(0o600)) {
            let _ = parent.remove_file(&name);
            return Err(storage_error(error));
        }
        Ok(StoreWriter {
            file: tokio::fs::File::from_std(file.into_std()),
        })
    }

    async fn create_dir_if_absent(&self, path: &str) -> Result<bool, AttachmentError> {
        validate_relative(path)?;
        let (parent, name) = self.parent(path, true).map_err(storage_error)?;
        #[cfg(unix)]
        let mut builder = cap_std::fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        #[cfg(unix)]
        let result = parent.create_dir_with(&name, &builder);
        #[cfg(not(unix))]
        let result = parent.create_dir(&name);
        let created = match result {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => false,
            Err(error) => return Err(storage_error(error)),
        };
        #[cfg(unix)]
        if created {
            if self.force_chmod_error() {
                tracing::warn!(%name, "could not set private attachment directory permissions");
            } else {
                repair_created_child_mode(&parent, &name).map_err(storage_error)?;
            }
        }
        let _child = parent.open_dir_nofollow(&name).map_err(storage_error)?;
        #[cfg(unix)]
        if !created {
            self.check_existing_owner(&parent, &name)
                .map_err(storage_error)?;
        }
        Ok(created)
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), StoreMoveError> {
        validate_relative(from)?;
        validate_relative(to)?;
        let (from_parent, from_name) = self.parent(from, false).map_err(storage_error)?;
        let source = from_parent
            .symlink_metadata(&from_name)
            .map_err(storage_error)?;
        if !source.is_file() || is_link(&source) {
            return Err(storage_error(()).into());
        }
        let (to_parent, to_name) = self.parent(to, true).map_err(storage_error)?;
        if exists_nofollow(&to_parent, &to_name).map_err(storage_error)? {
            return Err(StoreMoveError::DestinationExists);
        }
        match self.hard_link(&from_parent, &from_name, &to_parent, &to_name) {
            Ok(()) => match self.remove_source(&from_parent, &from_name) {
                Ok(()) => Ok(()),
                Err(_) => {
                    let _ = to_parent.remove_file(&to_name);
                    Err(storage_error(()).into())
                }
            },
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(StoreMoveError::DestinationExists)
            }
            Err(_) => {
                // Some file systems cannot make hard links. The fallback must
                // reject a destination created after this check.
                if exists_nofollow(&to_parent, &to_name).map_err(storage_error)? {
                    return Err(StoreMoveError::DestinationExists);
                }
                #[cfg(test)]
                if let Some(bytes) = &self.fallback_race_bytes {
                    use std::io::Write;
                    let mut options = OpenOptions::new();
                    options.write(true).create_new(true);
                    to_parent
                        .open_with(&to_name, &options)
                        .and_then(|mut file| file.write_all(bytes))
                        .map_err(storage_error)?;
                }
                rename_no_replace(&from_parent, &from_name, &to_parent, &to_name).map_err(|error| {
                    if error.kind() == io::ErrorKind::AlreadyExists {
                        StoreMoveError::DestinationExists
                    } else {
                        storage_error(error).into()
                    }
                })
            }
        }
    }

    async fn replace(&self, from: &str, to: &str) -> Result<(), AttachmentError> {
        validate_relative(from)?;
        validate_relative(to)?;
        let (from_parent, from_name) = self.parent(from, false).map_err(storage_error)?;
        let source = from_parent
            .symlink_metadata(&from_name)
            .map_err(storage_error)?;
        if !source.is_file() || is_link(&source) {
            return Err(storage_error(()));
        }
        let (to_parent, to_name) = self.parent(to, true).map_err(storage_error)?;
        match to_parent.symlink_metadata(&to_name) {
            Ok(metadata) if !metadata.is_file() || is_link(&metadata) => {
                return Err(storage_error(()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage_error(error)),
        }
        from_parent
            .rename(&from_name, &to_parent, &to_name)
            .map_err(storage_error)
    }

    async fn remove_dir_all(&self, path: &str) -> Result<(), AttachmentError> {
        validate_relative(path)?;
        let (parent, name) = self.parent(path, false).map_err(storage_error)?;
        parent
            .open_dir_nofollow(&name)
            .map_err(storage_error)?
            .remove_open_dir_all()
            .map_err(storage_error)
    }

    async fn remove_empty_dir(&self, path: &str) -> Result<(), AttachmentError> {
        validate_relative(path)?;
        let (parent, name) = self.parent(path, false).map_err(storage_error)?;
        parent.open_dir_nofollow(&name).map_err(storage_error)?;
        parent.remove_dir(&name).map_err(storage_error)
    }

    async fn remove_file(&self, path: &str) -> Result<(), AttachmentError> {
        validate_relative(path)?;
        let (parent, name) = self.parent(path, false).map_err(storage_error)?;
        let metadata = parent.symlink_metadata(&name).map_err(storage_error)?;
        if !metadata.is_file() || is_link(&metadata) {
            return Err(storage_error(()));
        }
        parent.remove_file(&name).map_err(storage_error)
    }

    async fn exists(&self, path: &str) -> Result<bool, AttachmentError> {
        validate_relative(path)?;
        let (parent, name) = match self.parent(path, false) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(storage_error(error)),
        };
        exists_nofollow(&parent, &name).map_err(storage_error)
    }

    async fn sync(&self, writer: &mut StoreWriter) -> Result<(), AttachmentError> {
        writer
            .file
            .sync_all()
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))
    }

    async fn finish_decode(
        &self,
        decoder: AttachmentDecoder,
        source: &str,
        output: &str,
    ) -> Result<DecodedMeta, AttachmentError> {
        validate_temp(source)?;
        validate_temp(output)?;
        let (source_parent, source_name) = self.parent(source, false).map_err(storage_error)?;
        let mut read_options = OpenOptions::new();
        read_options.read(true).follow(FollowSymlinks::No);
        let input = source_parent
            .open_with(&source_name, &read_options)
            .map_err(storage_error)?;
        let (output_parent, output_name) = self.parent(output, true).map_err(storage_error)?;
        let mut write_options = OpenOptions::new();
        write_options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        write_options.mode(0o600);
        let decoded = output_parent
            .open_with(&output_name, &write_options)
            .map_err(storage_error)?;
        #[cfg(unix)]
        if let Err(error) = decoded.set_permissions(cap_std::fs::Permissions::from_mode(0o600)) {
            let _ = output_parent.remove_file(&output_name);
            return Err(storage_error(error));
        }
        xmtp_common::task::spawn_blocking(move || {
            let mut input = input.into_std();
            let mut decoded = decoded.into_std();
            let meta = decoder.finish(&mut input, &mut decoded)?;
            decoded
                .sync_all()
                .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
            Ok(meta)
        })
        .await
        .map_err(|_| AttachmentError::new(Cause::LocalStorage))?
    }

    async fn list_files(&self) -> Result<Vec<StoreFile>, AttachmentError> {
        use cap_std::time::SystemClock;
        let root = self.root_dir.clone();
        xmtp_common::task::spawn_blocking(move || -> Result<Vec<StoreFile>, AttachmentError> {
            let mut files = Vec::new();
            let mut dirs = vec![(root.try_clone().map_err(storage_error)?, String::new())];
            while let Some((dir, prefix)) = dirs.pop() {
                for entry in dir.entries().map_err(storage_error)? {
                    let entry = entry.map_err(storage_error)?;
                    let Ok(name) = entry.file_name().into_string() else {
                        continue;
                    };
                    let descend = prefix.is_empty() && is_reconcile_dir(&name);
                    let path = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix}/{name}")
                    };
                    let metadata = dir.symlink_metadata(&name).map_err(storage_error)?;
                    if metadata.is_dir() && !is_link(&metadata) {
                        if descend {
                            dirs.push((dir.open_dir_nofollow(&name).map_err(storage_error)?, path));
                        }
                    } else if metadata.is_file() && !is_link(&metadata) && !prefix.is_empty() {
                        let modified_at_ns = metadata
                            .modified()
                            .ok()
                            .and_then(|time| time.duration_since(SystemClock::UNIX_EPOCH).ok())
                            .map_or(0, |age| age.as_nanos().min(i64::MAX as u128) as i64);
                        files.push(StoreFile {
                            path,
                            modified_at_ns,
                        });
                    }
                }
            }
            Ok(files)
        })
        .await
        .map_err(storage_error)?
    }
}
