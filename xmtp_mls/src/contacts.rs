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
    group::{ConversationType, GroupMembershipState, GroupQueryArgs, StoredGroup},
    prelude::*,
};
use xmtp_id::{
    InboxId,
    associations::{AssociationState, Identifier},
};

/// Represents a contact aggregated across all conversations
#[derive(Debug, Clone)]
pub struct Contact {
    pub inbox_id: InboxId,
    pub account_identifiers: Vec<Identifier>,
    pub installation_ids: Vec<Vec<u8>>,
    pub conversation_ids: Vec<Vec<u8>>,
    pub consent_state: ConsentState,
}

/// Query arguments for filtering contacts
#[derive(Debug, Default, Clone)]
pub struct ContactQueryArgs {
    /// Filter by specific group IDs (allow list)
    pub allowed_group_ids: Option<Vec<Vec<u8>>>,
    /// Exclude specific group IDs (deny list)
    pub denied_group_ids: Option<Vec<Vec<u8>>>,
    /// Filter by consent states
    pub consent_states: Option<Vec<ConsentState>>,
    /// Filter by conversation type
    pub conversation_type: Option<ConversationType>,
    /// Filter by membership state
    pub allowed_states: Option<Vec<GroupMembershipState>>,
    /// Only include contacts from groups created after this timestamp
    pub created_after_ns: Option<i64>,
    /// Only include contacts from groups created before this timestamp
    pub created_before_ns: Option<i64>,
}

impl AsRef<ContactQueryArgs> for ContactQueryArgs {
    fn as_ref(&self) -> &ContactQueryArgs {
        self
    }
}

// Type aliases to reduce complexity
type GroupMembersMap = HashMap<Vec<u8>, Vec<(String, i64)>>;
type MemberRequests = HashSet<(String, i64)>;
type ContactMap = HashMap<String, (AssociationState, HashSet<Vec<u8>>, ConsentState)>;

/// Filter groups by allow/deny lists
fn filter_groups_by_allow_deny_lists(
    stored_groups: Vec<StoredGroup>,
    allowed_group_ids: &Option<Vec<Vec<u8>>>,
    denied_group_ids: &Option<Vec<Vec<u8>>>,
) -> Vec<StoredGroup> {
    stored_groups
        .into_iter()
        .filter(|stored_group| {
            let group_id = &stored_group.id;

            // Apply allow list
            if let Some(allowed_ids) = allowed_group_ids
                && !allowed_ids.iter().any(|id| id == group_id)
            {
                return false;
            }

            // Apply deny list
            if let Some(denied_ids) = denied_group_ids
                && denied_ids.iter().any(|id| id == group_id)
            {
                return false;
            }

            true
        })
        .collect()
}

/// Extract group members from filtered groups
/// Returns a tuple of (group_members_map, all_member_requests)
fn extract_group_members<Context>(
    context: &Context,
    filtered_groups: &[StoredGroup],
) -> (GroupMembersMap, MemberRequests)
where
    Context: XmtpSharedContext,
{
    let storage = context.mls_storage();
    let mut group_members_map: GroupMembersMap = HashMap::new();
    let mut all_member_requests: MemberRequests = HashSet::new();

    for stored_group in filtered_groups {
        let mls_group = crate::groups::MlsGroup::new(
            context.clone(),
            stored_group.id.clone(),
            stored_group.dm_id.clone(),
            stored_group.conversation_type,
            stored_group.created_at_ns,
        );

        // Extract group membership from MLS extensions
        let group_membership = match mls_group.load_mls_group_with_lock(storage, |mls_group| {
            Ok(extract_group_membership(mls_group.extensions())?)
        }) {
            Ok(gm) => gm,
            Err(e) => {
                tracing::warn!(
                    "Failed to load group membership for {:?}: {:?}",
                    stored_group.id,
                    e
                );
                continue;
            }
        };

        let member_requests: Vec<(String, i64)> = group_membership
            .members
            .into_iter()
            .map(|(inbox_id, sequence_id)| (inbox_id, sequence_id as i64))
            .filter(|(_, sequence_id)| *sequence_id != 0)
            .collect();

        // Store for this group
        group_members_map.insert(stored_group.id.clone(), member_requests.clone());

        // Add to global set for batch lookup
        all_member_requests.extend(member_requests);
    }

    (group_members_map, all_member_requests)
}

