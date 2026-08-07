//! Cross-channel mention linker: document body → symbol (`MENTIONS`).

use std::collections::HashMap;

use codebrain_db::SymbolMentionTarget;

#[derive(Debug, Clone, PartialEq)]
pub struct MentionMatch {
    pub symbol_source: String,
    pub symbol_fqn: String,
    pub confidence: f32,
    pub evidence: String,
}

#[derive(Debug, Clone)]
struct IndexedSymbol {
    source: String,
    fqn: String,
    name_confidence: f32,
    fqn_confidence: f32,
}

/// Precomputed lookup tables so vault indexing stays ~O(tokens) per note.
#[derive(Debug, Default, Clone)]
pub struct MentionIndex {
    by_fqn: HashMap<String, IndexedSymbol>,
    by_name: HashMap<String, IndexedSymbol>,
}

impl MentionIndex {
    pub fn build(symbols: &[SymbolMentionTarget], threshold: f32) -> Self {
        let mut by_fqn = HashMap::new();
        let mut by_name = HashMap::new();
        for symbol in symbols {
            let indexed = IndexedSymbol {
                source: symbol.source_name.clone(),
                fqn: symbol.fqn.clone(),
                name_confidence: 0.9,
                fqn_confidence: 1.0,
            };
            if !symbol.fqn.is_empty() && indexed.fqn_confidence >= threshold {
                by_fqn.insert(symbol.fqn.clone(), indexed.clone());
            }
            if !symbol.name.is_empty()
                && symbol.name.len() >= 2
                && indexed.name_confidence >= threshold
            {
                // Prefer the first FQN when names collide; longer evidence still wins later.
                by_name.entry(symbol.name.clone()).or_insert(indexed);
            }
        }
        Self { by_fqn, by_name }
    }

    pub fn is_empty(&self) -> bool {
        self.by_fqn.is_empty() && self.by_name.is_empty()
    }
}

/// Find symbol mentions in a note body using word-boundary matching.
///
/// Confidence: exact FQN match = `1.0`, exact name match = `0.9`.
/// Longer needle wins when overlapping.
pub fn find_mentions(
    body: &str,
    symbols: &[SymbolMentionTarget],
    threshold: f32,
) -> Vec<MentionMatch> {
    find_mentions_indexed(body, &MentionIndex::build(symbols, threshold))
}

/// Same as [`find_mentions`] but reuses a prebuilt index (hot path during vault index).
pub fn find_mentions_indexed(body: &str, index: &MentionIndex) -> Vec<MentionMatch> {
    if index.is_empty() || body.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for (start, end, text) in body_identifiers(body) {
        if let Some(symbol) = index.by_fqn.get(text) {
            candidates.push(Candidate {
                start,
                end,
                symbol_source: symbol.source.clone(),
                symbol_fqn: symbol.fqn.clone(),
                confidence: symbol.fqn_confidence,
                evidence: text.to_string(),
            });
            continue;
        }
        if let Some(symbol) = index.by_name.get(text) {
            candidates.push(Candidate {
                start,
                end,
                symbol_source: symbol.source.clone(),
                symbol_fqn: symbol.fqn.clone(),
                confidence: symbol.name_confidence,
                evidence: text.to_string(),
            });
        }
    }

    candidates.sort_by(|left, right| {
        (right.end - right.start)
            .cmp(&(left.end - left.start))
            .then(right.confidence.total_cmp(&left.confidence))
            .then(left.start.cmp(&right.start))
    });

    let mut claimed = vec![false; body.len()];
    let mut matches = Vec::new();
    for candidate in candidates {
        if claimed[candidate.start..candidate.end]
            .iter()
            .any(|value| *value)
        {
            continue;
        }
        for slot in &mut claimed[candidate.start..candidate.end] {
            *slot = true;
        }
        matches.push(MentionMatch {
            symbol_source: candidate.symbol_source,
            symbol_fqn: candidate.symbol_fqn,
            confidence: candidate.confidence,
            evidence: candidate.evidence,
        });
    }

    matches.sort_by(|left, right| {
        left.symbol_fqn
            .cmp(&right.symbol_fqn)
            .then(left.evidence.cmp(&right.evidence))
    });
    matches.dedup_by(|left, right| {
        left.symbol_source == right.symbol_source && left.symbol_fqn == right.symbol_fqn
    });
    matches
}

#[derive(Debug)]
struct Candidate {
    start: usize,
    end: usize,
    symbol_source: String,
    symbol_fqn: String,
    confidence: f32,
    evidence: String,
}

fn body_identifiers(body: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !is_ident_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && is_ident_byte(bytes[i]) {
            i += 1;
        }
        // Only emit if the span is valid UTF-8 (identifiers are ASCII in practice).
        if let Ok(text) = std::str::from_utf8(&bytes[start..i])
            && text.len() >= 2
        {
            out.push((start, i, text));
        }
    }
    out
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_name_and_fqn_with_threshold() {
        let symbols = vec![
            SymbolMentionTarget {
                source_name: "code".into(),
                name: "Greeter".into(),
                fqn: "Services::Greeter".into(),
            },
            SymbolMentionTarget {
                source_name: "code".into(),
                name: "run".into(),
                fqn: "src::lib::run".into(),
            },
        ];
        let body = "The Services::Greeter class and another Greeter mention; also run helper.";
        let matches = find_mentions(body, &symbols, 0.75);
        assert!(
            matches
                .iter()
                .any(|value| value.symbol_fqn == "Services::Greeter" && value.confidence >= 0.9)
        );
        assert!(
            matches
                .iter()
                .any(|value| value.symbol_fqn == "src::lib::run")
        );
    }

    #[test]
    fn ignores_partial_identifier_tokens() {
        let symbols = vec![SymbolMentionTarget {
            source_name: "code".into(),
            name: "run".into(),
            fqn: "run".into(),
        }];
        assert!(find_mentions("runtime runner", &symbols, 0.75).is_empty());
        assert_eq!(find_mentions("please run now", &symbols, 0.75).len(), 1);
    }

    #[test]
    fn high_threshold_skips_name_only_matches() {
        let symbols = vec![SymbolMentionTarget {
            source_name: "code".into(),
            name: "Greeter".into(),
            fqn: "Services::Greeter".into(),
        }];
        assert!(find_mentions("Greeter only", &symbols, 0.95).is_empty());
        assert_eq!(find_mentions("Services::Greeter", &symbols, 0.95).len(), 1);
    }
}
