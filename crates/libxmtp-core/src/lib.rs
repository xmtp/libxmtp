pub mod client;

#[cfg(test)]
mod tests {
    use std::{rc::Rc, cell::RefCell};

    use crate::client::Client;

    #[test]
    fn it_works() {
        let result = Client::add(2, 2);
        assert_eq!(result, 4);
    }

    #[test]
    fn can_pass_persistence_methods() {
        let persisted_bytes = Rc::new(RefCell::new(Vec::new()));
        let write_to_persist_fn = {
            let persisted_bytes = persisted_bytes.clone();
            move |_key: String, value: &[u8]| -> Result<(), String> {
                let mut vec = persisted_bytes.borrow_mut();
                vec.clear();
                vec.extend_from_slice(value);
                Ok(())
            }
        };
        let read_from_persist_fn = {
            let persisted_bytes = persisted_bytes.clone();
            move |_key: String| -> Result<Vec<u8>, String> {
                let vec = persisted_bytes.borrow();
                Ok(vec.clone())
            }
        };

        let client = Client::new(
            Box::new(write_to_persist_fn),
            Box::new(read_from_persist_fn));
        client.write_to_persistence("foo".to_string(), b"bar").unwrap();
        assert_eq!(client.read_from_persistence("foo".to_string()).unwrap(), b"bar");
    }
}
