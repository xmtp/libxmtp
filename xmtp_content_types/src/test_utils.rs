#[cfg(test)]
use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};

#[cfg(test)]
pub(crate) fn rand_string() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
}

#[cfg(test)]
pub(crate) fn rand_vec() -> Vec<u8> {
    rand::thread_rng().gen::<[u8; 24]>().to_vec()
}
