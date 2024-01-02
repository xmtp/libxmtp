use super::validated_commit::{AggregatedMembershipChange, CommitParticipant, ValidatedCommit};

pub trait Policy {
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool;
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum StandardPolicies {
    Allow,
    Deny,
    // Allow if the change only applies to subject installations with the same account address as the actor
    AllowSameMember,
    // AllowIfActorAdmin, TODO: Enable this once we have admin roles
    // AllowIfActorCreator, TODO: Enable this once we know who the creator is
    // AllowIfSubjectRevoked, TODO: Enable this once we have revocation and have context on who is revoked
}

impl Policy for StandardPolicies {
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool {
        match self {
            StandardPolicies::Allow => true,
            StandardPolicies::Deny => false,
            StandardPolicies::AllowSameMember => change.account_address == actor.account_address,
        }
    }
}

// An AndCondition evaluates to true if all the policies it contains evaluate to true
#[derive(Clone, Debug)]
pub struct AndCondition<PolicyType>
where
    PolicyType: Policy,
{
    policies: Vec<PolicyType>,
}

#[allow(dead_code)]
impl<PolicyType> AndCondition<PolicyType>
where
    PolicyType: Policy,
{
    pub fn new(policies: Vec<PolicyType>) -> Self {
        Self { policies }
    }
}

impl<PolicyType> Policy for AndCondition<PolicyType>
where
    PolicyType: Policy,
{
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.evaluate(actor, change))
    }
}

// An AnyCondition evaluates to true if any of the contained policies evaluate to true
#[derive(Clone, Debug)]
pub struct AnyCondition<PolicyType>
where
    PolicyType: Policy,
{
    policies: Vec<PolicyType>,
}

#[allow(dead_code)]
impl<PolicyType> AnyCondition<PolicyType>
where
    PolicyType: Policy,
{
    pub fn new(policies: Vec<PolicyType>) -> Self {
        Self { policies }
    }
}

impl<PolicyType> Policy for AnyCondition<PolicyType>
where
    PolicyType: Policy,
{
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool {
        self.policies
            .iter()
            .any(|policy| policy.evaluate(actor, change))
    }
}

// A NotCondition evaluates to true if the contained policy evaluates to false
#[derive(Clone, Debug)]
pub struct NotCondition<PolicyType>
where
    PolicyType: Policy,
{
    policy: PolicyType,
}

#[allow(dead_code)]
impl<PolicyType> NotCondition<PolicyType>
where
    PolicyType: Policy,
{
    pub fn new(policy: PolicyType) -> Self {
        Self { policy }
    }
}

impl<PolicyType> Policy for NotCondition<PolicyType>
where
    PolicyType: Policy,
{
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool {
        !self.policy.evaluate(actor, change)
    }
}

#[allow(dead_code)]
pub struct GroupPermissions<
    AddMemberPolicy,
    RemoveMemberPolicy,
    AddInstallationPolicy,
    RemoveInstallationPolicy,
> where
    AddMemberPolicy: Policy + std::fmt::Debug,
    RemoveMemberPolicy: Policy + std::fmt::Debug,
    AddInstallationPolicy: Policy + std::fmt::Debug,
    RemoveInstallationPolicy: Policy + std::fmt::Debug,
{
    pub add_member_policy: AddMemberPolicy,
    pub remove_member_policy: RemoveMemberPolicy,
    pub add_installation_policy: AddInstallationPolicy,
    pub remove_installation_policy: RemoveInstallationPolicy,
}

#[allow(dead_code)]
impl<AddMemberPolicy, RemoveMemberPolicy, AddInstallationPolicy, RemoveInstallationPolicy>
    GroupPermissions<
        AddMemberPolicy,
        RemoveMemberPolicy,
        AddInstallationPolicy,
        RemoveInstallationPolicy,
    >
