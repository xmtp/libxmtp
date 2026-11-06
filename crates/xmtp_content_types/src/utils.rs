use serde::{Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};

pub(crate) fn get_param_or_default<'a>(params: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    params.get(key).map(|s| s.as_str()).unwrap_or("")
}

pub(crate) fn serialize_sorted_map<S: Serializer, V: Serialize>(
    map: &HashMap<String, V>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    map.iter().collect::<BTreeMap<_, _>>().serialize(serializer)
}

pub(crate) fn serialize_sorted_optional_map<S: Serializer, V: Serialize>(
    map: &Option<HashMap<String, V>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    map.as_ref()
        .map(|map| map.iter().collect::<BTreeMap<_, _>>())
        .serialize(serializer)
}
