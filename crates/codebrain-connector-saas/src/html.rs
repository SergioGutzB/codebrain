//! Strip Confluence `body.storage` HTML into plain text for indexing.

pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0;
    let mut in_tag = false;
    let mut in_script = false;
    while i < bytes.len() {
        let byte = bytes[i];
        if !in_tag && byte == b'<' {
            let rest = &html[i..];
            let lower = rest.to_ascii_lowercase();
            if lower.starts_with("<script") || lower.starts_with("<style") {
                in_script = true;
            }
            if lower.starts_with("</script") || lower.starts_with("</style") {
                in_script = false;
            }
            // Block-ish tags → newline so paragraphs stay separable.
            if starts_with_block_tag(&lower) && !out.ends_with('\n') {
                out.push('\n');
            }
            in_tag = true;
            i += 1;
            continue;
        }
        if in_tag {
            if byte == b'>' {
                in_tag = false;
            }
            i += 1;
            continue;
        }
        if in_script {
            i += 1;
            continue;
        }
        if byte == b'&' {
            if let Some((ch, consumed)) = decode_entity(&html[i..]) {
                out.push(ch);
                i += consumed;
                continue;
            }
        }
        out.push(char::from(byte));
        i += 1;
    }
    collapse_whitespace(&out)
}

fn starts_with_block_tag(lower: &str) -> bool {
    lower.starts_with("<p")
        || lower.starts_with("<br")
        || lower.starts_with("<div")
        || lower.starts_with("<h1")
        || lower.starts_with("<h2")
        || lower.starts_with("<h3")
        || lower.starts_with("<li")
        || lower.starts_with("<tr")
        || lower.starts_with("</p")
        || lower.starts_with("</div")
        || lower.starts_with("</h")
        || lower.starts_with("</li")
        || lower.starts_with("</tr")
}

fn decode_entity(input: &str) -> Option<(char, usize)> {
    if input.starts_with("&amp;") {
        return Some(('&', 5));
    }
    if input.starts_with("&lt;") {
        return Some(('<', 4));
    }
    if input.starts_with("&gt;") {
        return Some(('>', 4));
    }
    if input.starts_with("&quot;") {
        return Some(('"', 6));
    }
    if input.starts_with("&nbsp;") {
        return Some((' ', 6));
    }
    if input.starts_with("&mdash;") {
        return Some(('—', 7));
    }
    if input.starts_with("&ndash;") {
        return Some(('–', 7));
    }
    if let Some(rest) = input.strip_prefix("&#") {
        let end = rest.find(';')?;
        let digits = &rest[..end];
        let value = if let Some(hex) = digits
            .strip_prefix('x')
            .or_else(|| digits.strip_prefix('X'))
        {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            digits.parse::<u32>().ok()?
        };
        let ch = char::from_u32(value)?;
        return Some((ch, end + 3)); // &# + digits + ;
    }
    if let Some(rest) = input.strip_prefix('&') {
        let end = rest.find(';')?;
        if end > 0 && end <= 10 {
            let name = &rest[..end];
            let ch = match name {
                "aacute" | "Aacute" => 'á',
                "eacute" | "Eacute" => 'é',
                "iacute" | "Iacute" => 'í',
                "oacute" | "Oacute" => 'ó',
                "uacute" | "Uacute" => 'ú',
                "ntilde" => 'ñ',
                "Ntilde" => 'Ñ',
                _ => return None,
            };
            return Some((ch, end + 2));
        }
    }
    None
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_nl = false;
    for line in text.lines() {
        let trimmed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            if !prev_nl && !out.is_empty() {
                out.push('\n');
                prev_nl = true;
            }
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&trimmed);
        prev_nl = false;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_entities() {
        let html = "<h1>Hola &amp; mundo</h1><p>PPS-811 &mdash; demo</p>";
        let text = html_to_text(html);
        assert!(text.contains("Hola & mundo"));
        assert!(text.contains("PPS-811"));
        assert!(text.contains("demo"));
        assert!(!text.contains("<"));
    }
}
