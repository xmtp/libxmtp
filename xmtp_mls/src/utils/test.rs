use std::env;

use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};
use tempfile::{Builder, TempPath};

pub fn rand_string() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
}

pub fn rand_vec() -> Vec<u8> {
    rand::thread_rng().gen::<[u8; 24]>().to_vec()
}

pub fn tmp_path() -> String {
    let db_name = rand_string();
    return TempPath::from_path(format!(
        "{}/{}.db3",
        env::temp_dir().to_str().unwrap(),
        db_name
    ))
    .to_str()
    .unwrap()
    .to_string();
}
