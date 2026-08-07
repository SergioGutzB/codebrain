/// Target extracted from `[[wikilink]]`, `[[wikilink|alias]]`, or `![[embed]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wikilink {
    pub target: String,
    pub display: Option<String>,
    pub raw: String,
}

pub fn extract_wikilinks(body: &str) -> Vec<Wikilink> {
    let bytes = body.as_bytes();
    let mut links = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let embed = bytes.get(index) == Some(&b'!');
        let open_at = if embed { index + 1 } else { index };
        if bytes.get(open_at..open_at.saturating_add(2)) != Some(b"[[") {
            index += 1;
            continue;
        }
        let content_start = open_at + 2;
        let Some(relative_end) = body[content_start..].find("]]") else {
            break;
        };
        let content_end = content_start + relative_end;
        let raw_start = if embed { index } else { open_at };
        let raw_end = content_end + 2;
        let raw = body[raw_start..raw_end].to_string();
        let inner = body[content_start..content_end].trim();
        let (target_part, display) = match inner.split_once('|') {
            Some((target, display)) => (target, Some(display.trim())),
            None => (inner, None),
        };
        let target = target_part
            .split_once('#')
            .map_or(target_part, |(target, _)| target)
            .trim()
            .replace('\\', "/");
        if !target.is_empty() {
            links.push(Wikilink {
                target,
                display: display
                    .map(str::to_string)
                    .filter(|value| !value.is_empty()),
                raw,
            });
        }
        index = raw_end;
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_aliased_heading_and_embed_links() {
        let body =
            "See [[Note B]] and [[Note B|display]] plus [[path/Note#Heading]] and ![[Embed]].";
        let links = extract_wikilinks(body);
        assert_eq!(links.len(), 4);
        assert_eq!(links[0].target, "Note B");
        assert_eq!(links[1].display.as_deref(), Some("display"));
        assert_eq!(links[2].target, "path/Note");
        assert_eq!(links[3].target, "Embed");
    }

    #[test]
    fn ignores_empty_targets() {
        assert!(extract_wikilinks("[[]] [[|#]]").is_empty());
    }
}
