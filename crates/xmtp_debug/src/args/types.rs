use std::time::Duration;

use color_eyre::eyre::ensure;

#[derive(Debug, Clone)]
pub struct InboxId([u8; 32]);

impl std::fmt::Display for InboxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl std::ops::Deref for InboxId {
    type Target = [u8; 32];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::str::FromStr for InboxId {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut slice = [0u8; 32];
        hex::decode_to_slice(s, &mut slice)?;
        Ok(InboxId(slice))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroupId([u8; 16]);

impl std::fmt::Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl std::ops::Deref for GroupId {
    type Target = [u8; 16];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::str::FromStr for GroupId {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut slice = [0u8; 16];
        hex::decode_to_slice(s, &mut slice)?;
        Ok(GroupId(slice))
    }
}

impl TryFrom<Vec<u8>> for GroupId {
    type Error = color_eyre::eyre::Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        ensure!(value.len() == 16, "a group id must be 16 bytes long");
        let mut id = [0u8; 16];
        id.copy_from_slice(value.as_slice());
        Ok(GroupId(id))
    }
}

/// Millisecond Interval with a default of 1 second.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct MillisecondInterval(Duration);

impl Default for MillisecondInterval {
    fn default() -> Self {
        MillisecondInterval(Duration::from_secs(1))
    }
}

impl std::fmt::Display for MillisecondInterval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_millis())
    }
}

impl std::ops::Deref for MillisecondInterval {
    type Target = Duration;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::str::FromStr for MillisecondInterval {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let millis = s.parse()?;
        Ok(MillisecondInterval(Duration::from_millis(millis)))
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Concurrency(usize);

impl Default for Concurrency {
    fn default() -> Self {
        Concurrency(num_cpus::get())
    }
}

impl std::fmt::Display for Concurrency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Concurrency {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let n = s.parse()?;
        Ok(Concurrency(n))
    }
}

impl std::ops::Deref for Concurrency {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
