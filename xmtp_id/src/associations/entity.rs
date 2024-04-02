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
    pub added_by_entity: Option<String>,
}

impl Entity {
    pub fn new(role: EntityRole, id: String, added_by_entity: Option<String>) -> Self {
        Self {
            role,
            id,
            added_by_entity,
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
                added_by_entity: None,
            }
        }
    }
}
