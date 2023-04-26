pub mod client;
pub mod persistence;
pub mod vmac;

#[cfg(test)]
mod tests {
    use crate::{
        client::Client,
        persistence::InMemoryPersistence,
        vmac::{generate_outbound_session, generate_test_contact_bundle},
    };

    #[test]
    fn can_pass_persistence_methods() {
        let mut client = Client::new(InMemoryPersistence::new());
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