/// Batch read and resolve association states
async fn resolve_association_states<Context>(
    context: &Context,
    member_requests: Vec<(String, i64)>,
) -> Result<HashMap<String, AssociationState>, ClientError>
where
    Context: XmtpSharedContext,
{
    let db = context.db();

    let association_states_vec = db.batch_read_from_cache(member_requests.clone())?;
    let mut association_states: Vec<AssociationState> = association_states_vec
        .into_iter()
        .map(|a| a.try_into())
        .collect::<Result<_, _>>()
        .map_err(StorageError::from)?;

    // Handle missing association states
    if association_states.len() != member_requests.len() {
        let missing_requests: Vec<_> = member_requests
            .iter()
            .filter_map(|(id, sequence)| {
                if association_states
                    .iter()
                    .any(|state| state.inbox_id() == id)
                {
                    return None;
                }
                Some((id.as_str(), Some(*sequence)))
            })
            .collect();

        if !missing_requests.is_empty() {
            let identity_updates = IdentityUpdates::new(context);
            let mut new_states = identity_updates
                .batch_get_association_state(&db, &missing_requests)
                .await?;
            association_states.append(&mut new_states);
        }
    }

    // Create a lookup map: inbox_id -> AssociationState
    Ok(association_states
        .into_iter()
        .map(|state| (state.inbox_id().to_string(), state))
        .collect())
}

/// Build contact map from group members, association states, and consent records
fn build_contact_map<Context>(
    context: &Context,
    filtered_groups: &[StoredGroup],
    group_members_map: &GroupMembersMap,
    association_map: &HashMap<String, AssociationState>,
) -> Result<ContactMap, ClientError>
where
    Context: XmtpSharedContext,
{
    let db = context.db();

    // Batch read ALL consent records in one query
    let all_inbox_ids: Vec<String> = association_map.keys().cloned().collect();
    let consent_map: HashMap<String, ConsentState> = all_inbox_ids
        .iter()
        .filter_map(|inbox_id| {
            db.get_consent_record(inbox_id.clone(), ConsentType::InboxId)
                .ok()
                .flatten()
                .map(|record| (inbox_id.clone(), record.state))
        })
        .collect();

    // Build contact map using the batch-loaded data
    // Use HashSet for conversation_ids during construction to guarantee uniqueness
    let mut contact_map: ContactMap = HashMap::new();

    for stored_group in filtered_groups {
        let group_id = &stored_group.id;

        if let Some(member_requests) = group_members_map.get(group_id) {
            for (inbox_id, _) in member_requests {
                if let Some(association_state) = association_map.get(inbox_id) {
                    let consent_state = consent_map
                        .get(inbox_id)
                        .copied()
                        .unwrap_or(ConsentState::Unknown);

                    contact_map
                        .entry(inbox_id.clone())
                        .and_modify(|(_, conversation_ids, _)| {
                            conversation_ids.insert(group_id.clone());
                        })
                        .or_insert_with(|| {
                            let mut conversation_ids = HashSet::new();
                            conversation_ids.insert(group_id.clone());
                            (association_state.clone(), conversation_ids, consent_state)
                        });
                }
            }
        }
    }

    Ok(contact_map)
}

