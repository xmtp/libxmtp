use crate::persistence::Persistence;

pub struct Client<P: Persistence> {
    persistence: P,
}

impl<P: Persistence> Client<P> {
    pub fn new(
        persistence: P,
    ) -> Client<P> {
        let a = 1;
        Client {
            persistence,
        }
    }

    pub fn write_to_persistence(&mut self, s: String, b: &[u8]) -> Result<(), String> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: String) -> Result<Option<Vec<u8>>, String> {
        self.persistence.read(s)
    }
}
