pub mod client;
pub mod persistence;

#[cfg(test)]
mod tests {
    use crate::{client::Client, persistence::InMemoryPersistence};

    #[test]
    fn can_pass_persistence_methods() {
        let mut client = Client::new(Box::new(InMemoryPersistence::new()));
        assert_eq!(client.read_from_persistence("foo".to_string()).unwrap(), None);
        client.write_to_persistence("foo".to_string(), b"bar").unwrap();
        assert_eq!(client.read_from_persistence("foo".to_string()).unwrap(), Some(b"bar".to_vec()));
    }
}
