mod association_log;
mod hashes;
mod member;
mod signature;
mod state;
#[cfg(test)]
mod test_utils;
mod unsigned_actions;

pub use self::association_log::*;
pub use self::member::{Member, MemberIdentifier, MemberKind};
pub use self::signature::{Signature, SignatureError, SignatureKind};
pub use self::state::AssociationState;

// Apply a single IdentityUpdate to an existing AssociationState
pub fn apply_update(
    initial_state: AssociationState,
    update: IdentityUpdate,
) -> Result<AssociationState, AssociationError> {
    update.update_state(Some(initial_state))
}

// Get the current state from an array of `IdentityUpdate`s. Entire operation fails if any operation fails
pub fn get_state(updates: Vec<IdentityUpdate>) -> Result<AssociationState, AssociationError> {
    let new_state = updates.iter().try_fold(
        None,
        |state, update| -> Result<Option<AssociationState>, AssociationError> {
            let updated_state = update.update_state(state)?;
            Ok(Some(updated_state))
        },
    )?;

    new_state.ok_or(AssociationError::NotCreated)
}

#[cfg(test)]
mod tests {
    use self::test_utils::{rand_string, rand_u64, rand_vec};

    use super::*;

    #[derive(Clone)]
    struct MockSignature {
        is_valid: bool,
        signer_identity: MemberIdentifier,
        signature_kind: SignatureKind,
        signature_nonce: u64,
    }

    impl IdentityUpdate {
        pub fn new_test(actions: Vec<Action>) -> Self {
            Self::new(actions, rand_u64())
        }
    }

    impl MockSignature {
        pub fn new_boxed(
            is_valid: bool,
            signer_identity: MemberIdentifier,
            signature_kind: SignatureKind,
            // Signature nonce is used to control what the signature bytes are
            // Defaults to random
            signature_nonce: Option<u64>,
        ) -> Box<Self> {
            let nonce = signature_nonce.unwrap_or(rand_u64());
            Box::new(Self {
                is_valid,
                signer_identity,
                signature_kind,
                signature_nonce: nonce,
            })
        }
    }

    impl Signature for MockSignature {
        fn signature_kind(&self) -> SignatureKind {
            self.signature_kind.clone()
        }

        fn recover_signer(&self) -> Result<MemberIdentifier, SignatureError> {
            match self.is_valid {
                true => Ok(self.signer_identity.clone()),
                false => Err(SignatureError::Invalid),
            }
        }

        fn bytes(&self) -> Vec<u8> {
            let sig = format!("{}{}", self.signer_identity, self.signature_nonce);
            sig.as_bytes().to_vec()
        }
    }

    impl Default for AddAssociation {
        fn default() -> Self {
            let existing_member = rand_string();
            let new_member = rand_vec();
            return Self {
                existing_member_signature: MockSignature::new_boxed(
                    true,
                    existing_member.into(),
                    SignatureKind::Erc191,
                    None,
                ),
                new_member_signature: MockSignature::new_boxed(
                    true,
                    new_member.clone().into(),
                    SignatureKind::InstallationKey,
                    None,
                ),
                new_member_identifier: new_member.into(),
            };
        }
    }

    // Default will create an inbox with a ERC-191 signature
    impl Default for CreateInbox {
        fn default() -> Self {
            let signer = rand_string();
            return Self {
                nonce: rand_u64(),
                account_address: signer.clone(),
                initial_address_signature: MockSignature::new_boxed(
                    true,
                    signer.into(),
                    SignatureKind::Erc191,
                    None,
                ),
            };
        }
    }

    impl Default for RevokeAssociation {
        fn default() -> Self {
            let signer = rand_string();
            return Self {
                recovery_address_signature: MockSignature::new_boxed(
                    true,
                    signer.into(),
                    SignatureKind::Erc191,
                    None,
                ),
                revoked_member: rand_string().into(),
            };
        }
    }

    fn new_test_inbox() -> AssociationState {
        let create_request = CreateInbox::default();
        let identity_update = IdentityUpdate::new_test(vec![Action::CreateInbox(create_request)]);

        get_state(vec![identity_update]).unwrap()
    }

    fn new_test_inbox_with_installation() -> AssociationState {
        let initial_state = new_test_inbox();
        let initial_wallet_address: MemberIdentifier =
            initial_state.recovery_address().clone().into();

        let update = Action::AddAssociation(AddAssociation {
            existing_member_signature: MockSignature::new_boxed(
                true,
                initial_wallet_address.clone(),
                SignatureKind::Erc191,
                None,
            ),
            ..Default::default()
        });

        apply_update(initial_state, IdentityUpdate::new_test(vec![update])).unwrap()
    }

