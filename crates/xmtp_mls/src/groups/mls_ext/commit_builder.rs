//! The one way libxmtp starts an OpenMLS commit.

use openmls::{
    group::{CommitBuilder, CreateCommitError, Initial, LoadedPsks, MlsGroup},
    storage::StorageProvider,
};

/// Start a commit on `group`, let `configure` set its proposals and
/// options, and load its PSKs. The returned builder never creates a
/// GroupInfo: a GroupInfo carries the group context, whose AppData
/// dictionary holds the group root, and only a Welcome may carry the root.
///
/// Clippy disallows `MlsGroup::commit_builder`, `CommitBuilder::new`,
/// `CommitBuilder::create_group_info`, and
/// `CommitBuilder::use_ratchet_tree_extension` everywhere else, so no
/// caller can start a commit another way or turn the GroupInfo back on
/// after this function returns.
// implements: GMOD-042
#[allow(clippy::disallowed_methods)]
pub(crate) fn commit_without_group_info<'a, Storage: StorageProvider>(
    group: &'a mut MlsGroup,
    storage: &'a Storage,
    configure: impl FnOnce(CommitBuilder<'a, Initial>) -> CommitBuilder<'a, Initial>,
) -> Result<CommitBuilder<'a, LoadedPsks>, CreateCommitError> {
    Ok(configure(group.commit_builder())
        .load_psks(storage)?
        .create_group_info(false))
}
