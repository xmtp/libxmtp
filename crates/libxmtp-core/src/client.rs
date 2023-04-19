pub struct Client {
    write_to_persistence_fn: Box<dyn FnMut(String, &[u8]) -> Result<(), String>>,
    read_from_persistence_fn: Box<dyn FnMut(String) -> Result<Vec<u8>, String>>,
}

impl Client {
    pub fn new(
        write_to_persistence_fn: Box<dyn FnMut(String, &[u8]) -> Result<(), String>>,
        read_from_persistence_fn: Box<dyn FnMut(String) -> Result<Vec<u8>, String>>,
    ) -> Client {
        Client {
            write_to_persistence_fn,
            read_from_persistence_fn,
        }
    }

    pub fn add(left: usize, right: usize) -> usize {
        left + right
    }

    pub fn write_to_persistence(&mut self, s: String, b: &[u8]) -> Result<(), String> {
        (self.write_to_persistence_fn)(s, b)
    }

    pub fn read_from_persistence(&mut self, s: String) -> Result<Vec<u8>, String> {
        (self.read_from_persistence_fn)(s)
    }
}