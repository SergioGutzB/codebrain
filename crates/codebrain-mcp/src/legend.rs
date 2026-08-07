//! Static graph legend for MCP agents (`codebrain://schema`).

use serde::Serialize;

pub const SCHEMA_URI: &str = "codebrain://schema";
pub const STATUS_URI: &str = "codebrain://status";

#[derive(Debug, Serialize)]
pub struct GraphLegend {
    pub version: &'static str,
    pub node_token_format: &'static str,
    pub node_kinds: Vec<NodeKindLegend>,
    pub edge_types: Vec<EdgeLegend>,
    pub source_kinds: Vec<&'static str>,
    pub docs: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct NodeKindLegend {
    pub kind: &'static str,
    pub token_example: &'static str,
    pub meaning: &'static str,
}

#[derive(Debug, Serialize)]
pub struct EdgeLegend {
    pub relation: &'static str,
    pub from: &'static str,
    pub to: &'static str,
    pub meaning: &'static str,
}

pub fn graph_legend() -> GraphLegend {
    GraphLegend {
        version: env!("CARGO_PKG_VERSION"),
        node_token_format: "kind:source:key",
        node_kinds: vec![
            NodeKindLegend {
                kind: "symbol",
                token_example: "symbol:backend:Services::Greeter",
                meaning: "Code symbol (class, function, module, …)",
            },
            NodeKindLegend {
                kind: "file",
                token_example: "file:backend:app/models/plan.rb",
                meaning: "Source file in a git_repo",
            },
            NodeKindLegend {
                kind: "document",
                token_example: "document:notes:Design.md",
                meaning: "Note, Confluence/Notion page, or Jira issue (path = key or page id)",
            },
            NodeKindLegend {
                kind: "decision",
                token_example: "decision:system:Prefer Greeter facade",
                meaning: "Architectural decision record",
            },
            NodeKindLegend {
                kind: "chunk",
                token_example: "(internal embedding unit)",
                meaning: "Embedded text window for semantic search",
            },
        ],
        edge_types: vec![
            EdgeLegend {
                relation: "defines",
                from: "file",
                to: "symbol",
                meaning: "File defines this symbol",
            },
            EdgeLegend {
                relation: "calls",
                from: "symbol",
                to: "symbol",
                meaning: "Call edge from AST",
            },
            EdgeLegend {
                relation: "imports",
                from: "symbol",
                to: "symbol",
                meaning: "Import / require edge",
            },
            EdgeLegend {
                relation: "references",
                from: "document",
                to: "document",
                meaning: "Wikilink or SaaS doc citing another doc (e.g. Confluence→Jira)",
            },
            EdgeLegend {
                relation: "mentions",
                from: "document",
                to: "symbol",
                meaning: "Document body cites a symbol name/FQN",
            },
            EdgeLegend {
                relation: "explains",
                from: "document",
                to: "symbol",
                meaning: "Promoted mention after review",
            },
            EdgeLegend {
                relation: "about",
                from: "decision",
                to: "symbol|file|document",
                meaning: "ADR concerns this node",
            },
            EdgeLegend {
                relation: "resolves",
                from: "symbol",
                to: "document",
                meaning: "Issue key in source file → Jira ticket document",
            },
        ],
        source_kinds: vec!["git_repo", "obsidian_vault", "jira", "confluence", "notion"],
        docs: vec!["docs/KNOWLEDGE_GRAPH.md", "docs/MCP.md", "docs/BACKLOG.md"],
    }
}
