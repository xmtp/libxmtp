use xmtp_proto::xmtp::message_api::v1::QueryRequest;

use crate::{
    app_context::AppContext, client::ClientError, contact::Contact,
    types::networking::XmtpApiClient, utils::build_user_contact_topic,
};

pub async fn get_contacts<A: XmtpApiClient>(
    app_context: &AppContext<A>,
    wallet_address: &str,
) -> Result<Vec<Contact>, ClientError> {
    let topic = build_user_contact_topic(wallet_address.to_string());
    let response = app_context
        .api_client
        .query(QueryRequest {
            content_topics: vec![topic],
            start_time_ns: 0,
            end_time_ns: 0,
            paging_info: None,
        })
        .await
        .map_err(|e| ClientError::QueryError(format!("Could not query for contacts: {}", e)))?;

    let mut contacts = vec![];
    for envelope in response.envelopes {
        let contact_bundle = Contact::from_bytes(envelope.message, wallet_address.to_string());
        match contact_bundle {
            Ok(bundle) => {
                contacts.push(bundle);
            }
            Err(err) => {
                println!("bad contact bundle: {:?}", err);
            }
        }
    }

    Ok(contacts)
}
