/// Apply the file name table to an optional remote name.
pub fn local_file_name(filename: Option<&str>) -> String {
    filename.map_or_else(|| "attachment".to_owned(), sanitize_path_component)
}

/// Apply steps 2 to 8 of the file name table to a path component.
pub fn sanitize_path_component(value: &str) -> String {
    let part = value.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name: String = part
        .chars()
        .filter(|&c| {
            !matches!(c, '\u{0000}'..='\u{001f}' | '\u{007f}'..='\u{009f}'
                | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                | '<' | '>' | ':' | '"' | '|' | '?' | '*')
        })
        .collect();
    name = name.trim_matches(['.', ' ']).to_owned();
    let stem = name.split('.').next().unwrap_or_default();
    let upper = stem.to_ascii_uppercase();
    let reserved = matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || (upper.starts_with("COM") || upper.starts_with("LPT"))
        && upper
            .chars()
            .nth(3)
            .is_some_and(|c| matches!(c, '1'..='9' | '¹' | '²' | '³'))
        && upper.chars().count() == 4;
    if reserved {
        name.insert(0, '_');
    }
    while name.len() > 255 {
        let remove_at = name.rfind('.').and_then(|dot| {
            if dot > 0 {
                name[..dot].char_indices().last().map(|(index, _)| index)
            } else {
                None
            }
        });
        let remove_at = remove_at.or_else(|| name.char_indices().last().map(|(index, _)| index));
        if let Some(index) = remove_at {
            name.remove(index);
        }
    }
    let name = name.trim_matches(['.', ' ']);
    if name.is_empty() {
        "attachment".to_owned()
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // verifies: ATCH-042
    #[xmtp_common::test(unwrap_try = true)]
    async fn file_name_table() {
        assert_eq!(local_file_name(None), "attachment");
        let cases = [
            ("a/b\\c.txt", "c.txt"),
            (
                "a\u{0000}b\u{007f}c\u{009f}d\u{202e}e\u{2066}f.txt",
                "abcdef.txt",
            ),
            (" <bad>:name|?*.txt ", "badname.txt"),
            (" ..hello.. ", "hello"),
            ("..", "attachment"),
            ("\\", "attachment"),
            ("CON.txt", "_CON.txt"),
            ("conin$.txt", "_conin$.txt"),
            ("COM¹.txt", "_COM¹.txt"),
            ("lpt³", "_lpt³"),
            ("COM0.txt", "COM0.txt"),
            ("normal.txt", "normal.txt"),
        ];
        for (input, expected) in cases {
            assert_eq!(local_file_name(Some(input)), expected, "{input:?}");
        }
        let long = format!("{}.pdf", "é".repeat(130));
        let actual = local_file_name(Some(&long));
        assert_eq!(actual.len(), 254);
        assert!(actual.ends_with(".pdf"));
        assert_eq!(actual.chars().filter(|&c| c == 'é').count(), 125);
    }
}
