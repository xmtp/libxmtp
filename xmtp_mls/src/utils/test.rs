use std::env;

use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};

use crate::types::Address;

pub fn rand_string() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
}

pub fn rand_wallet_address() -> Address {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 42)
}

pub fn rand_vec() -> Vec<u8> {
    rand::thread_rng().gen::<[u8; 24]>().to_vec()
}

pub fn tmp_path() -> String {
    let db_name = rand_string();
    format!("{}/{}.db3", env::temp_dir().to_str().unwrap(), db_name)
}

pub fn rand_time() -> i64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(0..1_000_000_000)
}
