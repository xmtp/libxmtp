use std::collections::HashMap;

pub(crate) fn get_param_or_default<'a>(params: &'a HashMap<String, String>, key: &str) -> &'a str {
    params.get(key).map(|s| s.as_str()).unwrap_or("")
}
