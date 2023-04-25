pub trait Persistence {
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
