use std::collections::HashMap;

use super::{Cursor, Topic};

/// Independent positions for backend topics.
pub type TopicCursor = HashMap<Topic, Cursor>;
