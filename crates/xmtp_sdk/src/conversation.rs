use std::{collections::HashMap, future::Future, sync::Arc};
use xmtp_common::StreamHandle;
use xmtp_content_types::{
    ContentCodec,
    compression::compress_if_requested,
    encoded_content_to_bytes,
    reaction::ReactionCodec,
    reply::{Reply, ReplyCodec},
};
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::prelude::QueryDms;
use xmtp_db::prelude::QueryGroup;
use xmtp_db::prelude::QueryGroupMessage;
use xmtp_mls::MlsContext;
use xmtp_mls::context::{ForegroundCall, XmtpSharedContext};
use xmtp_mls::groups::{MlsGroup, send_message_opts::SendMessageOpts};
use xmtp_mls::messages::enrichment::EnrichedStoredMessage;
use xmtp_mls::mls_common::group_mutable_metadata::MetadataField;
use xmtp_mls::mls_store::MlsStore;
use xmtp_proto::types::{ConversationType, GroupId};

use crate::{
    ConsentState, ContentTypeId, ConversationId, ConversationReader, ConversationState,
    CreateDmOptions, CreateGroupOptions, DisappearingSettings, EncodedContent, GroupState,
    GroupSyncSummary, HmacKey, InboxId, ListConversationsOptions, ListMessagesOptions, Member,
    Message, MessageId, MessageReader, NotificationOverride, PublicIdentity, Reaction, SendOptions,
    StandardContent, Timestamp, XmtpError, client::CoreClient,
};

// Keep these declarations in one module so generated binding paths stay stable.
mod calls;
use calls::deletion_group;
pub(crate) use calls::{enter_call, on_sdk_worker, on_settled_worker};
include!("conversation/collection.rs");
include!("conversation/identity.rs");
mod content;
pub(crate) use content::{lift_history_messages, query_content_types};
use content::{require_content_type, send_encoded, send_standard};
include!("conversation/common.rs");
include!("conversation/group.rs");
include!("conversation/metadata.rs");
