//! Identity updates verified with one batch of smart contract wallet checks.
//!
//! Verification runs in three phases. [`prepare`] makes every check that needs
//! no chain, in update, action and field order: it verifies each signature of
//! another kind and checks each ERC-6492 input with [`ScwInput::checked`]. It
//! does no I/O, so a permanent rejection always wins over a retryable outage.
//! One [`SmartContractSignatureVerifier::verify_many`] call then judges each
//! distinct ERC-6492 input once, and [`assemble`] applies each verdict to every
//! signature with that input.

use super::{
    Action, IdentityUpdate, SignatureError,
    unverified::{Checked, UnverifiedAction, UnverifiedIdentityUpdate, UnverifiedSignature},
    verified_signature::VerifiedSignature,
};
use crate::scw_verifier::{
    ScwInput, SmartContractSignatureVerifier, ValidationResponse, VerifierError,
};
use std::collections::{HashMap, hash_map::Entry};

/// Convert a list of unverified updates to verified updates using the given
/// smart contract verifier, with at most one
/// [`SmartContractSignatureVerifier::verify_many`] call for the list.
pub async fn verify_updates(
    updates: Vec<UnverifiedIdentityUpdate>,
    scw_verifier: impl SmartContractSignatureVerifier,
) -> Result<Vec<IdentityUpdate>, SignatureError> {
    verify_all(&updates, scw_verifier).await
}

/// [`verify_updates`] over borrowed updates.
pub(super) async fn verify_all(
    updates: &[UnverifiedIdentityUpdate],
    scw_verifier: impl SmartContractSignatureVerifier,
) -> Result<Vec<IdentityUpdate>, SignatureError> {
    let prepared = prepare(updates)?;
    let verdicts = verdicts(scw_verifier, prepared.inputs()).await?;
    assemble(prepared, &verdicts)
}

/// The signatures of a list of identity updates after [`prepare`].
pub(crate) struct Prepared<'a> {
    updates: &'a [UnverifiedIdentityUpdate],
    /// Each update's signature text.
    texts: Vec<String>,
    checks: Checks,
}

impl Prepared<'_> {
    /// Each distinct ERC-6492 input of the updates, in the order first seen.
    pub(crate) fn inputs(&self) -> &[ScwInput] {
        self.checks.inputs()
    }
}

/// Make every check of `updates` that needs no chain. Does no I/O.
pub(crate) fn prepare(
    updates: &[UnverifiedIdentityUpdate],
) -> Result<Prepared<'_>, SignatureError> {
    let mut checks = Checks::default();
    let texts = updates
        .iter()
        .map(|update| {
            let text = update.signature_text();
            for action in &update.actions {
                checks.add(&text, action)?;
            }
            Ok(text)
        })
        .collect::<Result<_, SignatureError>>()?;
    Ok(Prepared {
        updates,
        texts,
        checks,
    })
}

/// The verified updates, given one verdict for each of [`Prepared::inputs`],
/// in input order. An invalid verdict is [`SignatureError::Invalid`].
pub(crate) fn assemble(
    prepared: Prepared<'_>,
    verdicts: &[ValidationResponse],
) -> Result<Vec<IdentityUpdate>, SignatureError> {
    let Prepared {
        updates,
        texts,
        checks,
    } = prepared;
    let mut bound = checks.bind(verdicts)?;
    updates
        .iter()
        .zip(&texts)
        .map(|(update, text)| {
            let actions = update
                .actions
                .iter()
                .map(|action| bound.action(text, action))
                .collect::<Result<_, _>>()?;
            Ok(IdentityUpdate::new(
                actions,
                update.inbox_id.clone(),
                update.client_timestamp_ns,
            ))
        })
        .collect()
}

/// The verifier's verdicts on `inputs`, without a call when there are none.
pub(super) async fn verdicts(
    scw_verifier: impl SmartContractSignatureVerifier,
    inputs: &[ScwInput],
) -> Result<Vec<ValidationResponse>, SignatureError> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    Ok(scw_verifier.verify_many(inputs).await?)
}

/// One signature after its checks without a chain.
enum Pending {
    /// Verified without a chain.
    Done(VerifiedSignature),
    /// Waits for the verdict on the distinct input at this index.
    Scw(usize),
}

/// Signatures checked without a chain, in the order they were added, with
/// each distinct ERC-6492 input once.
#[derive(Default)]
pub(super) struct Checks {
    inputs: Vec<ScwInput>,
    slots: HashMap<ScwInput, usize>,
    pending: Vec<Pending>,
}

