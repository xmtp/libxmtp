use openmls::{
    group::{MlsGroupJoinConfig, ProcessedWelcome, WireFormatPolicy},
    prelude::{BasicCredential, MlsMessageBodyIn, MlsMessageIn, Welcome},
};
use tls_codec::Deserialize;

use crate::{
    client::ClientError, configuration::MAX_PAST_EPOCHS, groups::GroupError, hpke::decrypt_welcome,
    identity::parse_credential, storage::xmtp_openmls_provider::XmtpOpenMlsProvider,
};

pub(crate) struct DecryptedWelcome {
    pub(crate) welcome: Welcome,
    pub(crate) added_by_inbox_id: String,
}

impl DecryptedWelcome {
    pub(crate) fn from_encrypted_bytes(
        provider: &XmtpOpenMlsProvider,
        hpke_public_key: &[u8],
        encrypted_welcome_bytes: &[u8],
    ) -> Result<DecryptedWelcome, GroupError> {
        tracing::info!("Trying to decrypt welcome");
        let welcome_bytes = decrypt_welcome(provider, hpke_public_key, encrypted_welcome_bytes)?;

        let welcome = deserialize_welcome(&welcome_bytes)?;

        let join_config = build_group_join_config();

        let processed_welcome =
            ProcessedWelcome::new_from_welcome(provider, &join_config, welcome.clone())?;
        let psks = processed_welcome.psks();
        if !psks.is_empty() {
            tracing::error!("No PSK support for welcome");
            return Err(GroupError::NoPSKSupport);
        }
        let staged_welcome = processed_welcome.into_staged_welcome(provider, None)?;

        let added_by_node = staged_welcome.welcome_sender()?;

        let added_by_credential = BasicCredential::try_from(added_by_node.credential().clone())?;
        let added_by_inbox_id = parse_credential(added_by_credential.identity())?;

        Ok(DecryptedWelcome {
            welcome,
            added_by_inbox_id,
        })
    }
}

pub(crate) fn build_group_join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(MAX_PAST_EPOCHS)
        .use_ratchet_tree_extension(true)
        .build()
}

fn deserialize_welcome(welcome_bytes: &Vec<u8>) -> Result<Welcome, ClientError> {
    // let welcome_proto = WelcomeMessageProto::decode(&mut welcome_bytes.as_slice())?;
    let welcome = MlsMessageIn::tls_deserialize(&mut welcome_bytes.as_slice())?;
    match welcome.extract() {
        MlsMessageBodyIn::Welcome(welcome) => Ok(welcome),
        _ => Err(ClientError::Generic(
            "unexpected message type in welcome".to_string(),
        )),
    }
}
