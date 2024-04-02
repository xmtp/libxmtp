mod association_log;
mod entity;
mod hashes;
mod signature;
mod state;
#[cfg(test)]
mod test_utils;

pub use self::association_log::*;
pub use self::entity::{Entity, EntityRole};
pub use self::signature::{Signature, SignatureError, SignatureKind};
pub use self::state::{AssociationState, StateError};

pub fn apply_updates(
    initial_state: AssociationState,
    association_events: Vec<AssociationEvent>,
) -> AssociationState {
    association_events
        .iter()
        .fold(initial_state, |state, update| {
            match update.update_state(Some(state.clone())) {
                Ok(new_state) => new_state,
                Err(err) => {
                    println!("invalid entry {}", err);
                    state
                }
            }
        })
}

pub fn get_state(
    association_updates: Vec<AssociationEvent>,
) -> Result<AssociationState, AssociationError> {
    association_updates
        .iter()
        .try_fold(None, |existing_state, update| {
            match update.update_state(existing_state.clone()) {
                Ok(new_state) => Ok(Some(new_state)),
                Err(err) => {
                    println!("Invalid state update {}", err);
                    Err(err)
                }
            }
        })
        .map(|v| v.unwrap())
}

#[cfg(test)]
mod tests {
    use self::test_utils::{rand_string, rand_u32};

    use super::*;

    struct MockSignature {
        is_valid: bool,
        signer_identity: String,
        signature_kind: SignatureKind,
    }

    impl MockSignature {
        pub fn new_boxed(
            is_valid: bool,
            signer_identity: String,
            signature_kind: SignatureKind,
        ) -> Box<Self> {
            Box::new(Self {
                is_valid,
                signer_identity,
                signature_kind,
            })
        }
    }

    impl Signature for MockSignature {
        fn signature_kind(&self) -> SignatureKind {
            self.signature_kind.clone()
        }

        fn recover_signer(&self) -> Result<String, SignatureError> {
            match self.is_valid {
                true => Ok(self.signer_identity.clone()),
                false => Err(SignatureError::Invalid),
            }
        }

        fn text(&self) -> String {
            self.signer_identity.clone()
        }
    }

    impl Default for AddAssociation {
        fn default() -> Self {
            return Self {
                client_timestamp_ns: rand_u32(),
                new_member_role: EntityRole::Installation,
                existing_member_signature: MockSignature::new_boxed(
                    true,
                    rand_string(),
                    SignatureKind::Erc191,
                ),
                new_member_signature: MockSignature::new_boxed(
                    true,
                    rand_string(),
                    SignatureKind::InstallationKey,
                ),
            };
        }
    }

    impl Default for CreateXid {
        fn default() -> Self {
            let signer = rand_string();
            return Self {
                nonce: rand_u32(),
                account_address: signer.clone(),
                initial_association: AddAssociation {
                    existing_member_signature: MockSignature::new_boxed(
                        true,
                        signer,
                        SignatureKind::Erc191,
                    ),
                    ..Default::default()
                },
            };
        }
    }

    impl Default for RevokeAssociation {
        fn default() -> Self {
            let signer = rand_string();
            return Self {
                client_timestamp_ns: rand_u32(),
                recovery_address_signature: MockSignature::new_boxed(
                    true,
                    signer,
                    SignatureKind::Erc191,
                ),
                revoked_member: rand_string(),
            };
        }
    }

    #[test]
    fn test_create_and_add() {
        let create_request = CreateXid::default();
        let account_address = create_request.account_address.clone();
        let state = get_state(vec![AssociationEvent::CreateXid(create_request)]).unwrap();
        assert_eq!(state.entities().len(), 2);

        let existing_entity = state.get(&account_address).unwrap();
        assert_eq!(existing_entity.id, account_address);
    }

    #[test]
    fn create_and_add_chained() {
        let create_request = CreateXid::default();
        let state = get_state(vec![AssociationEvent::CreateXid(create_request)]).unwrap();
        assert_eq!(state.entities().len(), 2);

        let all_installations = state.entities_by_role(EntityRole::Installation);
        let new_installation = all_installations.first().unwrap();

        let update = AssociationEvent::AddAssociation(AddAssociation {
            client_timestamp_ns: rand_u32(),
            new_member_role: EntityRole::Address,
            new_member_signature: MockSignature::new_boxed(
                true,
                rand_string(),
                SignatureKind::Erc191,
            ),
            existing_member_signature: MockSignature::new_boxed(
                true,
                new_installation.id.clone(),
                SignatureKind::InstallationKey,
            ),
        });

        let new_state = apply_updates(state, vec![update]);
        assert_eq!(new_state.entities().len(), 3);
    }

