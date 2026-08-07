//! Query orchestration behind the MCP tools: search, neighbors, and cross-channel context.

use std::collections::HashSet;

use codebrain_db::{
    Database, DocumentHit, NeighborEdge, NodeAddress, NodeKind, SourceSummary, SymbolHit,
    list_sources, neighbors, search_documents, search_symbols,
};
use serde::{Deserialize, Serialize};

/// Response budget applied to every tool so agents never receive unbounded payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryBudget {
    pub max_nodes: usize,
    pub max_neighbors: usize,
    pub max_depth: usize,
}

impl Default for QueryBudget {
    fn default() -> Self {
        Self {
            max_nodes: 40,
            max_neighbors: 25,
            max_depth: 2,
        }
    }
}

impl QueryBudget {
    pub fn clamp_limit(self, requested: Option<usize>) -> usize {
        requested.unwrap_or(self.max_nodes).clamp(1, self.max_nodes)
    }

    pub fn clamp_depth(self, requested: Option<usize>) -> usize {
        requested.unwrap_or(1).clamp(1, self.max_depth)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeighborhoodNode {
    pub token: String,
    pub kind: NodeKind,
    pub source: String,
    pub label: String,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeighborhoodEdge {
    pub relation: String,
    pub from: String,
    pub to: String,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Neighborhood {
    pub root: String,
    pub nodes: Vec<NeighborhoodNode>,
    pub edges: Vec<NeighborhoodEdge>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextBundle {
    pub query: String,
    pub symbols: Vec<SymbolHit>,
    pub documents: Vec<DocumentHit>,
    pub related: Vec<Neighborhood>,
    pub truncated: bool,
    /// Echo of applied source kind/name filter (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_filter: Option<Vec<String>>,
}

/// Resolve MCP `source_kinds` (kinds and/or source names) to concrete source names.
pub async fn resolve_source_filter(
    db: &Database,
    source_kinds: Option<&[String]>,
) -> anyhow::Result<Option<HashSet<String>>> {
    let Some(raw) = source_kinds else {
        return Ok(None);
    };
    let needles: HashSet<String> = raw
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    if needles.is_empty() {
        return Ok(None);
    }

    let summaries = list_sources(db).await?;
    let allowed: HashSet<String> = summaries
        .into_iter()
        .filter(|summary| {
            needles.contains(&summary.kind.to_ascii_lowercase())
                || needles.contains(&summary.name.to_ascii_lowercase())
        })
        .map(|summary| summary.name)
        .collect();
    Ok(Some(allowed))
}

pub async fn sources(
    db: &Database,
    source_kinds: Option<&[String]>,
) -> anyhow::Result<Vec<SourceSummary>> {
    let mut summaries = list_sources(db).await?;
    if let Some(allowed) = resolve_source_filter(db, source_kinds).await? {
        summaries.retain(|summary| allowed.contains(&summary.name));
    }
    Ok(summaries)
}

pub async fn symbols(
    db: &Database,
    query: &str,
    budget: QueryBudget,
    limit: Option<usize>,
    source_kinds: Option<&[String]>,
) -> anyhow::Result<Vec<SymbolHit>> {
    let allowed = resolve_source_filter(db, source_kinds).await?;
    Ok(search_symbols(db, query, budget.clamp_limit(limit), allowed.as_ref()).await?)
}

pub async fn documents(
    db: &Database,
    query: &str,
    budget: QueryBudget,
    limit: Option<usize>,
    source_kinds: Option<&[String]>,
) -> anyhow::Result<Vec<DocumentHit>> {
    let allowed = resolve_source_filter(db, source_kinds).await?;
    Ok(search_documents(db, query, budget.clamp_limit(limit), allowed.as_ref()).await?)
}

/// Breadth-first expansion around a node, bounded by depth and total node count.
pub async fn neighborhood(
    db: &Database,
    root: &NodeAddress,
    budget: QueryBudget,
    depth: Option<usize>,
    limit: Option<usize>,
) -> anyhow::Result<Neighborhood> {
    let max_depth = budget.clamp_depth(depth);
    let max_nodes = budget.clamp_limit(limit);

    let mut result = Neighborhood {
        root: root.to_token(),
        ..Neighborhood::default()
    };
    let mut seen = vec![root.to_token()];
    result.nodes.push(NeighborhoodNode {
        token: root.to_token(),
        kind: root.kind,
        source: root.source.clone(),
        label: root.key.clone(),
        depth: 0,
    });

    let mut frontier = vec![root.clone()];
    for current_depth in 1..=max_depth {
        let mut next = Vec::new();
        for node in &frontier {
            let edges: Vec<NeighborEdge> = neighbors(db, node, budget.max_neighbors).await?;
            for edge in edges {
                let token = edge.node.to_token();
                result.edges.push(match edge.direction {
                    codebrain_db::Direction::Outgoing => NeighborhoodEdge {
                        relation: edge.relation.clone(),
                        from: node.to_token(),
                        to: token.clone(),
                        confidence: edge.confidence,
                    },
                    codebrain_db::Direction::Incoming => NeighborhoodEdge {
                        relation: edge.relation.clone(),
                        from: token.clone(),
                        to: node.to_token(),
                        confidence: edge.confidence,
                    },
                });

                if seen.contains(&token) {
                    continue;
                }
                if result.nodes.len() >= max_nodes {
                    result.truncated = true;
                    return Ok(result);
                }
                seen.push(token.clone());
                result.nodes.push(NeighborhoodNode {
                    token,
                    kind: edge.node.kind,
                    source: edge.node.source.clone(),
                    label: edge.label,
                    depth: current_depth,
                });
                next.push(edge.node);
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }

    Ok(result)
}

/// Cross-channel entry point: code symbols + notes + the graph around the best hits.
pub async fn explore_context(
    db: &Database,
    query: &str,
    budget: QueryBudget,
    limit: Option<usize>,
    source_kinds: Option<&[String]>,
) -> anyhow::Result<ContextBundle> {
    let limit = budget.clamp_limit(limit);
    let allowed = resolve_source_filter(db, source_kinds).await?;
    let symbol_hits = search_symbols(db, query, limit, allowed.as_ref()).await?;
    let document_hits = search_documents(db, query, limit, allowed.as_ref()).await?;

    // Expand only the strongest hits; the budget is shared across the whole bundle.
    const EXPANDED_ROOTS: usize = 3;
    let roots: Vec<NodeAddress> = symbol_hits
        .iter()
        .take(EXPANDED_ROOTS)
        .map(SymbolHit::address)
        .chain(
            document_hits
                .iter()
                .take(EXPANDED_ROOTS)
                .map(DocumentHit::address),
        )
        .collect();

    let mut related = Vec::with_capacity(roots.len());
    for root in &roots {
        related.push(neighborhood(db, root, budget, Some(1), Some(limit)).await?);
    }

    let truncated = related.iter().any(|entry| entry.truncated)
        || symbol_hits.len() >= limit
        || document_hits.len() >= limit;

    Ok(ContextBundle {
        query: query.to_string(),
        symbols: symbol_hits,
        documents: document_hits,
        related,
        truncated,
        source_filter: source_kinds.map(|kinds| kinds.to_vec()),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use codebrain_db::{apply_schema, open_memory};

    use super::*;
    use crate::{Config, IndexConfig, SourceConfig, SourceKindConfig, index_configured_sources};

    async fn indexed_fixture_db() -> Database {
        let db = open_memory().await.expect("open db");
        apply_schema(&db).await.expect("schema");

        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/repo-mini")
            .canonicalize()
            .expect("repo fixture");
        let vault = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/vault-mini")
            .canonicalize()
            .expect("vault fixture");

        let config = Config {
            sources: HashMap::from([
                (
                    "code".into(),
                    SourceConfig {
                        kind: SourceKindConfig::GitRepo,
                        path: repo.to_string_lossy().into_owned(),
                        languages: vec!["ruby".into()],
                        ..Default::default()
                    },
                ),
                (
                    "notes".into(),
                    SourceConfig {
                        kind: SourceKindConfig::ObsidianVault,
                        path: vault.to_string_lossy().into_owned(),
                        languages: Vec::new(),
                        ..Default::default()
                    },
                ),
            ]),
            index: IndexConfig {
                batch_size: 2,
                ..IndexConfig::default()
            },
            ..Config::default()
        };

        index_configured_sources(&db, &config, Some("code"))
            .await
            .expect("index code");
        index_configured_sources(&db, &config, Some("notes"))
            .await
            .expect("index notes");
        db
    }

    #[tokio::test]
    async fn lists_sources_with_counts() {
        let db = indexed_fixture_db().await;
        let summaries = sources(&db, None).await.expect("sources");

        let code = summaries
            .iter()
            .find(|summary| summary.name == "code")
            .expect("code source");
        let notes = summaries
            .iter()
            .find(|summary| summary.name == "notes")
            .expect("notes source");

        assert_eq!(code.kind, "git_repo");
        assert!(code.symbols > 0);
        assert_eq!(notes.kind, "obsidian_vault");
        assert_eq!(notes.documents, 3);
    }

    #[tokio::test]
    async fn searches_symbols_and_documents() {
        let db = indexed_fixture_db().await;
        let budget = QueryBudget::default();

        let hits = symbols(&db, "Greeter", budget, None, None)
            .await
            .expect("symbol search");
        assert!(hits.iter().any(|hit| hit.name == "Greeter"));
        assert!(hits.iter().all(|hit| !hit.file_path.is_empty()));

        let notes = documents(&db, "Greeter", budget, None, None)
            .await
            .expect("document search");
        assert!(notes.iter().any(|note| note.title == "Note A"));
        assert!(notes.iter().all(|note| !note.excerpt.is_empty()));

        let code_only = documents(
            &db,
            "Greeter",
            budget,
            None,
            Some(&["git_repo".to_string()]),
        )
        .await
        .expect("filtered docs");
        assert!(code_only.is_empty());

        let vault_only = documents(
            &db,
            "Greeter",
            budget,
            None,
            Some(&["obsidian_vault".to_string()]),
        )
        .await
        .expect("vault docs");
        assert!(!vault_only.is_empty());
        assert!(vault_only.iter().all(|note| note.source == "notes"));
    }

    #[tokio::test]
    async fn expands_neighborhood_and_respects_budget() {
        let db = indexed_fixture_db().await;
        let budget = QueryBudget::default();

        let greeter = symbols(&db, "Greeter", budget, None, None)
            .await
            .expect("symbol search")
            .into_iter()
            .find(|hit| hit.name == "Greeter")
            .expect("Greeter symbol");

        let graph = neighborhood(&db, &greeter.address(), budget, Some(1), None)
            .await
            .expect("neighborhood");

        assert_eq!(graph.root, greeter.address().to_token());
        assert!(graph.nodes.len() > 1, "expected neighbors of Greeter");
        assert!(graph.edges.iter().any(|edge| edge.relation == "defines"));

        let tiny = QueryBudget {
            max_nodes: 2,
            ..budget
        };
        let clipped = neighborhood(&db, &greeter.address(), tiny, Some(1), None)
            .await
            .expect("clipped neighborhood");
        assert!(clipped.nodes.len() <= 2);
    }

    #[tokio::test]
    async fn explores_cross_channel_context() {
        let db = indexed_fixture_db().await;
        let bundle = explore_context(&db, "Greeter", QueryBudget::default(), None, None)
            .await
            .expect("context");

        assert_eq!(bundle.query, "Greeter");
        assert!(!bundle.symbols.is_empty(), "expected code hits");
        assert!(!bundle.documents.is_empty(), "expected note hits");
        assert!(
            bundle
                .related
                .iter()
                .any(|entry| entry.edges.iter().any(|edge| edge.relation == "mentions")),
            "expected a documento→symbol mention edge in the expanded graph"
        );
    }

    #[test]
    fn budget_clamps_requested_values() {
        let budget = QueryBudget::default();
        assert_eq!(budget.clamp_limit(None), budget.max_nodes);
        assert_eq!(budget.clamp_limit(Some(0)), 1);
        assert_eq!(budget.clamp_limit(Some(10_000)), budget.max_nodes);
        assert_eq!(budget.clamp_depth(Some(99)), budget.max_depth);
        assert_eq!(budget.clamp_depth(None), 1);
    }
}
