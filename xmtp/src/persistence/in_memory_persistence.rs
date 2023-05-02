use crate::persistence::Persistence;

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
    fn write(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        self.data.insert(key.to_string(), value.to_vec());
        Ok(())
    }

    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.data.get(&key.to_string()).cloned())
    }
}