    #[test]
    fn create_from_legacy_key() {
        let account_address = rand_string();
        let create_request = CreateXid {
            nonce: 0,
            account_address: account_address.clone(),
            initial_association: AddAssociation {
                client_timestamp_ns: rand_u32(),
                existing_member_signature: MockSignature::new_boxed(
                    true,
                    account_address.clone(),
                    SignatureKind::Erc191,
                ),
                new_member_signature: MockSignature::new_boxed(
                    true,
                    rand_string(),
                    SignatureKind::LegacyKey,
                ),
                new_member_role: EntityRole::LegacyKey,
            },
        };

        let initial_state = get_state(vec![AssociationEvent::CreateXid(create_request)]).unwrap();
        assert_eq!(initial_state.entities().len(), 2);

        let legacy_keys = initial_state.entities_by_role(EntityRole::LegacyKey);
        assert_eq!(legacy_keys.len(), 1);
    }

    #[test]
    fn reject_invalid_signature() {
        let create_request = CreateXid {
            initial_association: AddAssociation {
                existing_member_signature: MockSignature::new_boxed(
                    false,
                    rand_string(),
                    SignatureKind::Erc191,
                ),
                ..Default::default()
            },
            ..Default::default()
        };

        let state_result = get_state(vec![AssociationEvent::CreateXid(create_request)]);
        assert_eq!(state_result.is_err(), true);
        assert_eq!(
            state_result.err().unwrap(),
            AssociationError::Signature(SignatureError::Invalid)
        );
    }

    #[test]
    fn reject_if_signer_not_existing_member() {
        let create_request = CreateXid::default();
        let state = get_state(vec![AssociationEvent::CreateXid(create_request)]).unwrap();

        // The default here will create an AddAssociation from a random wallet
        let update = AssociationEvent::AddAssociation(AddAssociation {
            ..Default::default()
        });

        let new_state_result = update.update_state(Some(state));
        assert_eq!(
            new_state_result.err(),
            Some(AssociationError::MissingExistingMember)
        )
    }

    #[test]
    fn test_revoke_wallet() {
        let create_request = CreateXid::default();
        let initial_wallet = create_request.account_address.clone();
        let state = get_state(vec![AssociationEvent::CreateXid(create_request)]).unwrap();

        // This update should revoke the initial wallet and the one installation that is associated to it
        let update = AssociationEvent::RevokeAssociation(RevokeAssociation {
            recovery_address_signature: MockSignature::new_boxed(
                true,
                initial_wallet.clone(),
                SignatureKind::Erc191,
            ),
            revoked_member: initial_wallet.clone(),
            ..Default::default()
        });

        let new_state = update.update_state(Some(state)).unwrap();
        assert_eq!(new_state.entities().len(), 0);
    }

    #[test]
    fn test_revoke_installation() {
        let create_request = CreateXid::default();
        let state = get_state(vec![AssociationEvent::CreateXid(create_request)]).unwrap();
        let recovery_address = state.recovery_address.clone();
        let all_installations = state.entities_by_role(EntityRole::Installation);
        let installation_to_revoke = all_installations.first().unwrap();

        let update = RevokeAssociation {
            recovery_address_signature: MockSignature::new_boxed(
                true,
                recovery_address,
                SignatureKind::Erc191,
            ),
            revoked_member: installation_to_revoke.id.clone(),
            ..Default::default()
        };

        let new_state = update.update_state(Some(state)).unwrap();
        assert_eq!(new_state.entities().len(), 1);
    }

    #[test]
    fn test_replay_detection() {
        let create_request = CreateXid::default();
        let original_nonce = create_request.initial_association.client_timestamp_ns;
        let original_installation_id = create_request
            .initial_association
            .new_member_signature
            .recover_signer()
            .unwrap();

        let state = get_state(vec![AssociationEvent::CreateXid(create_request)]).unwrap();

        let recovery_address = state.recovery_address.clone();
        let all_installations = state.entities_by_role(EntityRole::Installation);
        let installation_to_revoke = all_installations.first().unwrap();

        let update = RevokeAssociation {
            recovery_address_signature: MockSignature::new_boxed(
                true,
                recovery_address.clone(),
                SignatureKind::Erc191,
            ),
            revoked_member: installation_to_revoke.id.clone(),
            ..Default::default()
        };

        let new_state = update.update_state(Some(state)).unwrap();
        assert_eq!(new_state.entities().len(), 1);

        let attempt_to_replay = AddAssociation {
            client_timestamp_ns: original_nonce,
            existing_member_signature: MockSignature::new_boxed(
                true,
                recovery_address,
                SignatureKind::Erc191,
            ),
            new_member_signature: MockSignature::new_boxed(
                true,
                original_installation_id,
                SignatureKind::InstallationKey,
            ),
            ..Default::default()
        };

        let replay_result = attempt_to_replay.update_state(Some(new_state));
        assert_eq!(replay_result.is_err(), true);
        assert_eq!(replay_result.err().unwrap(), AssociationError::Replay)
    }
}