impl Checks {
    /// Check each signature of `action` over `signature_text`, in the order
    /// of [`UnverifiedAction::signatures`].
    pub(super) fn add(
        &mut self,
        signature_text: &str,
        action: &UnverifiedAction,
    ) -> Result<(), SignatureError> {
        for signature in action.signatures() {
            self.add_signature(signature_text, signature)?;
        }
        Ok(())
    }

    /// Check one signature over `signature_text`.
    pub(super) fn add_signature(
        &mut self,
        signature_text: &str,
        signature: &UnverifiedSignature,
    ) -> Result<(), SignatureError> {
        let pending = match signature.check(signature_text)? {
            Checked::Verified(signature) => Pending::Done(signature),
            Checked::Scw(input) => Pending::Scw(self.slot(input)),
        };
        self.pending.push(pending);
        Ok(())
    }

    /// The index of `input` among the distinct inputs, adding it when new.
    fn slot(&mut self, input: ScwInput) -> usize {
        match self.slots.entry(input) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                self.inputs.push(entry.key().clone());
                *entry.insert(self.inputs.len() - 1)
            }
        }
    }

    /// Each distinct ERC-6492 input, in the order first seen.
    pub(super) fn inputs(&self) -> &[ScwInput] {
        &self.inputs
    }

    /// Pair the checks with one verdict for each input, in input order. A
    /// different number of verdicts is a retryable verifier failure, never a
    /// verdict on a signature.
    pub(super) fn bind(self, verdicts: &[ValidationResponse]) -> Result<Bound<'_>, SignatureError> {
        if verdicts.len() != self.inputs.len() {
            return Err(VerifierError::Io(std::io::Error::other(format!(
                "signature verification returned {} verdicts for {} inputs",
                verdicts.len(),
                self.inputs.len()
            )))
            .into());
        }
        Ok(Bound {
            inputs: self.inputs,
            verdicts,
            pending: self.pending.into_iter(),
        })
    }
}

/// [`Checks`] with their verdicts, taken back in the order they were added.
pub(super) struct Bound<'v> {
    inputs: Vec<ScwInput>,
    verdicts: &'v [ValidationResponse],
    pending: std::vec::IntoIter<Pending>,
}

impl Bound<'_> {
    /// The verified form of the next signature checked over `signature_text`.
    pub(super) fn signature(
        &mut self,
        signature_text: &str,
    ) -> Result<VerifiedSignature, SignatureError> {
        match self
            .pending
            .next()
            .expect("a signature is taken back once for each check")
        {
            Pending::Done(signature) => Ok(signature),
            Pending::Scw(slot) => VerifiedSignature::from_scw_verdict(
                &self.inputs[slot],
                &self.verdicts[slot],
                signature_text,
            ),
        }
    }

    /// The verified `action`, which [`Checks::add`] checked over
    /// `signature_text`.
    pub(super) fn action(
        &mut self,
        signature_text: &str,
        action: &UnverifiedAction,
    ) -> Result<Action, SignatureError> {
        action.verified(|| self.signature(signature_text))
    }
}

#[cfg(test)]
mod tests {
    use super::verify_updates;
    use crate::associations::{
        AccountId, Identifier, MemberIdentifier, SignatureError, get_state,
        unsigned_actions::{UnsignedAddAssociation, UnsignedCreateInbox},
        unverified::{
            UnverifiedAction, UnverifiedAddAssociation, UnverifiedCreateInbox,
            UnverifiedIdentityUpdate, UnverifiedSignature,
        },
    };
    use crate::scw_verifier::{
        ScwInput, SmartContractSignatureVerifier, ValidationResponse, VerifierError,
    };
    use alloy::primitives::{BlockNumber, Bytes};
    use parking_lot::Mutex;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use xmtp_common::RetryableError;

    /// The signature bytes [`Recording`] judges invalid.
    const INVALID: &[u8] = &[0xff];

    /// Records each batch it is asked to verify and counts single checks. A
    /// signature is valid unless its bytes are [`INVALID`].
    #[derive(Default)]
    struct Recording {
        batches: Mutex<Vec<Vec<ScwInput>>>,
        singles: AtomicUsize,
    }

    fn verdict(signature: &[u8]) -> ValidationResponse {
        ValidationResponse {
            is_valid: signature != INVALID,
            block_number: Some(1),
            error: None,
        }
    }

