use crate::{
    client::{Client, ClientError},
    context::XmtpSharedContext,
    groups::validated_commit::extract_group_membership,
    identity_updates::IdentityUpdates,
};
use std::collections::{HashMap, HashSet};
use xmtp_db::{
    StorageError,
    consent_record::{ConsentState, ConsentType},
    group::{ConversationType, GroupQueryArgs, StoredGroup},
    group_message::SortDirection,
    prelude::*,
};
use xmtp_id::{
    InboxId,
    associations::{AssociationState, Identifier},
};

/// Represents a contact aggregated across all conversations
#[derive(Debug, Clone)]
pub struct Contact {
    /// The unique inbox identifier for this contact
    pub inbox_id: InboxId,
    /// List of account addresses/identifiers associated with this contact's inbox
    pub account_identifiers: Vec<Identifier>,
    /// List of installation IDs registered to this contact's inbox
    pub installation_ids: Vec<Vec<u8>>,
    /// List of conversation IDs (groups and DMs) this contact is a member of
    pub conversation_ids: Vec<Vec<u8>>,
    /// The consent state for this contact (Allowed, Denied, or Unknown)
    pub consent_state: ConsentState,
}

/// Sort options for contacts
#[derive(Debug, Clone, Copy, Default)]
pub enum ContactsSortBy {
    /// Sort by inbox ID alphabetically
    #[default]
    InboxId,
    /// Sort by the first account address alphabetically
    Address,
}

/// Query arguments for filtering contacts
#[derive(Debug, Default, Clone)]
pub struct ContactsQueryArgs {
    /// Only include contacts from these specific group IDs
    pub include_group_ids: Option<Vec<Vec<u8>>>,
    /// Exclude contacts from these specific group IDs
    pub exclude_group_ids: Option<Vec<Vec<u8>>>,
    /// Filter by consent states
    pub consent_states: Option<Vec<ConsentState>>,
    /// Filter by conversation type
    pub conversation_type: Option<ConversationType>,
    /// Only include contacts from groups created after this timestamp
    pub created_after_ns: Option<i64>,
    /// Only include contacts from groups created before this timestamp
    pub created_before_ns: Option<i64>,
    /// Maximum number of contacts to return
    pub limit: Option<usize>,
    /// Number of contacts to skip before returning results
    pub offset: Option<usize>,
    /// Field to sort contacts by
    pub sort_by: Option<ContactsSortBy>,
    /// Sort direction (ascending or descending)
    pub sort_direction: Option<SortDirection>,
}

// Type aliases to reduce complexity
type GroupMembersMap = HashMap<Vec<u8>, Vec<(String, i64)>>;
type ContactMap = HashMap<String, (AssociationState, HashSet<Vec<u8>>, ConsentState)>;

/// Maximum number of groups to process per batch to prevent resource exhaustion
const MAX_GROUPS_PER_BATCH: usize = 100;

/// Extract group members from filtered groups
/// Returns a map of group_id -> Vec<(inbox_id, sequence_id)>
/// Processes groups in batches to prevent resource exhaustion
fn extract_group_members<Context>(
    context: &Context,
    filtered_groups: &[StoredGroup],
) -> Result<GroupMembersMap, ClientError>
where
    Context: XmtpSharedContext,
{
    let storage = context.mls_storage();
    let mut group_members_map: GroupMembersMap = HashMap::with_capacity(filtered_groups.len());

    // Process groups in batches to prevent resource exhaustion
    for chunk in filtered_groups.chunks(MAX_GROUPS_PER_BATCH) {
        for stored_group in chunk {
            let mls_group = crate::groups::MlsGroup::new(
                context.clone(),
                stored_group.id.clone(),
                stored_group.dm_id.clone(),
                stored_group.conversation_type,
                stored_group.created_at_ns,
            );

            // Extract group membership from MLS extensions
            let group_membership = mls_group.load_mls_group_with_lock(storage, |mls_group| {
                Ok(extract_group_membership(mls_group.extensions())?)
            })?;

            // Filter out sequence_id == 0 (uninitialized/invalid member states)
            let member_requests: Vec<(String, i64)> = group_membership
                .members
                .into_iter()
                .map(|(inbox_id, sequence_id)| (inbox_id, sequence_id as i64))
                .filter(|(_, sequence_id)| *sequence_id != 0)
                .collect();

            group_members_map.insert(stored_group.id.clone(), member_requests);
        }
    }

    Ok(group_members_map)
}

/// Batch read and resolve association states
async fn resolve_association_states<Context>(
    context: &Context,
    member_requests: &[(String, i64)],
) -> Result<HashMap<String, AssociationState>, ClientError>
where
    Context: XmtpSharedContext,
{
    let db = context.db();

    let association_states_vec = db.batch_read_from_cache(member_requests.to_vec())?;
    let mut association_states: Vec<AssociationState> = association_states_vec
        .into_iter()
        .map(|a| a.try_into())
        .collect::<Result<_, _>>()
        .map_err(StorageError::from)?;

    // Handle missing association states using HashSet for O(1) lookups
    if association_states.len() != member_requests.len() {
        let found_ids: HashSet<&str> = association_states
            .iter()
            .map(|state| state.inbox_id())
            .collect();

        let missing_requests: Vec<_> = member_requests
            .iter()
            .filter(|(id, _)| !found_ids.contains(id.as_str()))
            .map(|(id, sequence)| (id.as_str(), Some(*sequence)))
            .collect();

        if !missing_requests.is_empty() {
            let identity_updates = IdentityUpdates::new(context);
            let mut new_states = identity_updates
                .batch_get_association_state(&db, &missing_requests)
                .await?;
            association_states.append(&mut new_states);
        }
    }

    Ok(association_states
        .into_iter()
        .map(|state| (state.inbox_id().to_string(), state))
        .collect())
}