/// Apply final filters to contacts (own inbox_id, consent states)
fn apply_contact_filters(
    mut contacts: Vec<Contact>,
    own_inbox_id: &str,
    consent_states: &Option<Vec<ConsentState>>,
) -> Vec<Contact> {
    // Filter out the client's own inbox_id
    contacts.retain(|contact| contact.inbox_id != own_inbox_id);

    // Filter by consent states if specified
    if let Some(states) = consent_states {
        contacts.retain(|contact| states.contains(&contact.consent_state));
    }

    contacts
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    /// Query for contacts across all conversations with optional filters
    ///
    /// Aggregates all unique members from conversations (groups and DMs) and returns them as contacts.
    /// Each contact includes their inbox_id, account identifiers, the conversations they're in,
    /// and consent state.
    ///
    /// Filters:
    /// - allowed_group_ids: only include members from these specific group IDs
    /// - denied_group_ids: exclude members from these specific group IDs
    /// - consent_states: only return contacts with the given consent states
    /// - conversation_type: filter by conversation type (Group or Dm)
    /// - allowed_states: only include contacts from groups with the given membership states
    /// - created_after_ns/created_before_ns: filter by group creation time
    pub async fn list_contacts(&self, args: ContactQueryArgs) -> Result<Vec<Contact>, ClientError> {
        let ContactQueryArgs {
            allowed_group_ids,
            denied_group_ids,
            consent_states,
            conversation_type,
            allowed_states,
            created_after_ns,
            created_before_ns,
        } = args;

        // Build group query args from contact query args
        let group_args = GroupQueryArgs {
            allowed_states,
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

        // Get all groups matching the filter criteria directly from the database
        let stored_groups = self.context.db().find_groups(group_args)?;

        // Filter groups by allow/deny lists early
        let filtered_groups =
            filter_groups_by_allow_deny_lists(stored_groups, &allowed_group_ids, &denied_group_ids);

        if filtered_groups.is_empty() {
            tracing::debug!(
                "No groups remaining after applying filters. Query returned 0 contacts."
            );
            return Ok(vec![]);
        }

        // Extract all member inbox_ids from all groups
        let (group_members_map, all_member_requests) =
            extract_group_members(&self.context, &filtered_groups);

        // Batch read and resolve association states
        let requests_vec: Vec<(String, i64)> = all_member_requests.into_iter().collect();
        let association_map = resolve_association_states(&self.context, requests_vec).await?;

        // Build contact map from group members, association states, and consent records
        let contact_map = build_contact_map(
            &self.context,
            &filtered_groups,
            &group_members_map,
            &association_map,
        )?;

        // Convert map to vec
        let contacts: Vec<Contact> = contact_map
            .into_iter()
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

        // Apply final filters (own inbox_id, consent states)
        let filtered_contacts = apply_contact_filters(contacts, self.inbox_id(), &consent_states);

        Ok(filtered_contacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::ClientBuilder;
    use std::time::Duration;
    use xmtp_common::time::now_ns;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::consent_record::{ConsentType, StoredConsentRecord};

    #[xmtp_common::test]
    async fn test_list_contacts() {
        use xmtp_db::group::ConversationType;

        // Create 10 clients
        let clients: Vec<_> = (0..10).map(|_| generate_local_wallet()).collect::<Vec<_>>();

        let mut client_instances = Vec::new();
        for wallet in &clients {
            client_instances.push(ClientBuilder::new_test_client(wallet).await);
        }

        let alice = &client_instances[0];
        let bob = &client_instances[1];
        let charlie = &client_instances[2];
        let diana = &client_instances[3];
        let eve = &client_instances[4];
        let frank = &client_instances[5];
        let grace = &client_instances[6];
        let henry = &client_instances[7];
        let iris = &client_instances[8];
        let jack = &client_instances[9];

        // Create Group 1: Alice, Bob, Charlie, Diana
        let group1 = alice.create_group(None, None).unwrap();
        group1
            .add_members_by_inbox_id(&[bob.inbox_id(), charlie.inbox_id(), diana.inbox_id()])
            .await
            .unwrap();

        bob.sync_welcomes().await.unwrap();
        charlie.sync_welcomes().await.unwrap();
        diana.sync_welcomes().await.unwrap();

        // Small delay between groups
        xmtp_common::time::sleep(Duration::from_millis(10)).await;
        let mid_time = now_ns();
        xmtp_common::time::sleep(Duration::from_millis(10)).await;

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
            .list_contacts(ContactQueryArgs::default())
            .await
            .unwrap();

        assert_eq!(all_contacts.len(), 9, "Alice should have 9 contacts");

        // Verify Alice is not in her own contacts
        assert!(
            !all_contacts.iter().any(|c| c.inbox_id == alice.inbox_id()),
            "Alice should not be in her own contacts"
        );

        // Test 2: Filter by allowed_group_ids (only Group 1)
        let group1_contacts = alice
            .list_contacts(ContactQueryArgs {
                allowed_group_ids: Some(vec![group1.group_id.clone()]),
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

        // Test 3: Filter by denied_group_ids (exclude Group 2)
        let without_group2 = alice
            .list_contacts(ContactQueryArgs {
                denied_group_ids: Some(vec![group2.group_id.clone()]),
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
            .list_contacts(ContactQueryArgs {
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
            .list_contacts(ContactQueryArgs {
                consent_states: Some(vec![ConsentState::Denied]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(denied_contacts.len(), 1, "Should have 1 denied contact");
        assert_eq!(denied_contacts[0].inbox_id, charlie.inbox_id());

        // Test 6: Filter by consent_states (Unknown only)
        let unknown_contacts = alice
            .list_contacts(ContactQueryArgs {
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
            .list_contacts(ContactQueryArgs {
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
            .list_contacts(ContactQueryArgs {
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
            .list_contacts(ContactQueryArgs {
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
            .list_contacts(ContactQueryArgs {
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
            .list_contacts(ContactQueryArgs {
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

        // Test 13: Combined filters - allowed_group_ids + consent_states
        let group1_allowed = alice
            .list_contacts(ContactQueryArgs {
                allowed_group_ids: Some(vec![group1.group_id.clone()]),
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
            .list_contacts(ContactQueryArgs {
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
            .list_contacts(ContactQueryArgs {
                denied_group_ids: Some(vec![
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
}
