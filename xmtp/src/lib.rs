pub mod account;
pub mod builder;
pub mod client;
pub mod persistence;
pub mod vmac_protos;

pub use builder::ClientBuilder;
pub use client::Client;

#[cfg(test)]
mod tests {
    use crate::builder::ClientBuilder;

    #[test]
    fn can_pass_persistence_methods() {
        let mut client = ClientBuilder::new_test().build();
        assert_eq!(
            client.read_from_persistence("foo".to_string()).unwrap(),
            None
        );
        client
            .write_to_persistence("foo".to_string(), b"bar")
            .unwrap();
        assert_eq!(
            client.read_from_persistence("foo".to_string()).unwrap(),
            Some(b"bar".to_vec())
        );
    }
}
