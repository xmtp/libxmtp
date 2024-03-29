#[derive(Clone, Debug, PartialEq)]
pub enum EntityRole {
    Installation,
    Address,
    LegacyKey,
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub role: EntityRole,
    pub id: String,
    pub is_revoked: bool,
}

impl Entity {
    pub fn new(role: EntityRole, id: String, is_revoked: bool) -> Self {
        Self {
            role,
            id,
            is_revoked,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::associations::test_utils;

    use super::*;

    use test_utils::rand_string;

    impl Default for Entity {
        fn default() -> Self {
            Self {
                role: EntityRole::Address,
                id: rand_string(),
                is_revoked: false,
            }
        }
    }
}