    #[test]
    fn test_create_inbox() {
        let create_request = CreateInbox::default();
        let account_address = create_request.account_address.clone();
        let identity_update = IdentityUpdate::new_test(vec![Action::CreateInbox(create_request)]);
        let state = get_state(vec![identity_update]).unwrap();
        assert_eq!(state.members().len(), 1);

        let existing_entity = state.get(&account_address.clone().into()).unwrap();
        assert!(existing_entity.identifier.eq(&account_address.into()));
    }

    #[test]
    fn create_and_add_separately() {
        let initial_state = new_test_inbox();
        let new_installation_identifier: MemberIdentifier = rand_vec().into();
        let first_member: MemberIdentifier = initial_state.recovery_address().clone().into();

        let update = Action::AddAssociation(AddAssociation {
            new_member_identifier: new_installation_identifier.clone(),
            new_member_signature: MockSignature::new_boxed(
                true,
                new_installation_identifier.clone(),
                SignatureKind::InstallationKey,
                None,
            ),
            existing_member_signature: MockSignature::new_boxed(
                true,
                first_member.clone(),
                SignatureKind::Erc191,
                None,
            ),
            ..Default::default()
        });

        let new_state =
            apply_update(initial_state, IdentityUpdate::new_test(vec![update])).unwrap();
        assert_eq!(new_state.members().len(), 2);

        let new_member = new_state.get(&new_installation_identifier).unwrap();
        assert_eq!(new_member.added_by_entity, Some(first_member));
    }

    #[test]
    fn create_and_add_together() {
        let create_action = CreateInbox::default();
        let account_address = create_action.account_address.clone();
        let new_member_identifier: MemberIdentifier = rand_vec().into();
        let add_action = AddAssociation {
            existing_member_signature: MockSignature::new_boxed(
                true,
                account_address.clone().into(),
                SignatureKind::Erc191,
                None,
            ),
            // Add an installation ID
            new_member_signature: MockSignature::new_boxed(
                true,
                new_member_identifier.clone(),
                SignatureKind::InstallationKey,
                None,
            ),
            new_member_identifier: new_member_identifier.clone(),
            ..Default::default()
        };
        let identity_update = IdentityUpdate::new_test(vec![
            Action::CreateInbox(create_action),
            Action::AddAssociation(add_action),
        ]);
        let state = get_state(vec![identity_update]).unwrap();
        assert_eq!(state.members().len(), 2);
        assert_eq!(
            state.get(&new_member_identifier).unwrap().added_by_entity,
            Some(account_address.into())
        );
    }

    #[test]
    fn create_from_legacy_key() {
        let member_identifier: MemberIdentifier = rand_string().into();
        let create_action = CreateInbox {
            nonce: 0,
            account_address: member_identifier.to_string(),
            initial_address_signature: MockSignature::new_boxed(
                true,
                member_identifier.clone(),
                SignatureKind::LegacyDelegated,
                Some(0),
            ),
        };
        let state = get_state(vec![IdentityUpdate::new_test(vec![Action::CreateInbox(
            create_action,
        )])])
        .unwrap();
        assert_eq!(state.members().len(), 1);

        // The legacy key can only be used once. After this, subsequent updates should fail
        let update = Action::AddAssociation(AddAssociation {
            existing_member_signature: MockSignature::new_boxed(
                true,
                member_identifier,
                SignatureKind::LegacyDelegated,
                // All requests from the same legacy key will have the same signature nonce
                Some(0),
            ),
            ..Default::default()
        });
        let update_result = apply_update(state, IdentityUpdate::new_test(vec![update]));
        assert!(update_result.is_err());
        assert_eq!(update_result.err().unwrap(), AssociationError::Replay);
    }

    #[test]
    fn add_wallet_from_installation_key() {
        let initial_state = new_test_inbox_with_installation();
        let installation_id = initial_state
            .members_by_kind(MemberKind::Installation)
            .first()
            .cloned()
            .unwrap()
            .identifier;

        let new_wallet_address: MemberIdentifier = rand_string().into();
        let add_association = Action::AddAssociation(AddAssociation {
            new_member_identifier: new_wallet_address.clone(),
            new_member_signature: MockSignature::new_boxed(
                true,
                new_wallet_address.clone(),
                SignatureKind::Erc191,
                None,
            ),
            existing_member_signature: MockSignature::new_boxed(
                true,
                installation_id.clone(),
                SignatureKind::InstallationKey,
                None,
            ),
            ..Default::default()
        });

        let new_state = apply_update(
            initial_state,
            IdentityUpdate::new_test(vec![add_association]),
        )
        .expect("expected update to succeed");
        assert_eq!(new_state.members().len(), 3);
    }

