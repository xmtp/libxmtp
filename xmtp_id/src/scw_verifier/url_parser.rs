use std::{collections::HashMap, fs, str::FromStr};
use url::Url;

pub fn parse_chain_urls() -> HashMap<u64, Url> {
    let json = fs::read_to_string("chain_urls.json").expect("chain_urls.json is missing");
    let json: HashMap<u64, String> =
        serde_json::from_str(&json).expect("chain_urls.json is malformatted");

    json.into_iter()
        .map(|(id, url)| {
            (
                id,
                Url::from_str(&url).expect("unable to parse url in chain_urls.json"),
            )
        })
        .collect()
}
