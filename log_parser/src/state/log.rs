use crate::state::LogEvent;
use anyhow::Result;
use std::{
    collections::{HashMap, VecDeque},
    fs::read_to_string,
    path::Path,
};

pub struct LogFile {
    // It would be faster to use a vec+tuple, but a hashmap is simpler.
    pub streams: HashMap<String, VecDeque<LogEvent>>,
}

impl LogFile {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let file = read_to_string(path)?;
        Self::from_str(&file)
    }

    pub fn from_str(file: &str) -> Result<Self> {
        let mut streams = HashMap::new();
        let lines = file.split("\n");

        // Read the entire file into memory for now.
        for line in lines {
            let Ok(log) = LogEvent::from(&line) else {
                continue;
            };

            let events = streams
                .entry(log.installation.clone())
                .or_insert_with(|| VecDeque::new());

            events.push_back(log);
        }

        Ok(Self { streams })
    }
}
