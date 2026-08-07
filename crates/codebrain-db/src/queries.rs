//! Read-only graph queries backing the MCP tools.

use serde::{Deserialize, Serialize};

use crate::client::Database;
use crate::error::Result;
use crate::ids::{decision_id, document_id, file_id, source_id, symbol_id};

/// Node kinds addressable from the MCP surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Symbol,
    File,
    Document,
    Decision,
}

impl NodeKind {
    pub const fn table(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::File => "file",
            Self::Document => "document",
            Self::Decision => "architecture_decision",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "symbol" => Some(Self::Symbol),
            "file" => Some(Self::File),
            "document" => Some(Self::Document),
            "decision" | "architecture_decision" | "adr" => Some(Self::Decision),
            _ => None,
        }
    }
}

/// Stable address for a graph node: kind + source + business key (fqn or path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeAddress {
    pub kind: NodeKind,
    pub source: String,
    pub key: String,
}

impl NodeAddress {
    pub fn record_id(&self) -> String {
        match self.kind {
            NodeKind::Symbol => symbol_id(&self.source, &self.key),
            NodeKind::File => file_id(&self.source, &self.key),
            NodeKind::Document => document_id(&self.source, &self.key),
            NodeKind::Decision => decision_id(&self.key),
        }
    }

    /// Render as `kind:source:key`, the form exposed to agents.
    pub fn to_token(&self) -> String {
        format!(
            "{}:{}:{}",
            match self.kind {
                NodeKind::Symbol => "symbol",
                NodeKind::File => "file",
                NodeKind::Document => "document",
                NodeKind::Decision => "decision",
            },
            self.source,
            self.key
        )
    }