/// Build contact map from group members, association states, and consent records
fn build_contact_map<Context>(
    context: &Context,
    group_members_map: &GroupMembersMap,
    mut association_map: HashMap<String, AssociationState>,
) -> Result<ContactMap, ClientError>
where
    Context: XmtpSharedContext,
{
    let db = context.db();

    // Batch fetch all consent records for inbox_ids
    let inbox_ids: Vec<String> = association_map.keys().cloned().collect();
    let consent_records = db.get_consent_records_batch(&inbox_ids, ConsentType::InboxId)?;
    let consent_map: HashMap<String, ConsentState> = consent_records
        .into_iter()
        .map(|r| (r.entity, r.state))
        .collect();

    // Build contact map by iterating over group members
    let mut contact_map: ContactMap = HashMap::new();

    for (group_id, member_requests) in group_members_map {
        for (inbox_id, _) in member_requests {
            if let Some((_, conversation_ids, _)) = contact_map.get_mut(inbox_id) {
                // Contact already exists (appears in multiple groups), just add this group_id
                conversation_ids.insert(group_id.clone());
            } else if let Some(association_state) = association_map.remove(inbox_id) {
                // First time seeing this contact - remove from association_map to take ownership
                let consent_state = consent_map
                    .get(inbox_id)
                    .copied()
                    .unwrap_or(ConsentState::Unknown);

                contact_map.insert(
                    inbox_id.clone(),
                    (
                        association_state,
                        HashSet::from([group_id.clone()]),
                        consent_state,
                    ),
                );
            }
        }
    }

    Ok(contact_map)
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    /// List members from conversations with optional filters
    ///
    /// Aggregates all unique members from conversations (groups and DMs).
    /// Each member includes their inbox_id, account identifiers, the conversations they're in,
    /// and consent state.
    ///
    /// Filters:
    /// - include_group_ids: only include members from these specific group IDs
    /// - exclude_group_ids: exclude members from these specific group IDs
    /// - consent_states: only return contacts with the given consent states
    /// - conversation_type: filter by conversation type (Group or Dm)
    /// - created_after_ns/created_before_ns: filter by group creation time
    pub async fn list_members(
        &self,
        args: ContactsQueryArgs,
    ) -> Result<Vec<Contact>, ClientError> {
        let ContactsQueryArgs {
            include_group_ids,
            exclude_group_ids,
            consent_states,
            conversation_type,
            created_after_ns,
            created_before_ns,
            limit,
            offset,
            sort_by,
            sort_direction,
        } = args;

        // Build group query args from contact query args
        let group_args = GroupQueryArgs {
            allowed_states: None,
            created_after_ns,
            created_before_ns,
            last_activity_after_ns: None,
            last_activity_before_ns: None,
            limit: None, // We'll apply limit after deduplication
            conversation_type,
            consent_states: None, // We'll filter by contact consent state later
            include_sync_groups: false,
            include_duplicate_dms: false,
            should_publish_commit_log: None,
            order_by: None,
        };

        // Get all groups matching the filter criteria and apply allow/deny lists
        let filtered_groups: Vec<StoredGroup> = self
            .context
            .db()
            .find_groups(group_args)?
            .into_iter()
            .filter(|g| {
                include_group_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&g.id))
            })
            .filter(|g| {
                exclude_group_ids
                    .as_ref()
                    .is_none_or(|ids| !ids.contains(&g.id))
            })
            .collect();

        if filtered_groups.is_empty() {
            return Ok(vec![]);
        }

        // Extract all member inbox_ids from all groups
        let group_members_map = extract_group_members(&self.context, &filtered_groups)?;

        // Deduplicate members, keeping the highest sequence_id for each inbox_id
        // (same member can appear in multiple groups with different sequence_ids)
        let mut inbox_to_max_seq: HashMap<String, i64> = HashMap::new();
        for (inbox_id, sequence_id) in group_members_map.values().flatten() {
            inbox_to_max_seq
                .entry(inbox_id.clone())
                .and_modify(|existing| *existing = (*existing).max(*sequence_id))
                .or_insert(*sequence_id);
        }

        if inbox_to_max_seq.is_empty() {
            return Ok(vec![]);
        }

        let unique_requests: Vec<(String, i64)> = inbox_to_max_seq.into_iter().collect();

        // Batch resolve association states for all unique members
        let association_map = resolve_association_states(&self.context, &unique_requests).await?;

        // Build contact map from group members, association states, and consent records
        let contact_map = build_contact_map(&self.context, &group_members_map, association_map)?;

        // remove self and filter by consent states
        let mut contacts: Vec<Contact> = contact_map
            .into_iter()
            .filter(|(inbox_id, _)| inbox_id != self.inbox_id())
            .filter(|(_, (_, _, consent_state))| {
                consent_states
                    .as_ref()
                    .is_none_or(|states| states.contains(consent_state))
            })
            .map(
                |(inbox_id, (association_state, conversation_ids, consent_state))| Contact {
                    inbox_id,
                    account_identifiers: association_state.identifiers(),
                    installation_ids: association_state.installation_ids(),
                    conversation_ids: conversation_ids.into_iter().collect(),
                    consent_state,
                },
            )
            .collect();

        // Apply sorting
        let sort_by = sort_by.unwrap_or_default();
        let sort_direction = sort_direction.unwrap_or_default();

        match sort_by {
            ContactsSortBy::InboxId => match sort_direction {
                SortDirection::Ascending => contacts.sort_by(|a, b| a.inbox_id.cmp(&b.inbox_id)),
                SortDirection::Descending => contacts.sort_by(|a, b| b.inbox_id.cmp(&a.inbox_id)),
            },
            ContactsSortBy::Address => {
                // Sort by first account identifier's string representation
                let get_first_address = |c: &Contact| -> String {
                    c.account_identifiers
                        .first()
                        .map(|id| id.to_string())
                        .unwrap_or_default()
                };
                match sort_direction {
                    SortDirection::Ascending => contacts.sort_by_key(|c| get_first_address(c)),
                    SortDirection::Descending => {
                        contacts.sort_by_key(|c| std::cmp::Reverse(get_first_address(c)))
                    }
                }
            }
        }

        // Apply offset and limit
        let offset = offset.unwrap_or(0);
        let contacts: Vec<Contact> = contacts.into_iter().skip(offset).collect();

        let contacts: Vec<Contact> = if let Some(limit) = limit {
            contacts.into_iter().take(limit).collect()
        } else {
            contacts
        };

        Ok(contacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use std::time::Duration;
    use xmtp_common::time::now_ns;
    use xmtp_db::consent_record::{ConsentType, StoredConsentRecord};

    #[xmtp_common::test]
    async fn test_list_members() {
        use xmtp_db::group::ConversationType;

        // Create 10 clients
        tester!(alice);
        tester!(bob);
        tester!(charlie);
        tester!(diana);
        tester!(eve);
        tester!(frank);
        tester!(grace);
        tester!(henry);
        tester!(iris);
        tester!(jack);

        // Create Group 1: Alice, Bob, Charlie, Diana
        let group1 = alice.create_group(None, None).unwrap();
        group1
            .add_members_by_inbox_id(&[bob.inbox_id(), charlie.inbox_id(), diana.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();
        diana.sync_welcomes().await.unwrap();

        // Delay between groups to ensure distinct timestamps for time-based filtering tests
        xmtp_common::time::sleep(Duration::from_millis(100)).await;
        let mid_time = now_ns();
        xmtp_common::time::sleep(Duration::from_millis(100)).await;

        // Create Group 2: Alice, Eve, Frank, Grace
        let group2 = alice.create_group(None, None).unwrap();
        group2
            .add_members_by_inbox_id(&[eve.inbox_id(), frank.inbox_id(), grace.inbox_id()])
            .await
            .unwrap();

        eve.sync_welcomes().await.unwrap();
        frank.sync_welcomes().await.unwrap();
        grace.sync_welcomes().await.unwrap();

        // Create Group 3: Alice, Henry, Iris
        let group3 = alice.create_group(None, None).unwrap();
        group3
            .add_members_by_inbox_id(&[henry.inbox_id(), iris.inbox_id()])
            .await
            .unwrap();

        henry.sync_welcomes().await.unwrap();
        iris.sync_welcomes().await.unwrap();

        // Create DM 1: Alice <-> Bob
        let dm1 = alice
            .find_or_create_dm_by_inbox_id(bob.inbox_id(), None)
            .await
            .unwrap();
        bob.sync_welcomes().await.unwrap();

        // Create DM 2: Alice <-> Jack
        let dm2 = alice
            .find_or_create_dm_by_inbox_id(jack.inbox_id(), None)
            .await
            .unwrap();
        jack.sync_welcomes().await.unwrap();

        // Create DM 3: Alice <-> Charlie
        let dm3 = alice
            .find_or_create_dm_by_inbox_id(charlie.inbox_id(), None)
            .await
            .unwrap();
        charlie.sync_welcomes().await.unwrap();

        // Set up various consent states
        // Bob: Allowed
        alice
            .set_consent_states(&[StoredConsentRecord::new(
                ConsentType::InboxId,
                ConsentState::Allowed,
                bob.inbox_id().to_string(),
            )])
            .await
            .unwrap();

        // Charlie: Denied
        alice
            .set_consent_states(&[StoredConsentRecord::new(
                ConsentType::InboxId,
                ConsentState::Denied,
                charlie.inbox_id().to_string(),
            )])
            .await
            .unwrap();

        // Eve: Allowed
        alice
            .set_consent_states(&[StoredConsentRecord::new(
                ConsentType::InboxId,
                ConsentState::Allowed,
                eve.inbox_id().to_string(),
            )])
            .await
            .unwrap();

        // Diana, Frank, Grace, Henry, Iris, Jack: Unknown (no consent record)

        // Test 1: List all contacts (should have 9 contacts: all except Alice herself)
        let all_contacts = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();

        assert_eq!(all_contacts.len(), 9, "Alice should have 9 contacts");

        // Verify Alice is not in her own contacts
        assert!(
            !all_contacts.iter().any(|c| c.inbox_id == alice.inbox_id()),
            "Alice should not be in her own contacts"
        );

        // Test 2: Filter by include_group_ids (only Group 1)
        let group1_contacts = alice
            .list_members(ContactsQueryArgs {
                include_group_ids: Some(vec![group1.group_id.clone()]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(group1_contacts.len(), 3, "Group 1 should have 3 contacts");
        let group1_inbox_ids: Vec<String> =
            group1_contacts.iter().map(|c| c.inbox_id.clone()).collect();
        assert!(group1_inbox_ids.contains(&bob.inbox_id().to_string()));
        assert!(group1_inbox_ids.contains(&charlie.inbox_id().to_string()));
        assert!(group1_inbox_ids.contains(&diana.inbox_id().to_string()));

        // Test 3: Filter by exclude_group_ids (exclude Group 2)
        let without_group2 = alice
            .list_members(ContactsQueryArgs {
                exclude_group_ids: Some(vec![group2.group_id.clone()]),
                ..Default::default()
            })
            .await
            .unwrap();

        // Should exclude Eve, Frank, Grace (but they might still appear from other groups)
        // Since Eve, Frank, Grace are only in Group 2, they should be completely excluded
        assert_eq!(
            without_group2.len(),
            6,
            "Should exclude Group 2 only members"
        );
        let without_group2_ids: Vec<String> =
            without_group2.iter().map(|c| c.inbox_id.clone()).collect();
        assert!(!without_group2_ids.contains(&eve.inbox_id().to_string()));
        assert!(!without_group2_ids.contains(&frank.inbox_id().to_string()));
        assert!(!without_group2_ids.contains(&grace.inbox_id().to_string()));

        // Test 4: Filter by consent_states (Allowed only)
        let allowed_contacts = alice
            .list_members(ContactsQueryArgs {
                consent_states: Some(vec![ConsentState::Allowed]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(allowed_contacts.len(), 2, "Should have 2 allowed contacts");
        let allowed_ids: Vec<String> = allowed_contacts
            .iter()
            .map(|c| c.inbox_id.clone())
            .collect();
        assert!(allowed_ids.contains(&bob.inbox_id().to_string()));
        assert!(allowed_ids.contains(&eve.inbox_id().to_string()));

        // Test 5: Filter by consent_states (Denied only)
        let denied_contacts = alice
            .list_members(ContactsQueryArgs {
                consent_states: Some(vec![ConsentState::Denied]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(denied_contacts.len(), 1, "Should have 1 denied contact");
        assert_eq!(denied_contacts[0].inbox_id, charlie.inbox_id());

        // Test 6: Filter by consent_states (Unknown only)
        let unknown_contacts = alice
            .list_members(ContactsQueryArgs {
                consent_states: Some(vec![ConsentState::Unknown]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(unknown_contacts.len(), 6, "Should have 6 unknown contacts");
        let unknown_ids: Vec<String> = unknown_contacts
            .iter()
            .map(|c| c.inbox_id.clone())
            .collect();
        assert!(unknown_ids.contains(&diana.inbox_id().to_string()));
        assert!(unknown_ids.contains(&frank.inbox_id().to_string()));
        assert!(unknown_ids.contains(&grace.inbox_id().to_string()));
        assert!(unknown_ids.contains(&henry.inbox_id().to_string()));
        assert!(unknown_ids.contains(&iris.inbox_id().to_string()));
        assert!(unknown_ids.contains(&jack.inbox_id().to_string()));

        // Test 7: Filter by consent_states (Allowed + Unknown)
        let allowed_or_unknown = alice
            .list_members(ContactsQueryArgs {
                consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(
            allowed_or_unknown.len(),
            8,
            "Should have 8 allowed or unknown contacts"
        );

        // Test 8: Filter by conversation_type (Groups only)
        let group_contacts = alice
            .list_members(ContactsQueryArgs {
                conversation_type: Some(ConversationType::Group),
                ..Default::default()
            })
            .await
            .unwrap();

        // All contacts in groups: Bob, Charlie, Diana (Group1), Eve, Frank, Grace (Group2), Henry, Iris (Group3)
        assert_eq!(
            group_contacts.len(),
            8,
            "Should have 8 contacts from groups"
        );

        // Test 9: Filter by conversation_type (DMs only)
        let dm_contacts = alice
            .list_members(ContactsQueryArgs {
                conversation_type: Some(ConversationType::Dm),
                ..Default::default()
            })
            .await
            .unwrap();

        // Contacts in DMs: Bob, Jack, Charlie
        assert_eq!(dm_contacts.len(), 3, "Should have 3 contacts from DMs");
        let dm_ids: Vec<String> = dm_contacts.iter().map(|c| c.inbox_id.clone()).collect();
        assert!(dm_ids.contains(&bob.inbox_id().to_string()));
        assert!(dm_ids.contains(&charlie.inbox_id().to_string()));
        assert!(dm_ids.contains(&jack.inbox_id().to_string()));

        // Test 10: Filter by created_after_ns
        let recent_contacts = alice
            .list_members(ContactsQueryArgs {
                created_after_ns: Some(mid_time),
                ..Default::default()
            })
            .await
            .unwrap();

        // Should include contacts from Group 2, Group 3, and all DMs created after mid_time
        assert!(
            recent_contacts.len() >= 5,
            "Should have at least 5 recent contacts"
        );

        // Test 11: Filter by created_before_ns
        let early_contacts = alice
            .list_members(ContactsQueryArgs {
                created_before_ns: Some(mid_time),
                ..Default::default()
            })
            .await
            .unwrap();

        // Should include contacts from Group 1 created before mid_time
        assert!(
            early_contacts.len() >= 3,
            "Should have at least 3 early contacts"
        );

        // Test 12: Verify conversation_ids are populated correctly
        let bob_contact = all_contacts
            .iter()
            .find(|c| c.inbox_id == bob.inbox_id())
            .unwrap();
        assert_eq!(
            bob_contact.conversation_ids.len(),
            2,
            "Bob should be in 2 conversations (Group 1 + DM)"
        );

        let jack_contact = all_contacts
            .iter()
            .find(|c| c.inbox_id == jack.inbox_id())
            .unwrap();
        assert_eq!(
            jack_contact.conversation_ids.len(),
            1,
            "Jack should be in 1 conversation (DM only)"
        );

        // Test 13: Combined filters - include_group_ids + consent_states
        let group1_allowed = alice
            .list_members(ContactsQueryArgs {
                include_group_ids: Some(vec![group1.group_id.clone()]),
                consent_states: Some(vec![ConsentState::Allowed]),
                ..Default::default()
            })
            .await
            .unwrap();

        // Bob is in Group 1 and is Allowed
        assert_eq!(group1_allowed.len(), 1);
        assert_eq!(group1_allowed[0].inbox_id, bob.inbox_id());

        // Test 14: Combined filters - conversation_type + consent_states
        let dm_unknown = alice
            .list_members(ContactsQueryArgs {
                conversation_type: Some(ConversationType::Dm),
                consent_states: Some(vec![ConsentState::Unknown]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(dm_unknown.len(), 1);
        // Should be Jack (the only Unknown contact in DMs only)
        assert_eq!(dm_unknown[0].inbox_id, jack.inbox_id());

        // Test 15: Verify account_identifiers and installation_ids are populated
        assert!(
            !bob_contact.account_identifiers.is_empty(),
            "Bob should have account identifiers"
        );
        assert!(
            !bob_contact.installation_ids.is_empty(),
            "Bob should have installation IDs"
        );

        // Test 16: Edge case - deny all DMs
        let no_dms = alice
            .list_members(ContactsQueryArgs {
                exclude_group_ids: Some(vec![
                    dm1.group_id.clone(),
                    dm2.group_id.clone(),
                    dm3.group_id.clone(),
                ]),
                ..Default::default()
            })
            .await
            .unwrap();

        // Should still have all contacts but only from groups
        assert_eq!(no_dms.len(), 8, "Should have 8 contacts from groups only");
    }

    #[xmtp_common::test]
    async fn test_contacts_empty_groups() {
        // Test edge case: groups with no members should not cause errors
        tester!(client);

        // Create an empty group (no members added)
        let empty_group = client.create_group(None, None).unwrap();

        // List contacts - should return empty without errors
        let contacts = client
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();

        assert_eq!(
            contacts.len(),
            0,
            "Empty groups should result in no contacts"
        );

        // Filter specifically by this empty group
        let empty_group_contacts = client
            .list_members(ContactsQueryArgs {
                include_group_ids: Some(vec![empty_group.group_id.clone()]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(
            empty_group_contacts.len(),
            0,
            "Filtering by empty group should return no contacts"
        );
    }

    #[xmtp_common::test]
    async fn test_contacts_removed_members() {
        // Test edge case: groups where all members have been removed
        tester!(alice);
        tester!(bob);

        // Create a group and add Bob
        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();

        // Verify Bob is in the contacts
        let contacts_before = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();
        assert_eq!(contacts_before.len(), 1, "Should have Bob as a contact");
        assert_eq!(contacts_before[0].inbox_id, bob.inbox_id());

        // Remove Bob from the group
        group
            .remove_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();

        // Sync to ensure removal is processed
        alice.sync_welcomes().await.unwrap();

        // List contacts again - Bob should no longer appear since he's been removed
        let contacts_after = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();

        assert_eq!(
            contacts_after.len(),
            0,
            "Removed members should not appear in contacts"
        );
    }

    #[xmtp_common::test]
    async fn test_contacts_with_concurrent_modifications() {
        // Test edge case: modifications to groups during query execution
        tester!(alice);
        tester!(bob);
        tester!(charlie);

        // Create initial group with Bob
        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();
        bob.sync_welcomes().await.unwrap();

        // Get initial contacts
        let contacts1 = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();
        assert_eq!(contacts1.len(), 1);

        // Add Charlie to the group
        group
            .add_members_by_inbox_id(&[charlie.inbox_id()])
            .await
            .unwrap();
        charlie.sync_welcomes().await.unwrap();

        // Query again - should see both members
        let contacts2 = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();
        assert_eq!(contacts2.len(), 2, "Should see both Bob and Charlie");

        let inbox_ids: Vec<String> = contacts2.iter().map(|c| c.inbox_id.clone()).collect();
        assert!(inbox_ids.contains(&bob.inbox_id().to_string()));
        assert!(inbox_ids.contains(&charlie.inbox_id().to_string()));

        // Remove Bob
        group
            .remove_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();
        alice.sync_welcomes().await.unwrap();

        // Query again - should only see Charlie
        let contacts3 = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();
        assert_eq!(contacts3.len(), 1, "Should only see Charlie");
        assert_eq!(contacts3[0].inbox_id, charlie.inbox_id());
    }

    #[xmtp_common::test]
    async fn test_contacts_missing_consent_records() {
        // Test edge case: graceful handling when consent records are missing
        // This exercises the error handling path in build_contact_map
        tester!(alice);
        tester!(bob);
        tester!(charlie);

        // Create a group with multiple members, but don't set any consent records
        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bob.inbox_id(), charlie.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();

        // Query contacts - should succeed with Unknown consent state
        let contacts = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();

        assert_eq!(contacts.len(), 2, "Should have 2 contacts");

        // Verify all contacts have Unknown consent state (default when no record exists)
        for contact in &contacts {
            assert_eq!(
                contact.consent_state,
                ConsentState::Unknown,
                "Contact {} should have Unknown consent state",
                contact.inbox_id
            );
        }

        // Set consent for one contact
        alice
            .set_consent_states(&[StoredConsentRecord::new(
                ConsentType::InboxId,
                ConsentState::Allowed,
                bob.inbox_id().to_string(),
            )])
            .await
            .unwrap();

        // Query again
        let contacts_after = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();

        assert_eq!(contacts_after.len(), 2);

        // Verify Bob is Allowed, Charlie is still Unknown
        let bob_contact = contacts_after
            .iter()
            .find(|c| c.inbox_id == bob.inbox_id())
            .unwrap();
        assert_eq!(bob_contact.consent_state, ConsentState::Allowed);

        let charlie_contact = contacts_after
            .iter()
            .find(|c| c.inbox_id == charlie.inbox_id())
            .unwrap();
        assert_eq!(charlie_contact.consent_state, ConsentState::Unknown);
    }

    #[xmtp_common::test]
    async fn test_contacts_include_and_exclude_group_ids() {
        // Test using both include_group_ids and exclude_group_ids together
        tester!(alice);
        tester!(bob);
        tester!(charlie);
        tester!(diana);

        // Create 3 groups
        let group1 = alice.create_group(None, None).unwrap();
        group1
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();

        let group2 = alice.create_group(None, None).unwrap();
        group2
            .add_members_by_inbox_id(&[charlie.inbox_id()])
            .await
            .unwrap();

        let group3 = alice.create_group(None, None).unwrap();
        group3
            .add_members_by_inbox_id(&[diana.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();
        diana.sync_welcomes().await.unwrap();

        // Allow groups 1 and 2, but deny group 2 - should only get group 1 contacts
        let contacts = alice
            .list_members(ContactsQueryArgs {
                include_group_ids: Some(vec![group1.group_id.clone(), group2.group_id.clone()]),
                exclude_group_ids: Some(vec![group2.group_id.clone()]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(contacts.len(), 1, "Should only have Bob from group1");
        assert_eq!(contacts[0].inbox_id, bob.inbox_id());
    }

    #[xmtp_common::test]
    async fn test_contacts_nonexistent_group_ids() {
        // Test filtering by group IDs that don't exist
        tester!(alice);
        tester!(bob);

        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();
        bob.sync_welcomes().await.unwrap();

        // Filter by a non-existent group ID
        let contacts = alice
            .list_members(ContactsQueryArgs {
                include_group_ids: Some(vec![vec![0, 1, 2, 3]]), // fake group ID
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(contacts.len(), 0, "Should have no contacts from fake group");

        // Deny a non-existent group ID (should have no effect)
        let contacts = alice
            .list_members(ContactsQueryArgs {
                exclude_group_ids: Some(vec![vec![0, 1, 2, 3]]), // fake group ID
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(
            contacts.len(),
            1,
            "Should still have Bob since fake group denial has no effect"
        );
    }

    #[xmtp_common::test]
    async fn test_contacts_no_groups() {
        // Test when client has no groups at all
        tester!(alice);

        let contacts = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();

        assert_eq!(contacts.len(), 0, "Should have no contacts with no groups");
    }

    #[xmtp_common::test]
    async fn test_contacts_deduplication_across_groups() {
        // Test that contacts appearing in multiple groups are properly deduplicated
        // and have all their conversation_ids aggregated
        tester!(alice);
        tester!(bob);

        // Create 3 groups, all containing Bob
        let group1 = alice.create_group(None, None).unwrap();
        group1
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();

        let group2 = alice.create_group(None, None).unwrap();
        group2
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();

        let group3 = alice.create_group(None, None).unwrap();
        group3
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();

        // Query all contacts
        let contacts = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();

        // Should have exactly 1 contact (Bob), not 3
        assert_eq!(contacts.len(), 1, "Bob should appear only once");
        assert_eq!(contacts[0].inbox_id, bob.inbox_id());

        // Bob should be associated with all 3 groups
        assert_eq!(
            contacts[0].conversation_ids.len(),
            3,
            "Bob should be in 3 conversations"
        );

        // Verify all group IDs are present
        let group_ids: Vec<&Vec<u8>> = contacts[0].conversation_ids.iter().collect();
        assert!(group_ids.contains(&&group1.group_id));
        assert!(group_ids.contains(&&group2.group_id));
        assert!(group_ids.contains(&&group3.group_id));
    }

    #[xmtp_common::test]
    async fn test_contacts_pagination_limit() {
        // Test pagination with limit
        tester!(alice);
        tester!(bob);
        tester!(charlie);
        tester!(diana);

        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bob.inbox_id(), charlie.inbox_id(), diana.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();
        diana.sync_welcomes().await.unwrap();

        // Get all contacts first to verify we have 3
        let all_contacts = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();
        assert_eq!(all_contacts.len(), 3, "Should have 3 contacts");

        // Test limit of 2
        let limited_contacts = alice
            .list_members(ContactsQueryArgs {
                limit: Some(2),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            limited_contacts.len(),
            2,
            "Should have 2 contacts with limit"
        );

        // Test limit of 1
        let single_contact = alice
            .list_members(ContactsQueryArgs {
                limit: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(single_contact.len(), 1, "Should have 1 contact with limit");

        // Test limit greater than count
        let over_limit = alice
            .list_members(ContactsQueryArgs {
                limit: Some(10),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(over_limit.len(), 3, "Should have all 3 contacts");
    }

    #[xmtp_common::test]
    async fn test_contacts_pagination_offset() {
        // Test pagination with offset
        tester!(alice);
        tester!(bob);
        tester!(charlie);
        tester!(diana);

        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bob.inbox_id(), charlie.inbox_id(), diana.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();
        diana.sync_welcomes().await.unwrap();

        // Get all contacts with default sorting
        let all_contacts = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();
        assert_eq!(all_contacts.len(), 3);

        // Test offset of 1 - should skip first contact
        let offset_contacts = alice
            .list_members(ContactsQueryArgs {
                offset: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            offset_contacts.len(),
            2,
            "Should have 2 contacts with offset 1"
        );

        // Test offset of 2
        let offset_2 = alice
            .list_members(ContactsQueryArgs {
                offset: Some(2),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(offset_2.len(), 1, "Should have 1 contact with offset 2");

        // Test offset greater than count
        let over_offset = alice
            .list_members(ContactsQueryArgs {
                offset: Some(10),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            over_offset.len(),
            0,
            "Should have no contacts with large offset"
        );
    }

    #[xmtp_common::test]
    async fn test_contacts_pagination_limit_and_offset() {
        // Test pagination with both limit and offset
        tester!(alice);
        tester!(bob);
        tester!(charlie);
        tester!(diana);
        tester!(eve);

        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[
                bob.inbox_id(),
                charlie.inbox_id(),
                diana.inbox_id(),
                eve.inbox_id(),
            ])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();
        diana.sync_welcomes().await.unwrap();
        eve.sync_welcomes().await.unwrap();

        // Get all contacts sorted for comparison
        let all_sorted = alice
            .list_members(ContactsQueryArgs {
                sort_by: Some(ContactsSortBy::InboxId),
                sort_direction: Some(SortDirection::Ascending),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(all_sorted.len(), 4);

        // Get second page: offset 2, limit 2
        let page = alice
            .list_members(ContactsQueryArgs {
                limit: Some(2),
                offset: Some(2),
                sort_by: Some(ContactsSortBy::InboxId),
                sort_direction: Some(SortDirection::Ascending),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.len(), 2, "Page should have 2 contacts");

        // Verify the page contains the correct contacts (3rd and 4th from sorted list)
        assert_eq!(page[0].inbox_id, all_sorted[2].inbox_id);
        assert_eq!(page[1].inbox_id, all_sorted[3].inbox_id);
    }

    #[xmtp_common::test]
    async fn test_contacts_sorting() {
        // Test sorting by inbox_id
        tester!(alice);
        tester!(bob);
        tester!(charlie);
        tester!(diana);

        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bob.inbox_id(), charlie.inbox_id(), diana.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();
        diana.sync_welcomes().await.unwrap();

        // Get contacts sorted ascending
        let asc_contacts = alice
            .list_members(ContactsQueryArgs {
                sort_by: Some(ContactsSortBy::InboxId),
                sort_direction: Some(SortDirection::Ascending),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(asc_contacts.len(), 3);

        // Verify ascending order
        for i in 0..asc_contacts.len() - 1 {
            assert!(
                asc_contacts[i].inbox_id <= asc_contacts[i + 1].inbox_id,
                "Contacts should be sorted ascending by inbox_id"
            );
        }

        // Get contacts sorted descending
        let desc_contacts = alice
            .list_members(ContactsQueryArgs {
                sort_by: Some(ContactsSortBy::InboxId),
                sort_direction: Some(SortDirection::Descending),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(desc_contacts.len(), 3);

        // Verify descending order
        for i in 0..desc_contacts.len() - 1 {
            assert!(
                desc_contacts[i].inbox_id >= desc_contacts[i + 1].inbox_id,
                "Contacts should be sorted descending by inbox_id"
            );
        }

        // Verify ascending and descending are reverse of each other
        let asc_ids: Vec<String> = asc_contacts.iter().map(|c| c.inbox_id.clone()).collect();
        let desc_ids: Vec<String> = desc_contacts.iter().map(|c| c.inbox_id.clone()).collect();
        let reversed_asc: Vec<String> = asc_ids.iter().rev().cloned().collect();
        assert_eq!(
            desc_ids, reversed_asc,
            "Descending should be reverse of ascending"
        );
    }

    #[xmtp_common::test]
    async fn test_contacts_sorting_default() {
        // Test default sorting behavior
        tester!(alice);
        tester!(bob);
        tester!(charlie);

        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bob.inbox_id(), charlie.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();

        // Get contacts with default sorting (should be ascending by inbox_id)
        let default_contacts = alice
            .list_members(ContactsQueryArgs::default())
            .await
            .unwrap();

        // Get contacts with explicit ascending sort
        let explicit_asc = alice
            .list_members(ContactsQueryArgs {
                sort_by: Some(ContactsSortBy::InboxId),
                sort_direction: Some(SortDirection::Ascending),
                ..Default::default()
            })
            .await
            .unwrap();

        // Verify default sorting is ascending
        let default_ids: Vec<String> = default_contacts
            .iter()
            .map(|c| c.inbox_id.clone())
            .collect();
        let explicit_ids: Vec<String> = explicit_asc.iter().map(|c| c.inbox_id.clone()).collect();
        assert_eq!(
            default_ids, explicit_ids,
            "Default sort should match ascending by inbox_id"
        );
    }

    #[xmtp_common::test]
    async fn test_contacts_combined_sort_filter_paginate() {
        // Test combining sorting, filtering, and pagination
        tester!(alice);
        tester!(bob);
        tester!(charlie);
        tester!(diana);
        tester!(eve);

        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[
                bob.inbox_id(),
                charlie.inbox_id(),
                diana.inbox_id(),
                eve.inbox_id(),
            ])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();
        diana.sync_welcomes().await.unwrap();
        eve.sync_welcomes().await.unwrap();

        // Set consent for some contacts
        alice
            .set_consent_states(&[
                StoredConsentRecord::new(
                    ConsentType::InboxId,
                    ConsentState::Allowed,
                    bob.inbox_id().to_string(),
                ),
                StoredConsentRecord::new(
                    ConsentType::InboxId,
                    ConsentState::Allowed,
                    charlie.inbox_id().to_string(),
                ),
            ])
            .await
            .unwrap();

        // Query allowed contacts, sorted descending, with pagination
        let result = alice
            .list_members(ContactsQueryArgs {
                consent_states: Some(vec![ConsentState::Allowed]),
                sort_by: Some(ContactsSortBy::InboxId),
                sort_direction: Some(SortDirection::Descending),
                limit: Some(1),
                offset: Some(0),
                ..Default::default()
            })
            .await
            .unwrap();

        // Should have 1 contact (first of 2 allowed, sorted descending)
        assert_eq!(result.len(), 1);

        // Get the second allowed contact
        let second_result = alice
            .list_members(ContactsQueryArgs {
                consent_states: Some(vec![ConsentState::Allowed]),
                sort_by: Some(ContactsSortBy::InboxId),
                sort_direction: Some(SortDirection::Descending),
                limit: Some(1),
                offset: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(second_result.len(), 1);
        // The two results should be different
        assert_ne!(result[0].inbox_id, second_result[0].inbox_id);

        // Verify ordering - first result should be greater than second (descending)
        assert!(
            result[0].inbox_id > second_result[0].inbox_id,
            "First page should have greater inbox_id in descending order"
        );
    }

    #[xmtp_common::test]
    async fn test_contacts_sorting_by_address() {
        // Test sorting by address
        tester!(alice);
        tester!(bob);
        tester!(charlie);
        tester!(diana);

        let group = alice.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bob.inbox_id(), charlie.inbox_id(), diana.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();
        diana.sync_welcomes().await.unwrap();

        // Get contacts sorted by address ascending
        let asc_contacts = alice
            .list_members(ContactsQueryArgs {
                sort_by: Some(ContactsSortBy::Address),
                sort_direction: Some(SortDirection::Ascending),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(asc_contacts.len(), 3);

        // Verify each contact has account identifiers
        for contact in &asc_contacts {
            assert!(
                !contact.account_identifiers.is_empty(),
                "Each contact should have at least one account identifier"
            );
        }

        // Verify ascending order by first address
        for i in 0..asc_contacts.len() - 1 {
            let addr_a = asc_contacts[i]
                .account_identifiers
                .first()
                .map(|id| id.to_string())
                .unwrap_or_default();
            let addr_b = asc_contacts[i + 1]
                .account_identifiers
                .first()
                .map(|id| id.to_string())
                .unwrap_or_default();
            assert!(
                addr_a <= addr_b,
                "Contacts should be sorted ascending by address"
            );
        }

        // Get contacts sorted by address descending
        let desc_contacts = alice
            .list_members(ContactsQueryArgs {
                sort_by: Some(ContactsSortBy::Address),
                sort_direction: Some(SortDirection::Descending),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(desc_contacts.len(), 3);

        // Verify descending order by first address
        for i in 0..desc_contacts.len() - 1 {
            let addr_a = desc_contacts[i]
                .account_identifiers
                .first()
                .map(|id| id.to_string())
                .unwrap_or_default();
            let addr_b = desc_contacts[i + 1]
                .account_identifiers
                .first()
                .map(|id| id.to_string())
                .unwrap_or_default();
            assert!(
                addr_a >= addr_b,
                "Contacts should be sorted descending by address"
            );
        }

        // Verify ascending and descending are reverse of each other
        let asc_addrs: Vec<String> = asc_contacts
            .iter()
            .map(|c| {
                c.account_identifiers
                    .first()
                    .map(|id| id.to_string())
                    .unwrap_or_default()
            })
            .collect();
        let desc_addrs: Vec<String> = desc_contacts
            .iter()
            .map(|c| {
                c.account_identifiers
                    .first()
                    .map(|id| id.to_string())
                    .unwrap_or_default()
            })
            .collect();
        let reversed_asc: Vec<String> = asc_addrs.iter().rev().cloned().collect();
        assert_eq!(
            desc_addrs, reversed_asc,
            "Descending should be reverse of ascending"
        );
    }
}