    #[xmtp_common::async_trait]
    impl SmartContractSignatureVerifier for Recording {
        async fn is_valid_signature(
            &self,
            _: AccountId,
            _: [u8; 32],
            signature: Bytes,
            _: Option<BlockNumber>,
        ) -> Result<ValidationResponse, VerifierError> {
            self.singles.fetch_add(1, Ordering::SeqCst);
            Ok(verdict(&signature))
        }

        async fn verify_many(
            &self,
            inputs: &[ScwInput],
        ) -> Result<Vec<ValidationResponse>, VerifierError> {
            self.batches.lock().push(inputs.to_vec());
            Ok(inputs
                .iter()
                .map(|input| verdict(input.signature()))
                .collect())
        }
    }

    fn address(index: u32) -> String {
        format!("0x{:040x}", index)
    }

    fn scw_signature(chain_id: u64, address: &str, bytes: &[u8]) -> UnverifiedSignature {
        UnverifiedSignature::new_smart_contract_wallet(
            bytes.to_vec(),
            AccountId::new_evm(chain_id, address.to_string()),
            1,
        )
    }

    fn create(address: &str, signature: UnverifiedSignature) -> UnverifiedAction {
        UnverifiedAction::CreateInbox(UnverifiedCreateInbox::new(
            UnsignedCreateInbox {
                nonce: 0,
                account_identifier: Identifier::eth(address).unwrap(),
            },
            signature,
        ))
    }

    fn add(
        new_address: &str,
        new_signature: UnverifiedSignature,
        existing_signature: UnverifiedSignature,
    ) -> UnverifiedAction {
        UnverifiedAction::AddAssociation(UnverifiedAddAssociation::new(
            UnsignedAddAssociation {
                new_member_identifier: MemberIdentifier::eth(new_address).unwrap(),
            },
            new_signature,
            existing_signature,
        ))
    }

    fn inbox_of(address: &str) -> String {
        Identifier::eth(address).unwrap().inbox_id(0).unwrap()
    }

