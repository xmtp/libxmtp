use crate::state::LogEvent;
use anyhow::Result;
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{self, BufRead},
    path::{Path, PathBuf},
};

pub struct LogFile {
    pub path: PathBuf,
    // It would be faster to use a vec+tuple, but a hashmap is simpler.
    pub streams: HashMap<String, VecDeque<LogEvent>>,
}

impl LogFile {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = read_lines(path)?;
        let mut streams = HashMap::new();

        // Read the entire file into memory for now.
        for line in file.map_while(Result::ok) {
            let Ok(log) = LogEvent::from(&line) else {
                continue;
            };

            let events = streams
                .entry(log.inbox.clone())
                .or_insert_with(|| VecDeque::new());

            events.push_back(log);
        }

        Ok(Self {
            path: path.into(),
            streams,
        })
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