    #[test]
    fn reject_invalid_signature_on_create() {
        let bad_signature =
            MockSignature::new_boxed(false, rand_string().into(), SignatureKind::Erc191, None);
        let action = CreateInbox {
            initial_address_signature: bad_signature.clone(),
            ..Default::default()
        };

        let state_result = get_state(vec![IdentityUpdate::new_test(vec![Action::CreateInbox(
            action,
        )])]);
        assert!(state_result.is_err());
        assert_eq!(
            state_result.err().unwrap(),
            AssociationError::Signature(SignatureError::Invalid)
        );
    }

    #[test]
    fn reject_invalid_signature_on_update() {
        let initial_state = new_test_inbox();
        let bad_signature =
            MockSignature::new_boxed(false, rand_string().into(), SignatureKind::Erc191, None);

        let update_with_bad_existing_member = Action::AddAssociation(AddAssociation {
            existing_member_signature: bad_signature.clone(),
            ..Default::default()
        });

        let update_result = apply_update(
            initial_state.clone(),
            IdentityUpdate::new_test(vec![update_with_bad_existing_member]),
        );
        assert!(update_result.is_err());
        assert_eq!(
            update_result.err().unwrap(),
            AssociationError::Signature(SignatureError::Invalid)
        );

        let update_with_bad_new_member = Action::AddAssociation(AddAssociation {
            new_member_signature: bad_signature.clone(),
            existing_member_signature: MockSignature::new_boxed(
                true,
                initial_state.recovery_address().clone().into(),
                SignatureKind::Erc191,
                None,
            ),
            ..Default::default()
        });

        let update_result_2 = apply_update(
            initial_state,
            IdentityUpdate::new_test(vec![update_with_bad_new_member]),
        );
        assert!(update_result_2.is_err());
        assert_eq!(
            update_result_2.err().unwrap(),
            AssociationError::Signature(SignatureError::Invalid)
        );
    }

    #[test]
    fn reject_if_signer_not_existing_member() {
        let create_request = Action::CreateInbox(CreateInbox::default());
        // The default here will create an AddAssociation from a random wallet
        let update = Action::AddAssociation(AddAssociation {
            // Existing member signature is coming from a random wallet
            existing_member_signature: MockSignature::new_boxed(
                true,
                rand_string().into(),
                SignatureKind::Erc191,
                None,
            ),
            ..Default::default()
        });

        let state_result = get_state(vec![IdentityUpdate::new_test(vec![create_request, update])]);
        assert!(state_result.is_err());
        assert_eq!(
            state_result.err().unwrap(),
            AssociationError::MissingExistingMember
        );
    }

    #[test]
    fn reject_if_installation_adding_installation() {
        let existing_state = new_test_inbox_with_installation();
        let existing_installations = existing_state.members_by_kind(MemberKind::Installation);
        let existing_installation = existing_installations.first().unwrap();
        let new_installation_id: MemberIdentifier = rand_vec().into();

        let update = Action::AddAssociation(AddAssociation {
            existing_member_signature: MockSignature::new_boxed(
                true,
                existing_installation.identifier.clone(),
                SignatureKind::InstallationKey,
                None,
            ),
            new_member_identifier: new_installation_id.clone(),
            new_member_signature: MockSignature::new_boxed(
                true,
                new_installation_id.clone(),
                SignatureKind::InstallationKey,
                None,
            ),
            ..Default::default()
        });

        let update_result = apply_update(existing_state, IdentityUpdate::new_test(vec![update]));
        assert!(update_result.is_err());
        assert_eq!(
            update_result.err().unwrap(),
            AssociationError::MemberNotAllowed(
                MemberKind::Installation.to_string(),
                MemberKind::Installation.to_string()
            )
        );
    }

    #[test]
    fn revoke() {
        let initial_state = new_test_inbox_with_installation();
        let installation_id = initial_state
            .members_by_kind(MemberKind::Installation)
            .first()
            .cloned()
            .unwrap()
            .identifier;
        let update = Action::RevokeAssociation(RevokeAssociation {
            recovery_address_signature: MockSignature::new_boxed(
                true,
                initial_state.recovery_address().clone().into(),
                SignatureKind::Erc191,
                None,
            ),
            revoked_member: installation_id.clone(),
            ..Default::default()
        });

        let new_state = apply_update(initial_state, IdentityUpdate::new_test(vec![update]))
            .expect("expected update to succeed");
        assert!(new_state.get(&installation_id).is_none());
    }