    /// A cold history's smart contract wallet signatures reach the verifier
    /// as one batch, through the shared verifier a client holds, and never as
    /// one call for each signature. These two updates carry 99 and 100
    /// distinct signatures.
    #[xmtp_common::test(unwrap_try = true)]
    async fn identity_history_verifies_scw_signatures_in_one_batch() {
        let creator = address(1);
        let mut first_actions = vec![create(
            &creator,
            scw_signature(1, &creator, &1u32.to_be_bytes()),
        )];
        let mut second_actions = Vec::new();
        for index in 0..99_u32 {
            let new_address = address(index + 2);
            // The second update uses 50 distinct existing wallets that the
            // first one adds, so its 100 signatures have distinct signers.
            let existing = if index < 49 {
                creator.clone()
            } else {
                address(index - 48)
            };
            let action = add(
                &new_address,
                scw_signature(1, &new_address, &(2 * index + 2).to_be_bytes()),
                scw_signature(1, &existing, &(2 * index + 3).to_be_bytes()),
            );
            if index < 49 {
                first_actions.push(action);
            } else {
                second_actions.push(action);
            }
        }
        let updates = vec![
            UnverifiedIdentityUpdate::new(inbox_of(&creator), 1_000_000_000, first_actions),
            UnverifiedIdentityUpdate::new(inbox_of(&creator), 2_000_000_000, second_actions),
        ];

        let recording = Arc::new(Recording::default());
        let verifier: Arc<Box<dyn SmartContractSignatureVerifier>> =
            Arc::new(Box::new(recording.clone()));
        let verified = verify_updates(updates, &verifier).await?;

        assert_eq!(get_state(verified)?.members().len(), 100);
        let batches = recording.batches.lock();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 199);
        assert_eq!(recording.singles.load(Ordering::SeqCst), 0);
    }

    /// Identical inputs are verified once, and each verdict binds to every
    /// signature with that input and to no other. A wallet that approves
    /// several actions of one update signs one input; the members added
    /// beside it sign on other chains, so a verdict or input bound to the
    /// wrong signature changes a signer or a chain id.
    #[xmtp_common::test(unwrap_try = true)]
    async fn scw_verdicts_map_back_to_their_actions() {
        let [owner, first, second] = [address(1), address(2), address(3)];
        let history = |second_signature: &[u8]| {
            let owner_signature = scw_signature(1, &owner, &[1]);
            vec![UnverifiedIdentityUpdate::new(
                inbox_of(&owner),
                1_000_000_000,
                vec![
                    create(&owner, owner_signature.clone()),
                    add(
                        &first,
                        scw_signature(2, &first, &[2]),
                        owner_signature.clone(),
                    ),
                    add(
                        &second,
                        scw_signature(3, &second, second_signature),
                        owner_signature,
                    ),
                ],
            )]
        };

        let recording = Recording::default();
        let state = get_state(verify_updates(history(&[3]), &recording).await?)?;
        let batches = recording.batches.lock().clone();
        assert_eq!(batches.len(), 1);
        assert_eq!(
            batches[0]
                .iter()
                .map(|input| (input.chain_id(), input.signature().to_vec()))
                .collect::<Vec<_>>(),
            [(1, vec![1]), (2, vec![2]), (3, vec![3])]
        );
        for (member, chain_id) in [(&owner, 1), (&first, 2), (&second, 3)] {
            assert_eq!(
                state
                    .get(&MemberIdentifier::eth(member)?)?
                    .added_on_chain_id,
                Some(chain_id)
            );
        }

        // Only the last signature's input is judged invalid.
        let error = verify_updates(history(INVALID), &recording)
            .await
            .unwrap_err();
        assert!(matches!(error, SignatureError::Invalid), "{error}");
    }

    /// Fails every check with the error its function makes.
    struct Failing(fn() -> VerifierError);

    #[xmtp_common::async_trait]
    impl SmartContractSignatureVerifier for Failing {
        async fn is_valid_signature(
            &self,
            _: AccountId,
            _: [u8; 32],
            _: Bytes,
            _: Option<BlockNumber>,
        ) -> Result<ValidationResponse, VerifierError> {
            Err((self.0)())
        }
    }

    /// An outage: the chain has no route.
    fn unrouted() -> VerifierError {
        VerifierError::NoVerifier("eip155:1".to_string())
    }

    /// A permanent refusal from the verifier.
    fn refused() -> VerifierError {
        VerifierError::UnexpectedERC6492Result("refused".to_string())
    }

    /// Answers one verdict fewer than it is asked for.
    struct Short;

    #[xmtp_common::async_trait]
    impl SmartContractSignatureVerifier for Short {
        async fn is_valid_signature(
            &self,
            _: AccountId,
            _: [u8; 32],
            signature: Bytes,
            _: Option<BlockNumber>,
        ) -> Result<ValidationResponse, VerifierError> {
            Ok(verdict(&signature))
        }

        async fn verify_many(
            &self,
            inputs: &[ScwInput],
        ) -> Result<Vec<ValidationResponse>, VerifierError> {
            Ok(inputs[1..]
                .iter()
                .map(|input| verdict(input.signature()))
                .collect())
        }
    }

    /// A verifier that reaches no verdict, or answers the wrong number of
    /// verdicts, leaves the updates eligible for a later attempt: the error is
    /// retryable and never an invalid signature. A verifier's permanent
    /// refusal stays permanent. A rejection that needs no chain stays
    /// permanent too, even behind a signature that waits on an outage.
    // verifies: IDENT-061
    #[xmtp_common::test(unwrap_try = true)]
    async fn verify_updates_keeps_verifier_failures_retryable() {
        let [owner, member] = [address(1), address(2)];
        let history = || {
            vec![UnverifiedIdentityUpdate::new(
                inbox_of(&owner),
                1_000_000_000,
                vec![
                    create(&owner, scw_signature(1, &owner, &[1])),
                    add(
                        &member,
                        scw_signature(1, &member, &[2]),
                        scw_signature(1, &owner, &[3]),
                    ),
                ],
            )]
        };

        for error in [
            verify_updates(history(), Failing(unrouted))
                .await
                .unwrap_err(),
            verify_updates(history(), Short).await.unwrap_err(),
        ] {
            assert!(
                matches!(error, SignatureError::VerifierError(_)) && error.is_retryable(),
                "{error}"
            );
        }
        let error = verify_updates(history(), Failing(refused))
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                SignatureError::VerifierError(VerifierError::UnexpectedERC6492Result(_))
            ) && !error.is_retryable(),
            "{error}"
        );

        let zero = format!("0x{}", "0".repeat(40));
        let mut updates = history();
        updates.push(UnverifiedIdentityUpdate::new(
            inbox_of(&owner),
            2_000_000_000,
            vec![add(
                &zero,
                scw_signature(1, &zero, &[4]),
                scw_signature(1, &owner, &[5]),
            )],
        ));
        let error = verify_updates(updates, Failing(unrouted))
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                SignatureError::VerifierError(VerifierError::InvalidAccountId(_))
            ),
            "{error}"
        );
        assert!(!error.is_retryable());
    }
}
