type WriteToPersistenceFn = Box<dyn Fn(String, &[u8]) -> Result<(), String>>;
type ReadFromPersistenceFn = Box<dyn Fn(String) -> Result<Vec<u8>, String>>;

pub struct Client {
    write_to_persistence_fn: WriteToPersistenceFn,
    read_from_persistence_fn: ReadFromPersistenceFn,
}

impl Client {
    pub fn new(
        write_to_persistence_fn: WriteToPersistenceFn,
        read_from_persistence_fn: ReadFromPersistenceFn,
    ) -> Client {
        Client {
            write_to_persistence_fn,
            read_from_persistence_fn,
        }
    }

    pub fn add(left: usize, right: usize) -> usize {
        left + right
    }

    pub fn write_to_persistence(&self, s: String, b: &[u8]) -> Result<(), String> {
        (self.write_to_persistence_fn)(s, b)
    }

    pub fn read_from_persistence(&self, s: String) -> Result<Vec<u8>, String> {
        (self.read_from_persistence_fn)(s)
    }
}