use super::*;

/// A name part of `len` characters: a letter, then digits and the two
/// punctuation characters the grammar allows.
fn name_of_len(len: usize) -> String {
    let tail = "0_-9".chars().cycle().take(len - 1);
    std::iter::once('a').chain(tail).collect()
}

/// The shortest and longest name parts, the lowest code, and the longest
/// handle are all handles, so no limit is off by one.
// verifies: IDENT-078
#[xmtp_common::test(unwrap_try = true)]
fn grammar_accepts_boundary_forms() {
    let longest_name = name_of_len(MAX_HANDLE_NAME_LEN);
    let longest = format!("{longest_name}#9999");
    assert_eq!(longest.len(), 32);
    for handle in [
        "a",
        "nick",
        "n1_-",
        longest_name.as_str(),
        "nick#0000",
        "nick#2341",
        longest.as_str(),
    ] {
        assert_eq!(Handle::parse(handle)?.as_str(), handle);
    }
}

/// Every other spelling is rejected: capitals, a name part that does not start
/// with a letter, punctuation the grammar does not list, a code of the wrong
/// length, a name part one character too long, a letter outside ASCII, and a
/// trailing newline.
// verifies: IDENT-078
#[xmtp_common::test(unwrap_try = true)]
fn grammar_rejects_other_forms() {
    let too_long = name_of_len(MAX_HANDLE_NAME_LEN + 1);
    for input in [
        "",
        "Nick",
        "nicK",
        "1nick",
        "_nick",
        "-nick",
        "nick.eth",
        "nick@x",
        "nick ",
        "nick#123",
        "nick#12345",
        "nick#",
        "#1234",
        "nick#12a4",
        "nick#1234#5678",
        too_long.as_str(),
        "nïck",
        "nick\n",
        "nick#1234\n",
    ] {
        assert_eq!(Handle::parse(input), Err(InvalidHandle), "{input:?}");
    }
}

/// `parse` keeps the bytes it was given or fails; `from_input` changes only
/// ASCII capitals.
#[xmtp_common::test(unwrap_try = true)]
fn parse_keeps_bytes_and_input_lowercases() {
    assert_eq!(Handle::parse("NICK#0042"), Err(InvalidHandle));
    assert_eq!(Handle::from_input("NICK#0042")?.as_str(), "nick#0042");
    assert_eq!(Handle::from_input("Nick_X")?.as_str(), "nick_x");
    assert_eq!(Handle::from_input("nick")?.as_str(), "nick");
    // A capital outside ASCII is not lowercased, so the input stays invalid.
    assert_eq!(Handle::from_input("NÏCK"), Err(InvalidHandle));
    assert_eq!(Handle::from_input("nick\n"), Err(InvalidHandle));
}

/// The name part stops at `#`, and `with_code` writes the code as four digits.
#[xmtp_common::test(unwrap_try = true)]
fn name_part_excludes_code() {
    assert_eq!(Handle::parse("nick#2341")?.name(), "nick");
    assert_eq!(Handle::parse("nick")?.name(), "nick");
    assert_eq!(Handle::with_code("nick", 7)?.as_str(), "nick#0007");
    assert_eq!(Handle::with_code("nick", 9_999)?.as_str(), "nick#9999");
    assert_eq!(Handle::with_code("nick", 10_000), Err(InvalidHandle));
    assert_eq!(Handle::with_code("nick#0001", 1), Err(InvalidHandle));
    assert_eq!(Handle::with_code("Nick", 1), Err(InvalidHandle));
}
