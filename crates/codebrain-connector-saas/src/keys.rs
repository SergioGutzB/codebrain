//! Detect Jira-style issue keys (`PROJ-123`) in free text.

/// Return unique issue keys found in `text`, in order of first appearance.
pub fn find_issue_keys(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_uppercase() {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'-' {
            continue;
        }
        let dash = i;
        i += 1;
        let digits_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == digits_start {
            continue;
        }
        let project_len = dash - start;
        let number_len = i - digits_start;
        if !(2..=10).contains(&project_len) || number_len > 6 {
            continue;
        }
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = i >= bytes.len() || !is_ident_byte(bytes[i]);
        if !(before_ok && after_ok) {
            continue;
        }
        if let Ok(key) = std::str::from_utf8(&bytes[start..i]) {
            let key = key.to_string();
            if !out.iter().any(|existing| existing == &key) {
                out.push(key);
            }
        }
    }
    out
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_unique_keys() {
        let keys = find_issue_keys("Fixes MM-147 and also MM-147 / SALES-12 in branch");
        assert_eq!(keys, vec!["MM-147".to_string(), "SALES-12".to_string()]);
    }

    #[test]
    fn ignores_lowercase_noise() {
        assert!(find_issue_keys("see mm-147 later").is_empty());
    }
}
