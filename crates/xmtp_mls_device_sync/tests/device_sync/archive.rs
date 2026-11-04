#![allow(unused)]
use diesel::prelude::*;
use futures::AsyncReadExt;
use futures::StreamExt;
use futures::io::{BufReader, Cursor};
use std::{path::Path, sync::Arc};
use xmtp_archive::exporter;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::{ConnectionExt, group::GroupMembershipState};
use xmtp_db::{TransactionOutcome::Continue, prelude::*};
use xmtp_db::{
    consent_record::StoredConsentRecord,
    group::StoredGroup,
    group_message::StoredGroupMessage,
    schema::{consent_records, group_messages, groups},
};
use xmtp_events::{ArchiveRestored, ClientEvent};
use xmtp_mls::prelude::*;
use xmtp_mls::tester;
use xmtp_mls::utils::{LocalTester, Tester};
use xmtp_mls::worker::device_sync::{ArchiveOptions, BackupElementSelection};
use xmtp_mls::worker::device_sync::{
    DeviceSyncError,
    archive::*,
    test_support::{collect_self_deletions, insert_elements, insert_importer},
};
use xmtp_mls::{builder::ClientBuilder, utils::test::wait_for_min_intents};
use xmtp_mls_engine::groups::MlsGroup;
use xmtp_mls_engine::groups::send_message_opts::SendMessageOpts;
use xmtp_mls_group_state::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_mutable_metadata::MessageDisappearingSettings,
};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::device_sync::{BackupElement, backup_element::Element};

mod deletion_tests;

mod pre8_tests;

mod restored_tests;

#[cfg(not(target_arch = "wasm32"))]
mod migration_tests;

mod backup_file_tests;
mod dm_tests;
mod export_tests;
mod import_tests;