where
    AddMemberPolicy: Policy + std::fmt::Debug,
    RemoveMemberPolicy: Policy + std::fmt::Debug,
    AddInstallationPolicy: Policy + std::fmt::Debug,
    RemoveInstallationPolicy: Policy + std::fmt::Debug,
{
    pub fn new(
        add_member_policy: AddMemberPolicy,
        remove_member_policy: RemoveMemberPolicy,
        add_installation_policy: AddInstallationPolicy,
        remove_installation_policy: RemoveInstallationPolicy,
    ) -> Self {
        Self {
            add_member_policy,
            remove_member_policy,
            add_installation_policy,
            remove_installation_policy,
        }
    }

    pub fn evaluate_commit(&self, commit: &ValidatedCommit) -> bool {
        self.evaluate_policy(
            commit.members_added.iter(),
            &self.add_member_policy,
            &commit.actor,
        ) && self.evaluate_policy(
            commit.members_removed.iter(),
            &self.remove_member_policy,
            &commit.actor,
        ) && self.evaluate_policy(
            commit.installations_added.iter(),
            &self.add_installation_policy,
            &commit.actor,
        ) && self.evaluate_policy(
            commit.installations_removed.iter(),
            &self.remove_installation_policy,
            &commit.actor,
        )
    }

    fn evaluate_policy<'a, I, P>(
        &self,
        mut changes: I,
        policy: &P,
        actor: &CommitParticipant,
    ) -> bool
    where
        I: Iterator<Item = &'a AggregatedMembershipChange>,
        P: Policy + std::fmt::Debug,
    {
        changes.all(|change| {
            let is_ok = policy.evaluate(actor, change);
            if !is_ok {
                log::info!(
                    "Policy {:?} failed for actor {:?} and change {:?}",
                    policy,
                    actor,
                    change
                );
            }
            is_ok
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::test::{rand_account_address, rand_vec};

    use super::*;

    fn build_change(
        account_address: Option<String>,
        installation_id: Option<Vec<u8>>,
    ) -> AggregatedMembershipChange {
        AggregatedMembershipChange {
            account_address: account_address.unwrap_or_else(rand_account_address),
            installation_ids: vec![installation_id.unwrap_or_else(rand_vec)],
        }
    }

    fn build_actor(
        account_address: Option<String>,
        installation_id: Option<Vec<u8>>,
    ) -> CommitParticipant {
        CommitParticipant {
            account_address: account_address.unwrap_or_else(rand_account_address),
            installation_id: installation_id.unwrap_or_else(rand_vec),
        }
    }

    fn build_validated_commit(
        // Add a member with the same account address as the actor if true, random account address if false
        member_added: Option<bool>,
        member_removed: Option<bool>,
        installation_added: Option<bool>,
        installation_removed: Option<bool>,
    ) -> ValidatedCommit {
        let actor = build_actor(None, None);
        let build_membership_change = |same_address_as_actor| {
            if same_address_as_actor {
                vec![build_change(Some(actor.account_address.clone()), None)]
            } else {
                vec![build_change(None, None)]
            }
        };
        ValidatedCommit {
            actor: actor.clone(),
            members_added: member_added
                .map(build_membership_change)
                .unwrap_or_default(),
            members_removed: member_removed
                .map(build_membership_change)
                .unwrap_or_else(std::vec::Vec::new),
            installations_added: installation_added
                .map(build_membership_change)
                .unwrap_or_else(std::vec::Vec::new),
            installations_removed: installation_removed
                .map(build_membership_change)
                .unwrap_or_else(std::vec::Vec::new),
        }
    }

    #[test]
    fn test_allow_all() {
        let permissions = GroupPermissions::new(
            StandardPolicies::Allow,
            StandardPolicies::Allow,
            StandardPolicies::Allow,
            StandardPolicies::Allow,
        );
        let commit = build_validated_commit(Some(true), Some(true), Some(true), Some(true));
        assert!(permissions.evaluate_commit(&commit));
    }

    #[test]
    fn test_deny() {
        let permissions = GroupPermissions::new(
            StandardPolicies::Deny,
            StandardPolicies::Deny,
            StandardPolicies::Deny,
            StandardPolicies::Deny,
        );

        let member_added_commit = build_validated_commit(Some(false), None, None, None);
        assert!(!permissions.evaluate_commit(&member_added_commit));

        let member_removed_commit = build_validated_commit(None, Some(false), None, None);
        assert!(!permissions.evaluate_commit(&member_removed_commit));

        let installation_added_commit = build_validated_commit(None, None, Some(false), None);
        assert!(!permissions.evaluate_commit(&installation_added_commit));

        let installation_removed_commit = build_validated_commit(None, None, None, Some(false));
        assert!(!permissions.evaluate_commit(&installation_removed_commit));
    }

    #[test]
    fn test_allow_same_member() {
        let permissions = GroupPermissions::new(
            StandardPolicies::AllowSameMember,
            StandardPolicies::Deny,
            StandardPolicies::Deny,
            StandardPolicies::Deny,
        );

        let commit_with_same_member = build_validated_commit(Some(true), None, None, None);
        assert!(permissions.evaluate_commit(&commit_with_same_member));

        let commit_with_different_member = build_validated_commit(Some(false), None, None, None);
        assert!(!permissions.evaluate_commit(&commit_with_different_member));
    }

    #[test]
    fn test_and_condition() {
        let permissions = GroupPermissions::new(
            AndCondition::new(vec![StandardPolicies::Deny, StandardPolicies::Allow]),
            StandardPolicies::Allow,
            StandardPolicies::Allow,
            StandardPolicies::Allow,
        );

        let member_added_commit = build_validated_commit(Some(true), None, None, None);
        assert!(!permissions.evaluate_commit(&member_added_commit));
    }

    #[test]
    fn test_any_condition() {
        let permissions = GroupPermissions::new(
            AnyCondition::new(vec![StandardPolicies::Deny, StandardPolicies::Allow]),
            StandardPolicies::Allow,
            StandardPolicies::Allow,
            StandardPolicies::Allow,
        );

        let member_added_commit = build_validated_commit(Some(true), None, None, None);
        assert!(permissions.evaluate_commit(&member_added_commit));
    }

    #[test]
    fn test_not_condition() {
        let permissions = GroupPermissions::new(
            NotCondition::new(StandardPolicies::Allow),
            StandardPolicies::Allow,
            StandardPolicies::Allow,
            StandardPolicies::Allow,
        );

        let member_added_commit = build_validated_commit(Some(true), None, None, None);

        assert!(!permissions.evaluate_commit(&member_added_commit));
    }
}
