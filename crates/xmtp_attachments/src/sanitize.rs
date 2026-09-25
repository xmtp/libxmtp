/// Apply the file name table to an optional remote name.
pub fn local_file_name(filename: Option<&str>) -> String {
    filename.map_or_else(|| "attachment".to_owned(), sanitize_path_component)
}

/// Apply steps 2 to 8 of the file name table to a path component.
pub fn sanitize_path_component(value: &str) -> String {
    sanitize_path_component_with_limit(value, 255)
}

/// Apply the file name table with a caller-selected UTF-8 byte limit.
pub fn sanitize_path_component_with_limit(value: &str, limit: usize) -> String {
    let part = value.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean_ascii = part.bytes().all(|byte| {
        matches!(byte, 0x20..=0x7e)
            && !matches!(byte, b'<' | b'>' | b':' | b'"' | b'|' | b'?' | b'*')
    });
    let mut name: String = if clean_ascii {
        part.to_owned()
    } else {
        part.chars()
            .filter(|&c| {
                !matches!(c, '\u{0000}'..='\u{001f}' | '\u{007f}'..='\u{009f}'
                    | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                    | '<' | '>' | ':' | '"' | '|' | '?' | '*')
            })
            .collect()
    };
    if name.starts_with(['.', ' ']) || name.ends_with(['.', ' ']) {
        name = name.trim_matches(['.', ' ']).to_owned();
    }
    if is_reserved_device_name(&name) {
        name.insert(0, '_');
    }
    if name.len() > limit {
        name = match name.rfind('.') {
            Some(dot) if dot > 0 => {
                let suffix = &name[dot..];
                if suffix.len() >= limit {
                    truncate_bytes(suffix, limit).to_owned()
                } else {
                    let stem = truncate_bytes(&name[..dot], limit - suffix.len());
                    format!("{stem}{suffix}")
                }
            }
            _ => truncate_bytes(&name, limit).to_owned(),
        };
    }
    let name = name.trim_matches(['.', ' ']);
    if name.is_empty() {
        "attachment".to_owned()
    } else {
        name.to_owned()
    }
}

pub(crate) fn is_reserved_device_name(name: &str) -> bool {
    let stem = if name.is_ascii() {
        name[..name.len().min(8)]
            .split('.')
            .next()
            .unwrap_or_default()
    } else {
        name.split('.').next().unwrap_or_default()
    };
    if stem.len() <= 7 {
        let upper = stem.to_ascii_uppercase();
        matches!(
            upper.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper
                .chars()
                .nth(3)
                .is_some_and(|c| matches!(c, '1'..='9' | '¹' | '²' | '³'))
            && upper.chars().count() == 4
    } else {
        false
    }
}

/// Keep the longest UTF-8 prefix within a byte limit.
fn truncate_bytes(value: &str, limit: usize) -> &str {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
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
            ("CONOUT$.txt", "_CONOUT$.txt"),
            ("conout$", "_conout$"),
            ("CONOUT$X.txt", "CONOUT$X.txt"),
            ("CONOUT$1", "CONOUT$1"),
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

        let no_dot = "a".repeat(256);
        assert_eq!(local_file_name(Some(&no_dot)), "a".repeat(255));
        let after_truncation = format!("{} bb", "a".repeat(254));
        assert_eq!(local_file_name(Some(&after_truncation)), "a".repeat(254));
        let long_extension = format!("x.{}", "a".repeat(300));
        assert_eq!(local_file_name(Some(&long_extension)), "a".repeat(254));
    }

    // verifies: ATCH-042
    #[xmtp_common::test(unwrap_try = true)]
    async fn huge_file_name_linear() {
        let no_dot = "a".repeat(4 * 1024 * 1024);
        assert_eq!(local_file_name(Some(&no_dot)), "a".repeat(255));

        let long_extension = format!("x.{}", "a".repeat(4 * 1024 * 1024));
        assert_eq!(local_file_name(Some(&long_extension)), "a".repeat(254));
    }
}
