use std::collections::HashMap;

use openmls::prelude::MlsMessageOut;
use openmls::prelude::tls_codec::Serialize;
use openmls::test_utils::frankenstein::FrankenFramedContentBody;
use openmls::test_utils::frankenstein::FrankenMlsMessage;
use openmls::test_utils::frankenstein::FrankenMlsMessageBody;
use openmls::test_utils::frankenstein::FrankenPublicMessage;
use proptest::collection;
use proptest::prelude::*;
use xmtp_common::FakeMlsApplicationMessage;
use xmtp_common::Generate;
use xmtp_common::sha256_bytes;
use xmtp_proto::types::Cursor;
use xmtp_proto::types::GlobalCursor;
use xmtp_proto::types::OriginatorId;
use xmtp_proto::types::Topic;
use xmtp_proto::xmtp::mls::api::v1::GroupMessageInput;
use xmtp_proto::xmtp::mls::api::v1::SendGroupMessagesRequest;
use xmtp_proto::xmtp::mls::api::v1::group_message_input;

use crate::protocol::CursorStore;
use crate::protocol::CursorStoreError;

#[derive(Clone)]
pub struct TestRequest {
    pub request: SendGroupMessagesRequest,
    pub dependencies: HashMap<Vec<u8>, Cursor>,
}

impl std::fmt::Debug for TestRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestRequest")
            .field("request", &self.request);
        for (hash, cursor) in self.dependencies.iter() {
            write!(f, "[{}/{}]", hex::encode(hash), cursor)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct MessageWithDependency {
    pub request: GroupMessageInput,
    pub hash: [u8; 32],
}

prop_compose! {
    fn group_message()(data in collection::vec(any::<u8>(), 32), group_id in collection::vec(any::<u8>(), 16)) -> MessageWithDependency {
        let mut msg = FrankenPublicMessage::generate();
        msg.content.group_id = group_id.into();
        msg.content.body = FrankenFramedContentBody::Application(data.into());
        let message = FakeMlsApplicationMessage {
            inner: FrankenMlsMessage {
                version: 1,
                body: FrankenMlsMessageBody::PublicMessage(msg)
            }
        };
        let out: MlsMessageOut = message.into();
        let out = out.tls_serialize_detached().unwrap();
        let hash = sha256_bytes(&out);
        let input = GroupMessageInput {
            version: Some(group_message_input::Version::V1(group_message_input::V1 {
                data: out,
                sender_hmac: vec![],
                should_push: false,
            })),
        };
        let mut buffer = [0u8; 32];
        buffer.copy_from_slice(&hash);
        MessageWithDependency {
            request: input,
            hash: buffer,
        }
    }
}

// request containing potentially many group messages
prop_compose! {
    pub fn cursor_gen()(sid in 0u64..1000, oid in 0u32..40) -> Cursor {
        Cursor::new(sid, oid)
    }
}
prop_compose! {
    pub fn group_message_request(length: usize)
        (msgs in collection::vec(group_message(), 0..length))
        (cursors in collection::hash_set(cursor_gen(), msgs.len()), msgs in Just(msgs)) -> TestRequest {
        let messages: Vec<GroupMessageInput> = msgs.iter().map(|m| m.request.clone()).collect();
        let map = msgs
            .iter()
            .map(|m| m.hash.to_vec()).zip(cursors)
            .collect::<HashMap<_, _>>();
        TestRequest {
            request: SendGroupMessagesRequest {
                messages
            },
            dependencies: map
        }
    }
}

type DataHash = Vec<u8>;
#[derive(Default, Clone)]
pub struct TestCursorStore {
    pub dependencies: HashMap<DataHash, Cursor>,
}

impl CursorStore for TestCursorStore {
    fn lowest_common_cursor(&self, _: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        unreachable!()
    }

    fn latest(&self, _: &Topic) -> Result<GlobalCursor, CursorStoreError> {
        unreachable!()
    }

    fn latest_per_originator(
        &self,
        _: &Topic,
        _: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        unreachable!()
    }

    fn latest_for_topics(
        &self,
        _: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        unreachable!()
    }

    fn lcc_maybe_missing(&self, _: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        unreachable!()
    }

    fn find_message_dependencies(
        &self,
        _hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        Ok(self.dependencies.clone())
    }

    fn ice(
        &self,
        _orphans: Vec<xmtp_proto::types::OrphanedEnvelope>,
    ) -> Result<(), CursorStoreError> {
        unreachable!()
    }

    fn resolve_children(
        &self,
        _cursors: &[Cursor],
    ) -> Result<Vec<xmtp_proto::types::OrphanedEnvelope>, CursorStoreError> {
        unreachable!()
    }

    fn set_cutover_ns(&self, _cutover_ns: i64) -> Result<(), CursorStoreError> {
        Ok(())
    }

    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(i64::MAX)
    }

    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(0)
    }

    fn set_last_checked_ns(&self, _last_checked_ns: i64) -> Result<(), CursorStoreError> {
        Ok(())
    }

    fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        Ok(false)
    }

    fn set_has_migrated(&self, _has_migrated: bool) -> Result<(), CursorStoreError> {
        Ok(())
    }
}
