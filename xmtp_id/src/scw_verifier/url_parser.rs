use std::{collections::HashMap, fs, str::FromStr};
use url::Url;

pub fn parse_chain_urls() -> HashMap<u64, Url> {
    let json = fs::read_to_string("chain_urls.json").unwrap();
    let json: HashMap<u64, String> = serde_json::from_str(&json).unwrap();

    json.into_iter()
        .map(|(id, url)| (id, Url::from_str(&url).unwrap()))
        .collect()
}
