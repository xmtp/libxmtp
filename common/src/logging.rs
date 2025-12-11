use tracing_subscriber::EnvFilter;

/// Metadata about a log event variant, including its doc comment and required context fields.
/// This struct is used by proc macros to access event metadata at compile time.
#[derive(Debug, Clone, Copy)]
pub struct EventMetadata {
    /// The name of the enum variant
    pub name: &'static str,
    /// The doc comment describing the event
    pub doc: &'static str,
    /// The required context fields for this event
    pub context_fields: &'static [&'static str],
}

pub fn filter_directive(level: &str) -> EnvFilter {
    let filter = format!(
        "xmtp_mls={level},xmtp_id={level},\
        xmtp_api={level},xmtp_api_grpc={level},xmtp_proto={level},\
        xmtp_common={level},xmtp_api_d14n={level},\
        xmtp_content_types={level},xmtp_cryptography={level},\
        xmtp_user_preferences={level},xmtpv3={level},xmtp_db={level},\
        bindings_wasm={level},bindings_node={level}"
    );
    EnvFilter::builder().parse_lossy(filter)
}

#[xmtp_macro::build_logging_metadata]
pub enum Event {
    /// Received staged commit. Merging and clearing any pending commits.
    #[context(group_id, inbox_id, sender_inbox_id, msg_epoch, current_epoch)]
    MLSReceivedStagedCommit,

    /// Processed staged commit.
    #[context(group_id, current_epoch)]
    MLSProcessedStagedCommit,

    /// Received application message.
    #[context(group_id, current_epoch, msg_epoch, sender_inbox_id)]
    MLSReceivedApplicationMessage,

    /// Processed application message.
    #[context(group_id)]
    MLSProcessedApplicationMessage,
}

// #[macro_export]
// macro_rules! log_event {
// ($event:expr $(, $r:tt)*) => {
// let required = [("Event::MLSReceivedStagedCommit", ["group_id"])];
// let event_str = stringify!($event);
// let Some((_, required)) = required.iter().find(|(event, _, _)| event == event_str) else {
// ::core::compile_error!(concat!("Could not find event variant ", stringify!($event)));
// };
// let mut required: HashSet<&str> = required.iter().collect();
// log_event!(@proc $event; required; $(, $r)*)
// };
// (@proc $event:expr; $required:ident; $($p:tt)*) => {
// if !required.is_empty() {
// ::core::compile_error!("Missing required fields.");
// }
//
// tracing::info!(
// $event,
// $($p:tt)*
// );
// };
// (@proc $event:expr; $required:ident; $($($p:tt)? ;)* $k:ident $(, $r:tt)* ) => {
// required.remove(stringify!($k));
// log_event!(@proc $event; $required; $($p)*, $k; $(, $r:tt)*);
// };
// }
