pub mod in_memory_persistence;

pub trait Persistence {
    fn write(&mut self, key: &str, value: &[u8]) -> Result<(), String>;
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String>;
}

pub struct NamespacedPersistence<P: Persistence> {
    pub namespace: String,
    pub persistence: P,
}

impl<P: Persistence> NamespacedPersistence<P> {
    pub fn new(namespace: &str, persistence: P) -> Self {
        NamespacedPersistence {
            namespace: namespace.to_string(),
            persistence,
        }
    }

    pub fn write(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        let key = format!("{}/{}", self.namespace, key);
        self.persistence.write(&key, value)
    }

    pub fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        let key = format!("{}/{}", self.namespace, key);
        self.persistence.read(&key)
    }
}
