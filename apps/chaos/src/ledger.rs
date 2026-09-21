//! Private, bounded records for one chaos run.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

const DEFAULT_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_FILES: usize = 256;
const RECORD_BYTES: usize = 64 * 1024;
const JSON_BYTES: usize = 4 * 1024 * 1024;
const ROUND_FILE_BYTES: u64 = 8 * 1024 * 1024;
const KEPT_ROUNDS: u64 = 2;
const EVIDENCE_METADATA_BYTES: u64 = 16 * 1024 * 1024;
const EVIDENCE_METADATA_FILES: usize = 3;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub(crate) bytes: u64,
    pub(crate) files: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: DEFAULT_BYTES,
            files: DEFAULT_FILES,
        }
    }
}

pub(crate) struct RunLedger {
    root: PathBuf,
    limits: Limits,
    round: Option<u64>,
}

impl RunLedger {
    pub(crate) fn new(root: impl AsRef<Path>, limits: Limits) -> Result<Self> {
        ensure!(
            limits.bytes > 0 && limits.files > 0,
            "invalid evidence limits"
        );
        private_dir(root.as_ref())?;
        let ledger = Self {
            root: root.as_ref().canonicalize()?,
            limits,
            round: None,
        };
        ledger.enforce_bounds()?;
        Ok(ledger)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn begin_round(&mut self, round: u64) -> Result<()> {
        if let Some(previous) = self.round {
            ensure!(round > previous, "round numbers must increase");
        }
        let rounds = self.root.join("rounds");
        private_dir(&rounds)?;
        for entry in fs::read_dir(&rounds)? {
            let entry = entry?;
            ensure!(entry.file_type()?.is_dir(), "unexpected round entry");
            let number: u64 = entry
                .file_name()
                .to_string_lossy()
                .parse()
                .context("invalid round directory")?;
            if number <= round.saturating_sub(KEPT_ROUNDS) && round >= KEPT_ROUNDS {
                // Validate the tree before removal. Never follow a link.
                inventory(&entry.path(), self.limits.files)?;
                fs::remove_dir_all(entry.path())?;
            }
        }
        private_dir(&rounds.join(round.to_string()))?;
        self.round = Some(round);
        self.enforce_bounds()
    }

    pub(crate) fn append(&self, kind: &str, value: &Value) -> Result<()> {
        valid_name(kind)?;
        let round = self.round.context("no active round")?;
        let path = self
            .root
            .join("rounds")
            .join(round.to_string())
            .join(format!("{kind}.jsonl"));
        let mut data = bounded_json(value, RECORD_BYTES)?;
        data.push(b'\n');
        let current = regular_size(&path)?.unwrap_or(0);
        ensure!(
            current + data.len() as u64 <= ROUND_FILE_BYTES,
            "round ledger byte cap reached"
        );
        self.reserve(data.len() as u64, usize::from(!path.exists()))?;
        let mut file = private_file(&path, false)?;
        file.write_all(&data)?;
        file.sync_data()?;
        Ok(())
    }

    pub(crate) fn write_status(&self, value: &Value) -> Result<()> {
        self.write_json("status.json", value)
    }

    pub(crate) fn write_json(&self, name: &str, value: &Value) -> Result<()> {
        self.write_bytes(name, &bounded_json(value, JSON_BYTES)?)
    }

    pub(crate) fn write_bytes(&self, name: &str, bytes: &[u8]) -> Result<()> {
        valid_name(name)?;
        let destination = self.root.join(name);
        regular_size(&destination)?;
        let temporary = self.root.join(format!(".{name}.tmp"));
        ensure!(!temporary.try_exists()?, "unfinished atomic evidence write");
        self.reserve(bytes.len() as u64, 1)?;
        let result = (|| -> Result<()> {
            let mut file = private_file(&temporary, true)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &destination)?;
            File::open(&self.root)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() && regular_size(&temporary)?.is_some() {
            fs::remove_file(temporary)?;
        }
        result
    }

    pub(crate) fn enforce_bounds(&self) -> Result<()> {
        self.reserve(0, 0)
    }

    /// Keep room for stopped database copies and the final evidence records.
    /// The supervisor checks this between healthy rounds. Bundle writes use
    /// `enforce_bounds`, which applies the full hard limit without reserving twice.
    pub(crate) fn enforce_healthy_bounds(&self) -> Result<()> {
        let (used_bytes, used_files) = inventory(&self.root, self.limits.files)?;
        let metadata_bytes = EVIDENCE_METADATA_BYTES.min(self.limits.bytes / 8);
        let healthy_bytes = self.limits.bytes.saturating_sub(metadata_bytes) / 2;
        let healthy_files = self.limits.files.saturating_sub(EVIDENCE_METADATA_FILES) / 2;
        ensure!(
            used_bytes <= healthy_bytes,
            "healthy run byte budget reached ({used_bytes} > {healthy_bytes}); remaining space is reserved for violation database copies"
        );
        ensure!(
            used_files <= healthy_files,
            "healthy run file budget reached ({used_files} > {healthy_files}); remaining files are reserved for violation database copies"
        );
        Ok(())
    }

    pub(crate) fn reserve(&self, bytes: u64, files: usize) -> Result<()> {
        let (used_bytes, used_files) = inventory(&self.root, self.limits.files)?;
        ensure!(
            used_bytes.saturating_add(bytes) <= self.limits.bytes,
            "run directory byte cap reached"
        );
        ensure!(
            used_files.saturating_add(files) <= self.limits.files,
            "run directory file cap reached"
        );
        Ok(())
    }
}

pub(crate) fn valid_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && name.len() <= 128,
        "invalid evidence name"
    );
    ensure!(
        name.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte)),
        "invalid evidence name"
    );
    ensure!(name != "." && name != "..", "invalid evidence name");
    Ok(())
}

