//! The group root in a group context's `GROUP_ROOT_KEY` component.
//!
//! A keyed group stores its 32-byte root in the immutable `GROUP_ROOT_KEY`
//! component. Every installation the group adds receives it in the Welcome's
//! group context, and a joiner accepts a Welcome only when the root derives
//! the Welcome's group id.
//!
//! Reads borrow the dictionary's bytes. The only copies these helpers make
//! are a [`GroupRoot`] and the [`GroupKeys`] derived from it, and both wipe
//! on drop. No error carries root or key bytes.

use openmls::{extensions::Extensions, group::GroupContext};

use crate::{
    app_data::{
        component_id::ComponentId, component_source::read_from_app_data_dict_from_extensions,
    },
    group_keys::{GroupKeys, GroupRoot, GroupRootLengthError},
};

/// Why a group context's root cannot name its group.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GroupRootError {
    /// The group context has no `GROUP_ROOT_KEY` component.
    #[error("the group context has no GROUP_ROOT_KEY component")]
    Missing,
    /// The stored root is not 32 bytes.
    #[error(transparent)]
    Length(#[from] GroupRootLengthError),
    /// The root derives a group id other than the group's.
    #[error("the group root does not derive the group id")]
    OtherGroup,
}

/// Read the group root from a group context. `Ok(None)` when the context has
/// no `GROUP_ROOT_KEY` component, as for a restored placeholder.
pub fn read_group_root(
    extensions: &Extensions<GroupContext>,
) -> Result<Option<GroupRoot>, GroupRootLengthError> {
    read_from_app_data_dict_from_extensions(ComponentId::GROUP_ROOT_KEY, extensions)
        .map(GroupRoot::from_bytes)
        .transpose()
}

/// Derive the group key table from the root in a group context.
pub fn group_keys_from_extensions(
    extensions: &Extensions<GroupContext>,
) -> Result<GroupKeys, GroupRootError> {
    let root = read_group_root(extensions)?.ok_or(GroupRootError::Missing)?;
    Ok(GroupKeys::derive(&root))
}

/// Require the root in a group context to derive `group_id`, and return the
/// keys it derives. A 16-byte group id never matches: every derived id is a
/// 32-byte public key.
// implements: JOIN-083
pub fn check_group_root(
    extensions: &Extensions<GroupContext>,
    group_id: &[u8],
) -> Result<GroupKeys, GroupRootError> {
    let keys = group_keys_from_extensions(extensions)?;
    if keys.group_id().as_slice() != group_id {
        return Err(GroupRootError::OtherGroup);
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmls::extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension};

    fn extensions(root: Option<&[u8]>) -> Extensions<GroupContext> {
        let mut dictionary = AppDataDictionary::new();
        if let Some(root) = root {
            dictionary.insert(ComponentId::GROUP_ROOT_KEY.as_u16(), root.to_vec());
        }
        Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dictionary),
        )])
        .unwrap()
    }

    fn id_of(root: &[u8]) -> [u8; 32] {
        GroupKeys::derive(&GroupRoot::from_bytes(root).unwrap()).group_id()
    }

    // verifies: JOIN-083
    #[xmtp_common::test(unwrap_try = true)]
    fn the_root_must_derive_the_group_id() {
        let root = [7u8; 32];
        let group_id = id_of(&root);

        let keys = check_group_root(&extensions(Some(&root)), &group_id)?;
        assert_eq!(keys.group_id(), group_id);

        // Another root's id, the same id with one bit flipped, and a 16-byte
        // id that is a prefix of the real one all fail.
        let other = id_of(&[8u8; 32]);
        let mut flipped = group_id;
        flipped[31] ^= 1;
        for id in [&other[..], &flipped[..], &group_id[..16]] {
            assert_eq!(
                check_group_root(&extensions(Some(&root)), id).unwrap_err(),
                GroupRootError::OtherGroup
            );
        }

        assert_eq!(
            check_group_root(&extensions(None), &group_id).unwrap_err(),
            GroupRootError::Missing
        );
        assert_eq!(
            check_group_root(&Extensions::<GroupContext>::empty(), &group_id).unwrap_err(),
            GroupRootError::Missing
        );
        for len in [0, 16, 31, 33, 64] {
            assert_eq!(
                check_group_root(&extensions(Some(&vec![7u8; len])), &group_id).unwrap_err(),
                GroupRootError::Length(GroupRootLengthError { len })
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn errors_show_no_root_bytes() {
        let root = [0xA5u8; 32];
        let error = check_group_root(&extensions(Some(&root)), &[0u8; 32]).unwrap_err();
        for text in [format!("{error}"), format!("{error:?}")] {
            assert!(!text.contains(&hex::encode(root)), "{text}");
            assert!(!text.contains(&hex::encode(id_of(&root))), "{text}");
            assert!(!text.contains("165, 165"), "{text}");
        }
    }
}
