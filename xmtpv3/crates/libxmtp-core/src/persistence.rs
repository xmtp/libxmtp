// WARNING: Send and Sync are unsafe traits. We add them here to make the compiler happy,
// knowing that the entry-point to this library should be single-threaded. This code will
// FAIL in unexpected ways in a multi-threaded environment.
pub trait Persistence: Send + Sync {
    fn write(&mut self, key: String, value: &[u8]) -> Result<(), String>;
    fn read(&self, key: String) -> Result<Option<Vec<u8>>, String>;
}

pub struct InMemoryPersistence {
    data: std::collections::HashMap<String, Vec<u8>>,
}

impl InMemoryPersistence {
    pub fn new() -> Self {
        InMemoryPersistence {
            data: std::collections::HashMap::new(),
        }
    }
}

impl Default for InMemoryPersistence {
    fn default() -> Self {
        Self::new()
    }
}

impl Persistence for InMemoryPersistence {
    fn write(&mut self, key: String, value: &[u8]) -> Result<(), String> {
        self.data.insert(key, value.to_vec());
        Ok(())
    }

    fn read(&self, key: String) -> Result<Option<Vec<u8>>, String> {
        Ok(self.data.get(&key).cloned())
    }
}
