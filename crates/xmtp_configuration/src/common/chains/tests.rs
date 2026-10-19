use super::*;

// verifies: IDENT-060
#[xmtp_common::test(unwrap_try = true)]
fn eip155_chain_ids_parse_only_in_canonical_decimal_form() {
    for (chain, id) in [
        ("eip155:0", 0),
        ("eip155:1", 1),
        ("eip155:8453", 8453),
        ("eip155:18446744073709551615", u64::MAX),
    ] {
        assert_eq!(parse_eip155_chain_id(chain), Ok(id), "{chain:?}");
    }
    for chain in ["", "1", "eip155", "EIP155:1", "cosmos:1", ":1", "eip1551"] {
        assert_eq!(
            parse_eip155_chain_id(chain),
            Err(Eip155ChainIdError::MissingPrefix),
            "{chain:?}"
        );
    }
    for chain in [
        "eip155:",
        "eip155:01",
        "eip155:00",
        "eip155:+1",
        "eip155:-1",
        "eip155:0x1",
        "eip155: 1",
        "eip155:1 ",
        "eip155:1\n",
        "eip155:1:0x0",
        "eip155:18446744073709551616",
    ] {
        assert_eq!(
            parse_eip155_chain_id(chain),
            Err(Eip155ChainIdError::NotCanonical),
            "{chain:?}"
        );
    }
}
