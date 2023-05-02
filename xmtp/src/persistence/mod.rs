pub mod in_memory_persistence;

pub trait Persistence {
    type Error;
    fn write(&mut self, key: &str, value: &[u8]) -> Result<(), Self::Error>;
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error>;
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

    pub fn write(&mut self, key: &str, value: &[u8]) -> Result<(), P::Error> {
        let key = format!("{}/{}", self.namespace, key);
        self.persistence.write(&key, value)
    }

    pub fn read(&self, key: &str) -> Result<Option<Vec<u8>>, P::Error> {
        let key = format!("{}/{}", self.namespace, key);
        self.persistence.read(&key)
    }
}
