use crate::xmtp::mls::message_contents::GroupUpdated;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

macro_rules! hash_arm {
    ($self:ident, $update:ident, $field:ident) => {
        if !$update.$field.is_empty() {
            let mut hasher = FxHasher::default();
            $update.$field.hash(&mut hasher);
            $self.$field = Some(hasher.finish());
        }
    };
}

macro_rules! match_arm {
    ($self:ident, $other:ident, $field:ident) => {
        match $self.$field.is_some() {
            true => $self.$field == $other.$field,
            false => true,
        }
    };
}

#[derive(Default)]
pub struct GroupUpdateDeduper {
    added_inboxes: Option<u64>,
    removed_inboxes: Option<u64>,
    metadata_field_changes: Option<u64>,
    left_inboxes: Option<u64>,
    added_admin_inboxes: Option<u64>,
    removed_admin_inboxes: Option<u64>,
    added_super_admin_inboxes: Option<u64>,
    removed_super_admin_inboxes: Option<u64>,
}

impl GroupUpdateDeduper {
    pub fn consume(&mut self, update: &GroupUpdated) {
        hash_arm!(self, update, added_inboxes);
        hash_arm!(self, update, removed_inboxes);
        hash_arm!(self, update, metadata_field_changes);
        hash_arm!(self, update, left_inboxes);
        hash_arm!(self, update, added_admin_inboxes);
        hash_arm!(self, update, removed_admin_inboxes);
        hash_arm!(self, update, added_super_admin_inboxes);
        hash_arm!(self, update, removed_super_admin_inboxes);
    }

    pub fn is_dupe(&self, update: &GroupUpdated) -> bool {
        let mut hash = Self::default();
        hash.consume(update);

        match_arm!(hash, self, added_inboxes)
            && match_arm!(hash, self, removed_inboxes)
            && match_arm!(hash, self, metadata_field_changes)
            && match_arm!(hash, self, left_inboxes)
            && match_arm!(hash, self, added_admin_inboxes)
            && match_arm!(hash, self, removed_admin_inboxes)
            && match_arm!(hash, self, added_super_admin_inboxes)
            && match_arm!(hash, self, removed_super_admin_inboxes)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        impls::update_dedupe::GroupUpdateDeduper,
        xmtp::mls::message_contents::{
            GroupUpdated,
            group_updated::{Inbox, MetadataFieldChange},
        },
    };

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_dedupe() {
        let add_update = GroupUpdated {
            added_inboxes: vec![Inbox {
                inbox_id: "123".to_string(),
            }],
            ..Default::default()
        };

        let mut deduper = GroupUpdateDeduper::default();
        deduper.consume(&add_update);

        // An update should be a duplicate of itself.
        assert!(deduper.is_dupe(&add_update));

        // Change the added_inboxes, this should now be unique.
        let mut another_add_update = add_update.clone();
        another_add_update.added_inboxes = vec![Inbox {
            inbox_id: "234".to_string(),
        }];
        assert!(!deduper.is_dupe(&another_add_update));

        // Have the deduper consume the diff update
        deduper.consume(&another_add_update);
        // The diff update should now be a dupe.
        assert!(deduper.is_dupe(&another_add_update));

        // Now let's check with other update fields.
        let remove_update = GroupUpdated {
            removed_inboxes: vec![Inbox {
                inbox_id: "123".to_string(),
            }],
            ..Default::default()
        };
        // This should of course be unique.
        assert!(!deduper.is_dupe(&remove_update));

        // The old add should still be a dupe.
        assert!(deduper.is_dupe(&another_add_update));
        // The original update should no longer be a dupe because new
        // updates to that field have occurred since it was consumed.
        assert!(!deduper.is_dupe(&add_update));

        // Now let's check with multiple fields.
        let multi_update = GroupUpdated {
            removed_inboxes: vec![Inbox {
                inbox_id: "234".to_string(),
            }],
            metadata_field_changes: vec![MetadataFieldChange {
                field_name: "disappearing_msgs".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        // Even though this contains the same removed_inboxes as the udpate before
        // (duplicating that field), this should still be unique due to the metadata
        // updates.
        assert!(!deduper.is_dupe(&multi_update));

        // Now consume the multi_upddate and ensure that it's marked as a dupe.
        deduper.consume(&multi_update);
        assert!(deduper.is_dupe(&multi_update));
    }
}
