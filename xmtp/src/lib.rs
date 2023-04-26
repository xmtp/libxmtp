pub mod builder;
pub mod client;
pub mod persistence;
pub mod vmac;
pub mod vmac_traits;

pub use builder::ClientBuilder;
pub use client::Client;

#[cfg(test)]
mod tests {
    use crate::{
        vmac::{generate_outbound_session, generate_test_contact_bundle},
        builder::ClientBuilder, 
    };

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

    #[test]
    fn test_can_generate_test_contact_bundle_and_session() {
        let bundle = generate_test_contact_bundle();
        assert!(bundle.identity_key.is_some());
        assert!(bundle.prekey.is_some());

        // Generate an outbound session (Olm Prekey Message) given a VmacContactBundle
        let session = generate_outbound_session(bundle);
        assert!(!session.is_empty());
    }
}
