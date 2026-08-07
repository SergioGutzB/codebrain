use std::path::Path;

/// Minimal Obsidian YAML frontmatter reader for `title`, `aliases`, and `tags`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNote {
    pub frontmatter: Frontmatter,
    pub body: String,
}

pub fn split_frontmatter(raw: &str) -> ParsedNote {
    let normalized = raw.replace("\r\n", "\n");
    if !normalized.starts_with("---\n") {
        return ParsedNote {
            frontmatter: Frontmatter::default(),
            body: normalized,
        };
    }

    let rest = &normalized[4..];
    let Some(end) = rest.find("\n---") else {
        return ParsedNote {
            frontmatter: Frontmatter::default(),
            body: normalized,
        };
    };
    let yaml = &rest[..end];
    let body_start = end + "\n---".len();
    let body = rest
        .get(body_start..)
        .unwrap_or_default()
        .trim_start_matches('\n')
        .to_string();

    ParsedNote {
        frontmatter: parse_frontmatter(yaml),
        body,
    }
}

pub fn default_title(relative_path: &str, frontmatter: &Frontmatter, body: &str) -> String {
    if let Some(title) = frontmatter.title.as_ref().filter(|value| !value.is_empty()) {
        return title.clone();
    }
    if let Some(heading) = first_heading(body) {
        return heading;
    }
    Path::new(relative_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(relative_path)
        .to_string()
}

pub fn collect_tags(frontmatter: &Frontmatter, body: &str) -> Vec<String> {
    let mut tags = frontmatter.tags.clone();
    for tag in inline_tags(body) {
        if !tags.iter().any(|existing| existing == &tag) {
            tags.push(tag);
        }
    }
    tags
}

fn parse_frontmatter(yaml: &str) -> Frontmatter {
    let mut frontmatter = Frontmatter::default();
    let mut lines = yaml.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(value) = stripped_key(trimmed, "title:") {
            frontmatter.title = Some(unquote(value));
            continue;
        }
        if let Some(value) = stripped_key(trimmed, "aliases:") {
            frontmatter.aliases = parse_list_value(value, &mut lines);
            continue;
        }
        if let Some(value) = stripped_key(trimmed, "tags:") {
            frontmatter.tags = parse_list_value(value, &mut lines);
        }
    }
    frontmatter
}

fn stripped_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix(key).map(str::trim)
}

fn parse_list_value(
    inline: &str,
    lines: &mut std::iter::Peekable<std::str::Lines<'_>>,
) -> Vec<String> {
    if inline.is_empty() {
        let mut values = Vec::new();
        while let Some(next) = lines.peek().copied() {
            let trimmed = next.trim();
            if let Some(item) = trimmed.strip_prefix("- ") {
                values.push(unquote(item.trim()));
                lines.next();
            } else {
                break;
            }
        }
        return values;
    }
    if let Some(inner) = inline
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        return inner
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(unquote)
            .collect();
    }
    vec![unquote(inline)]
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        return trimmed[1..trimmed.len().saturating_sub(1)].to_string();
    }
    trimmed.to_string()
}

fn first_heading(body: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix('#')
            .map(|rest| rest.trim_start_matches('#').trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn inline_tags(body: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for token in body
        .split(|character: char| character.is_whitespace() || ".,;:!?()[]{}".contains(character))
    {
        if let Some(tag) = token.strip_prefix('#')
            && !tag.is_empty()
            && tag.chars().all(|character| {
                character.is_alphanumeric() || character == '_' || character == '-'
            })
            && !tags.iter().any(|existing| existing == tag)
        {
            tags.push(tag.to_string());
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_yaml_frontmatter_lists_and_title() {
        let raw = "---\ntitle: Note A\naliases:\n  - Alpha\n  - A\ntags: [docs, graph]\n---\n\nBody #inline\n";
        let parsed = split_frontmatter(raw);
        assert_eq!(parsed.frontmatter.title.as_deref(), Some("Note A"));
        assert_eq!(parsed.frontmatter.aliases, vec!["Alpha", "A"]);
        assert_eq!(parsed.frontmatter.tags, vec!["docs", "graph"]);
        assert!(parsed.body.contains("Body"));
        assert_eq!(
            collect_tags(&parsed.frontmatter, &parsed.body),
            vec!["docs", "graph", "inline"]
        );
    }

    #[test]
    fn falls_back_to_heading_then_filename() {
        assert_eq!(
            default_title("folder/Note.md", &Frontmatter::default(), "# Heading\n"),
            "Heading"
        );
        assert_eq!(
            default_title("folder/Note.md", &Frontmatter::default(), "no heading"),
            "Note"
        );
    }
}
