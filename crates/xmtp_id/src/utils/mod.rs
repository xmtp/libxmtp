#[cfg(any(test, feature = "test-utils"))]
pub mod test;

#[cfg(any(test, feature = "test-utils"))]
pub mod passkey;
#[cfg(any(test, feature = "test-utils"))]
pub fn generate_inbox_id_credential() -> (String, xmtp_cryptography::XmtpInstallationCredential) {
    use crate::InboxOwner;
    let signing_key = xmtp_cryptography::XmtpInstallationCredential::new();
    let wallet = xmtp_cryptography::utils::generate_local_wallet();
    let inbox_id = wallet
        .get_identifier()
        .expect("generated wallet has an identifier")
        .inbox_id(0)
        .expect("generated identifier has an inbox");
    (inbox_id, signing_key)
}
