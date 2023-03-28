// Simple helper trait that returns a signature to be used for verification
pub trait SignatureVerifiable<T> {
    fn get_signature(&self) -> Option<T>;
}

// For a given signature type, this trait abstracts the verification process
pub trait SignatureVerifier<T>
where
    T: SignatureVerifiable<T>,
{
    fn verify_signature(&self, predigest_message: &[u8], signature: &T) -> Result<(), String>;
}
