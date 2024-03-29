use rand::{distributions::Alphanumeric, Rng};

pub fn rand_string() -> String {
    let v: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    v
}

pub fn rand_u32() -> u32 {
    rand::thread_rng().gen()
}
