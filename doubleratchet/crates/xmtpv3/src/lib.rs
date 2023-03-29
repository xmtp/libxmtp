use anyhow::Result;
use vodozemac::olm::{Account, AccountPickle};


pub fn pickle_account() -> Result<()> {	
    const PICKLE_KEY: [u8; 32] = [0u8; 32];
    let mut account = Account::new();

    account.generate_one_time_keys(10);
    account.generate_fallback_key();

    let pickle = account.pickle().encrypt(&PICKLE_KEY);

    let account2: Account = AccountPickle::from_encrypted(&pickle, &PICKLE_KEY)?.into();

    assert_eq!(account.identity_keys(), account2.identity_keys());

    Ok(())
}

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pickle_account() {
        pickle_account().unwrap();
    }

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