    #[test]
    fn revoke_children() {
        let initial_state = new_test_inbox_with_installation();
        let wallet_address = initial_state
            .members_by_kind(MemberKind::Address)
            .first()
            .cloned()
            .unwrap()
            .identifier;

        let add_second_installation = Action::AddAssociation(AddAssociation {
            existing_member_signature: MockSignature::new_boxed(
                true,
                wallet_address.clone(),
                SignatureKind::Erc191,
                None,
            ),
            ..Default::default()
        });

        let new_state = apply_update(
            initial_state,
            IdentityUpdate::new_test(vec![add_second_installation]),
        )
        .expect("expected update to succeed");
        assert_eq!(new_state.members().len(), 3);

        let revocation = Action::RevokeAssociation(RevokeAssociation {
            recovery_address_signature: MockSignature::new_boxed(
                true,
                wallet_address.clone(),
                SignatureKind::Erc191,
                None,
            ),
            revoked_member: wallet_address.clone(),
            ..Default::default()
        });

        // With this revocation the original wallet + both installations should be gone
        let new_state = apply_update(new_state, IdentityUpdate::new_test(vec![revocation]))
            .expect("expected update to succeed");
        assert_eq!(new_state.members().len(), 0);
    }

    #[test]
    fn revoke_and_re_add() {
        let initial_state = new_test_inbox();
        let wallet_address = initial_state
            .members_by_kind(MemberKind::Address)
            .first()
            .cloned()
            .unwrap()
            .identifier;

        let second_wallet_address: MemberIdentifier = rand_string().into();
        let add_second_wallet = Action::AddAssociation(AddAssociation {
            new_member_identifier: second_wallet_address.clone(),
            new_member_signature: MockSignature::new_boxed(
                true,
                second_wallet_address.clone(),
                SignatureKind::Erc191,
                None,
            ),
            existing_member_signature: MockSignature::new_boxed(
                true,
                wallet_address.clone(),
                SignatureKind::Erc191,
                None,
            ),
            ..Default::default()
        });

        let revoke_second_wallet = Action::RevokeAssociation(RevokeAssociation {
            recovery_address_signature: MockSignature::new_boxed(
                true,
                wallet_address.clone(),
                SignatureKind::Erc191,
                None,
            ),
            revoked_member: second_wallet_address.clone(),
            ..Default::default()
        });

        let state_after_remove = apply_update(
            initial_state,
            IdentityUpdate::new_test(vec![add_second_wallet, revoke_second_wallet]),
        )
        .expect("expected update to succeed");
        assert_eq!(state_after_remove.members().len(), 1);

        let add_second_wallet_again = Action::AddAssociation(AddAssociation {
            new_member_identifier: second_wallet_address.clone(),
            new_member_signature: MockSignature::new_boxed(
                true,
                second_wallet_address.clone(),
                SignatureKind::Erc191,
                None,
            ),
            existing_member_signature: MockSignature::new_boxed(
                true,
                wallet_address,
                SignatureKind::Erc191,
                None,
            ),
            ..Default::default()
        });

        let state_after_re_add = apply_update(
            state_after_remove,
            IdentityUpdate::new_test(vec![add_second_wallet_again]),
        )
        .expect("expected update to succeed");
        assert_eq!(state_after_re_add.members().len(), 2);
    }

    #[test]
    fn change_recovery_address() {
        let initial_state = new_test_inbox_with_installation();
        let initial_recovery_address: MemberIdentifier =
            initial_state.recovery_address().clone().into();
        let new_recovery_address = rand_string();
        let update_recovery = Action::ChangeRecoveryAddress(ChangeRecoveryAddress {
            new_recovery_address: new_recovery_address.clone(),
            recovery_address_signature: MockSignature::new_boxed(
                true,
                initial_state.recovery_address().clone().into(),
                SignatureKind::Erc191,
                None,
            ),
        });

        let new_state = apply_update(
            initial_state,
            IdentityUpdate::new_test(vec![update_recovery]),
        )
        .expect("expected update to succeed");
        assert_eq!(new_state.recovery_address(), &new_recovery_address);

        let attempted_revoke = Action::RevokeAssociation(RevokeAssociation {
            recovery_address_signature: MockSignature::new_boxed(
                true,
                initial_recovery_address.clone(),
                SignatureKind::Erc191,
                None,
            ),
            revoked_member: initial_recovery_address.clone(),
            ..Default::default()
        });

        let revoke_result =
            apply_update(new_state, IdentityUpdate::new_test(vec![attempted_revoke]));
        assert!(revoke_result.is_err());
        assert_eq!(
            revoke_result.err().unwrap(),
            AssociationError::MissingExistingMember
        );
    }
}