pub(crate) fn private_dir(path: &Path) -> Result<()> {
    // Walk each component so a parent link cannot redirect a private write.
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir),
            "parent path is not allowed"
        );
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "evidence path is not a directory"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new().mode(0o700).create(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(crate) fn regular_size(path: &Path) -> Result<Option<u64>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "evidence path is not a regular file"
            );
            Ok(Some(metadata.len()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn private_file(path: &Path, create_new: bool) -> Result<File> {
    regular_size(path)?;
    let file = OpenOptions::new()
        .write(true)
        .append(!create_new)
        .create(!create_new)
        .create_new(create_new)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

fn inventory(root: &Path, max_files: usize) -> Result<(u64, usize)> {
    let mut paths = vec![root.to_path_buf()];
    let mut bytes = 0_u64;
    let mut files = 0_usize;
    let mut directories = 0_usize;
    while let Some(path) = paths.pop() {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            // SQLite can remove a journal between directory enumeration and stat.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && path != root => {
                continue;
            }
            Err(error) => {
                return Err(error).with_context(|| format!("inventory {}", path.display()));
            }
        };
        ensure!(
            !metadata.file_type().is_symlink(),
            "links are not allowed in evidence"
        );
        if metadata.is_dir() {
            directories += 1;
            ensure!(
                directories <= max_files.saturating_add(1),
                "run directory count cap reached"
            );
            for entry in fs::read_dir(&path)? {
                paths.push(entry?.path());
                ensure!(
                    paths.len() <= max_files.saturating_add(1),
                    "run directory entry cap reached"
                );
            }
        } else if metadata.is_file() {
            files += 1;
            ensure!(files <= max_files, "run directory file cap reached");
            bytes = bytes.saturating_add(metadata.len());
        } else {
            bail!("special files are not allowed in evidence");
        }
    }
    Ok((bytes, files))
}

struct CappedWriter {
    bytes: Vec<u8>,
    cap: usize,
}

impl Write for CappedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(buffer.len()) > self.cap {
            return Err(std::io::Error::other("JSON record byte cap reached"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn bounded_json(value: &Value, cap: usize) -> Result<Vec<u8>> {
    let mut output = CappedWriter {
        bytes: Vec::new(),
        cap,
    };
    serde_json::to_writer(&mut output, value)?;
    Ok(output.bytes)
}

pub(crate) fn read_bounded(path: &Path, cap: usize) -> Result<Vec<u8>> {
    let size = regular_size(path)?.context("evidence file is missing")?;
    ensure!(size <= cap as u64, "evidence input byte cap reached");
    let mut output = Vec::new();
    File::open(path)?
        .take(cap as u64 + 1)
        .read_to_end(&mut output)?;
    ensure!(output.len() <= cap, "evidence input byte cap reached");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[xmtp_common::test(unwrap_try = true)]
    async fn inventory_allows_live_journal_removal() {
        let temp = tempfile::tempdir()?;
        fs::write(temp.path().join("active.db3"), [0_u8; 30])?;
        let journal = temp.path().join("active.db3-journal");
        let writer = std::thread::spawn(move || {
            for _ in 0..2000 {
                fs::write(&journal, [0_u8; 8]).unwrap();
                fs::remove_file(&journal).unwrap();
            }
        });
        for _ in 0..1000 {
            let (bytes, files) = inventory(temp.path(), 8)?;
            assert!(bytes >= 30);
            assert!((1..=2).contains(&files));
        }
        writer.join().unwrap();
        assert_eq!(inventory(temp.path(), 8)?, (30, 1));
        assert!(inventory(&temp.path().join("missing"), 8).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rotates_rounds_and_keeps_private_files() {
        let temp = tempfile::tempdir()?;
        let mut ledger = RunLedger::new(temp.path().canonicalize()?, Limits::default())?;
        for round in 0..4 {
            ledger.begin_round(round)?;
            ledger.append("operations", &json!({"round": round}))?;
        }
        assert!(!temp.path().join("rounds/1").exists());
        assert!(temp.path().join("rounds/2/operations.jsonl").exists());
        ledger.write_status(&json!({"round": 3}))?;
        assert_eq!(
            fs::metadata(temp.path())?.permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(temp.path().join("status.json"))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn cap_preserves_existing_status_and_database() {
        let temp = tempfile::tempdir()?;
        let ledger = RunLedger::new(
            temp.path().canonicalize()?,
            Limits {
                bytes: 48,
                files: 2,
            },
        )?;
        ledger.write_status(&json!({"ok": true}))?;
        fs::write(temp.path().join("active.db3"), [0_u8; 30])?;
        assert!(ledger.write_status(&json!({"next": true})).is_err());
        assert_eq!(
            fs::read(temp.path().join("status.json"))?,
            br#"{"ok":true}"#
        );
        assert_eq!(fs::metadata(temp.path().join("active.db3"))?.len(), 30);
        assert!(ledger.write_json("extra.json", &json!({})).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn refuses_links_and_oversized_records() {
        let temp = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let mut ledger = RunLedger::new(temp.path().canonicalize()?, Limits::default())?;
        ledger.begin_round(0)?;
        assert!(
            ledger
                .append("operations", &json!("x".repeat(RECORD_BYTES)))
                .is_err()
        );
        std::os::unix::fs::symlink(outside.path(), temp.path().join("escape"))?;
        assert!(ledger.enforce_bounds().is_err());
        assert!(RunLedger::new(temp.path().join("escape/run"), Limits::default()).is_err());
        assert!(ledger.write_json("../escape", &json!({})).is_err());
    }
}
