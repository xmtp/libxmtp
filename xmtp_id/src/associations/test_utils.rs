use rand::{distributions::Alphanumeric, Rng};

pub fn rand_string() -> String {
    let v: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    v
}

pub fn rand_u64() -> u64 {
    rand::thread_rng().gen()
}

pub fn rand_vec() -> Vec<u8> {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill(&mut buf[..]);
    buf.to_vec()
}
