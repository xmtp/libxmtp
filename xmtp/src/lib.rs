pub mod account;
pub mod association;
pub mod builder;
pub mod client;
pub mod networking;
pub mod persistence;
mod types;
mod utils;
pub mod vmac_protos;

pub use builder::ClientBuilder;
pub use client::Client;

pub trait Signable {
    fn bytes_to_sign(&self) -> Vec<u8>;
}

#[cfg(test)]
mod tests {
    use crate::builder::ClientBuilder;

    #[test]
    fn can_pass_persistence_methods() {
        let mut client = ClientBuilder::new_test().build().unwrap();
        assert_eq!(client.read_from_persistence("foo").unwrap(), None);
        client.write_to_persistence("foo", b"bar").unwrap();
        assert_eq!(
            client.read_from_persistence("foo").unwrap(),
            Some(b"bar".to_vec())
        );
    }
}
