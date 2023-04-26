// This trait acts as a abstraction layer to allow "SignatureVerifiers" to be used with other types of Signature-like enums one day
pub trait SignatureVerifiable<T> {
    fn get_signature(&self) -> Option<T>;
}

// For a given type that is SignatureVerifiable, implement the verification process
pub trait SignatureVerifier<T>
where
    T: SignatureVerifiable<T>,
{
    fn verify_signature(&self, predigest_message: &[u8], signature: &T) -> Result<(), String>;
}