    pub fn parse_token(token: &str) -> Option<Self> {
        let (kind, rest) = token.split_once(':')?;
        let (source, key) = rest.split_once(':')?;
        if source.is_empty() || key.is_empty() {
            return None;
        }
        Some(Self {
            kind: NodeKind::parse(kind)?,
            source: source.to_string(),
            key: key.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSummary {
    pub name: String,
    pub kind: String,
    pub root_path: Option<String>,
    pub files: i64,
    pub symbols: i64,
    pub documents: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolHit {
    pub source: String,
    pub name: String,
    pub fqn: String,
    pub kind: String,
    pub signature: Option<String>,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
}

impl SymbolHit {
    pub fn address(&self) -> NodeAddress {
        NodeAddress {
            kind: NodeKind::Symbol,
            source: self.source.clone(),
            key: self.fqn.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentHit {
    pub source: String,
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub excerpt: String,
}

impl DocumentHit {
    pub fn address(&self) -> NodeAddress {
        NodeAddress {
            kind: NodeKind::Document,
            source: self.source.clone(),
            key: self.path.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeighborEdge {
    pub relation: String,
    pub direction: Direction,
    pub node: NodeAddress,
    pub label: String,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Outgoing,
    Incoming,
}

const RELATION_TABLES: [&str; 8] = [
    "defines",
    "calls",
    "imports",
    "references",
    "mentions",
    "explains",
    "about",
    "resolves",
];

#[derive(Debug, Deserialize)]
struct SourceRow {
    name: String,
    kind: String,
    root_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CountRow {
    count: i64,
}

#[derive(Debug, Deserialize)]
struct SymbolRow {
    name: String,
    fqn: String,
    kind: String,
    signature: Option<String>,
    start_line: i64,
    end_line: i64,
    file_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DocumentRow {
    path: String,
    title: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    body: String,
}

#[derive(Debug, Deserialize)]
struct EdgeRow {
    #[serde(rename = "in")]
    from: surrealdb::RecordId,
    #[serde(rename = "out")]
    to: surrealdb::RecordId,
    confidence: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct NodeInfoRow {
    name: Option<String>,
    fqn: Option<String>,
    path: Option<String>,
    title: Option<String>,
    source_name: Option<String>,
}

pub async fn list_sources(db: &Database) -> Result<Vec<SourceSummary>> {
    let mut response = db
        .query("SELECT name, kind, root_path FROM source ORDER BY name;")
        .await?;
    let rows: Vec<SourceRow> = response.take(0)?;

    let mut summaries = Vec::with_capacity(rows.len());
    for row in rows {
        let id = source_id(&row.name);
        summaries.push(SourceSummary {
            files: count_for_source(db, "file", &id).await?,
            symbols: count_for_source(db, "symbol", &id).await?,
            documents: count_for_source(db, "document", &id).await?,
            name: row.name,
            kind: row.kind,
            root_path: row.root_path,
        });
    }
    Ok(summaries)
}

pub async fn search_symbols(
    db: &Database,
    query: &str,
    limit: usize,
    allowed_sources: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<SymbolHit>> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }

    let mut hits = Vec::new();
    for source in source_names(db).await? {
        if let Some(allowed) = allowed_sources {
            if !allowed.contains(&source) {
                continue;
            }
        }
        let mut response = db
            .query(
                "
                SELECT
                    name, fqn, kind, signature, start_line, end_line,
                    file.path AS file_path
                FROM symbol
                WHERE source = type::thing('source', $source_id)
                  AND (string::lowercase(fqn) CONTAINS $needle
                       OR string::lowercase(name) CONTAINS $needle)
                LIMIT $limit;
                ",
            )
            .bind(("source_id", source_id(&source)))
            .bind(("needle", needle.clone()))
            .bind(("limit", limit as i64))
            .await?;
        let rows: Vec<SymbolRow> = response.take(0)?;
        hits.extend(rows.into_iter().map(|row| SymbolHit {
            source: source.clone(),
            name: row.name,
            fqn: row.fqn,
            kind: row.kind,
            signature: row.signature,
            file_path: row.file_path.unwrap_or_default(),
            start_line: row.start_line,
            end_line: row.end_line,
        }));
    }

    hits.sort_by_key(|hit| (hit.fqn.len(), hit.fqn.clone()));
    hits.truncate(limit);
    Ok(hits)
}

pub async fn search_documents(
    db: &Database,
    query: &str,
    limit: usize,
    allowed_sources: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<DocumentHit>> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }

    let mut hits = Vec::new();
    for source in source_names(db).await? {
        if let Some(allowed) = allowed_sources {
            if !allowed.contains(&source) {
                continue;
            }
        }
        let mut response = db
            .query(
                "
                SELECT path, title, tags, body
                FROM document
                WHERE source = type::thing('source', $source_id)
                  AND (string::lowercase(title) CONTAINS $needle
                       OR string::lowercase(body) CONTAINS $needle)
                LIMIT $limit;
                ",
            )
            .bind(("source_id", source_id(&source)))
            .bind(("needle", needle.clone()))
            .bind(("limit", limit as i64))
            .await?;
        let rows: Vec<DocumentRow> = response.take(0)?;
        hits.extend(rows.into_iter().map(|row| DocumentHit {
            source: source.clone(),
            excerpt: excerpt_around(&row.body, &needle),
            path: row.path,
            title: row.title,
            tags: row.tags,
        }));
    }

    hits.truncate(limit);
    Ok(hits)
}

pub async fn neighbors(
    db: &Database,
    address: &NodeAddress,
    limit: usize,
) -> Result<Vec<NeighborEdge>> {
    let record = format!("{}:{}", address.kind.table(), address.record_id());
    let mut edges = Vec::new();

    for table in RELATION_TABLES {
        let sql = format!(
            "SELECT in, out, confidence FROM {table}
             WHERE in = type::thing($node_table, $node_id)
                OR out = type::thing($node_table, $node_id)
             LIMIT $limit;"
        );
        let mut response = db
            .query(sql)
            .bind(("node_table", address.kind.table().to_string()))
            .bind(("node_id", address.record_id()))
            .bind(("limit", limit as i64))
            .await?;
        let rows: Vec<EdgeRow> = response.take(0)?;

        for row in rows {
            let from = format!("{}:{}", row.from.table(), row.from.key());
            let (direction, other) = if from == record {
                (Direction::Outgoing, row.to)
            } else {
                (Direction::Incoming, row.from)
            };
            if let Some((node, label)) = describe_node(db, &other).await? {
                edges.push(NeighborEdge {
                    relation: table.to_string(),
                    direction,
                    node,
                    label,
                    confidence: row.confidence,
                });
            }
        }
    }

    edges.truncate(limit);
    Ok(edges)
}

async fn describe_node(
    db: &Database,
    record: &surrealdb::RecordId,
) -> Result<Option<(NodeAddress, String)>> {
    let Some(kind) = NodeKind::parse(record.table()) else {
        return Ok(None);
    };
    let sql = match kind {
        NodeKind::Decision => {
            "SELECT title FROM architecture_decision WHERE id = type::thing($table, $id) LIMIT 1;"
                .to_string()
        }
        _ => format!(
            "SELECT name, fqn, path, title, source.name AS source_name
             FROM {} WHERE id = type::thing($table, $id) LIMIT 1;",
            kind.table()
        ),
    };
    let mut response = db
        .query(sql)
        .bind(("table", record.table().to_string()))
        .bind(("id", record.key().to_string()))
        .await?;

    let rows: Vec<NodeInfoRow> = response.take(0)?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let key = match kind {
        NodeKind::Symbol => row.fqn.unwrap_or_default(),
        NodeKind::File | NodeKind::Document => row.path.unwrap_or_default(),
        NodeKind::Decision => row
            .title
            .clone()
            .or_else(|| row.name.clone())
            .unwrap_or_default(),
    };
    if key.is_empty() {
        return Ok(None);
    }
    let label = match kind {
        NodeKind::Decision => row.title.unwrap_or_else(|| key.clone()),
        _ => row.title.or(row.name).unwrap_or_else(|| key.clone()),
    };

    Ok(Some((
        NodeAddress {
            kind,
            source: row.source_name.unwrap_or_else(|| {
                if kind == NodeKind::Decision {
                    "system".into()
                } else {
                    String::new()
                }
            }),
            key,
        },
        label,
    )))
}

async fn source_names(db: &Database) -> Result<Vec<String>> {
    #[derive(Debug, Deserialize)]
    struct NameRow {
        name: String,
    }

    let mut response = db.query("SELECT name FROM source ORDER BY name;").await?;
    let rows: Vec<NameRow> = response.take(0)?;
    Ok(rows.into_iter().map(|row| row.name).collect())
}

async fn count_for_source(db: &Database, table: &str, source_id: &str) -> Result<i64> {
    let sql = format!(
        "SELECT count() AS count FROM {table}
         WHERE source = type::thing('source', $source_id) GROUP ALL;"
    );
    let mut response = db
        .query(sql)
        .bind(("source_id", source_id.to_string()))
        .await?;
    let rows: Vec<CountRow> = response.take(0)?;
    Ok(rows.first().map_or(0, |row| row.count))
}

fn excerpt_around(body: &str, needle: &str) -> String {
    const WINDOW: usize = 160;
    let lowered = body.to_ascii_lowercase();
    let start = lowered
        .find(needle)
        .map(|index| index.saturating_sub(WINDOW / 2))
        .unwrap_or(0);
    let start = floor_char_boundary(body, start);
    let end = ceil_char_boundary(body, (start + WINDOW).min(body.len()));
    let mut excerpt = body[start..end].replace('\n', " ").trim().to_string();
    if end < body.len() {
        excerpt.push('…');
    }
    excerpt
}

fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_char_boundary(value: &str, mut index: usize) -> usize {
    while index < value.len() && !value.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_node_tokens() {
        let address = NodeAddress {
            kind: NodeKind::Symbol,
            source: "code".into(),
            key: "Services::Greeter".into(),
        };
        let token = address.to_token();
        assert_eq!(token, "symbol:code:Services::Greeter");
        assert_eq!(NodeAddress::parse_token(&token), Some(address));
        assert_eq!(NodeAddress::parse_token("bogus"), None);
    }

    #[test]
    fn builds_excerpt_around_needle() {
        let body = "alpha beta gamma delta";
        assert!(excerpt_around(body, "gamma").contains("gamma"));
    }
}
