//! Contract smoke test: a real MCP client must be able to list and call every tool.
//!
//! Integration tests compile as their own crate, so clippy's `allow-*-in-tests`
//! settings (which key off `#[cfg(test)]`) do not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use chrono::Utc;
use codebrain_connector::{DocumentNode, ExtractBatch, FileNode, SymbolNode};
use codebrain_core::{Config, QueryBudget};
use codebrain_db::{
    Database, apply_schema, open_memory, persist_code_batch, persist_document_batch, relate_mention,
};
use codebrain_mcp::CodeBrainServer;
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;

async fn seeded_db() -> Database {
    let db = open_memory().await.expect("open db");
    apply_schema(&db).await.expect("schema");

    let code = ExtractBatch {
        files: vec![FileNode {
            path: "services/greeter.rb".into(),
            language: Some("ruby".into()),
            content_hash: "hash-file".into(),
            mtime: Utc::now(),
        }],
        symbols: vec![SymbolNode {
            file_path: "services/greeter.rb".into(),
            name: "Greeter".into(),
            fqn: "Services::Greeter".into(),
            kind: "class".into(),
            signature: Some("class Greeter".into()),
            start_line: 1,
            end_line: 8,
            content_hash: "hash-symbol".into(),
        }],
        ..ExtractBatch::default()
    };
    persist_code_batch(&db, "code", "/tmp/code", &code)
        .await
        .expect("persist code");

    let notes = ExtractBatch {
        documents: vec![DocumentNode {
            path: "Design.md".into(),
            title: "Greeter design".into(),
            aliases: Vec::new(),
            tags: vec!["design".into()],
            body: "The Greeter class owns greeting copy.".into(),
            content_hash: "hash-doc".into(),
            updated_at: Utc::now(),
        }],
        ..ExtractBatch::default()
    };
    persist_document_batch(&db, "notes", "/tmp/notes", &notes)
        .await
        .expect("persist notes");

    relate_mention(
        &db,
        "notes",
        "Design.md",
        "code",
        "Services::Greeter",
        0.9,
        Some("Greeter"),
    )
    .await
    .expect("relate mention");

    db
}

use codebrain_embed::{EmbedderConfig, EmbedderKind, build_embedder};

async fn connect() -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let db = seeded_db().await;
    let embedder = build_embedder(&EmbedderConfig {
        kind: EmbedderKind::None,
        ..EmbedderConfig::default()
    })
    .expect("none embedder");
    let server = CodeBrainServer::new(db, Config::default(), QueryBudget::default(), embedder);
    let (client_io, server_io) = tokio::io::duplex(1 << 16);

    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_io).await {
            let _ = running.waiting().await;
        }
    });

    ().serve(client_io).await.expect("client handshake")
}

fn tool_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn lists_every_tool_on_connect() {
    let client = connect().await;
    let tools = client
        .list_tools(Default::default())
        .await
        .expect("list tools");

    let names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    for expected in [
        "list_sources",
        "search_symbols",
        "explore_context",
        "graph_neighbors",
        "semantic_search",
        "add_architectural_decision",
        "promote_mention",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool {expected}: {names:?}"
        );
    }

    client.cancel().await.expect("shutdown");
}

#[tokio::test]
async fn explore_context_returns_code_and_notes() {
    let client = connect().await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("explore_context")
                .with_arguments(rmcp::object!({ "query": "Greeter" })),
        )
        .await
        .expect("call explore_context");

    let text = tool_text(&result);
    assert!(text.contains("Services::Greeter"), "missing symbol: {text}");
    assert!(text.contains("Greeter design"), "missing note: {text}");
    assert!(text.contains("mentions"), "missing mention edge: {text}");

    client.cancel().await.expect("shutdown");
}

#[tokio::test]
async fn graph_neighbors_rejects_malformed_node_token() {
    let client = connect().await;
    let error = client
        .call_tool(
            CallToolRequestParams::new("graph_neighbors")
                .with_arguments(rmcp::object!({ "node": "not-a-token" })),
        )
        .await
        .expect_err("malformed token must fail");

    assert!(
        error.to_string().contains("kind:source:key"),
        "error should explain the token format: {error}"
    );

    client.cancel().await.expect("shutdown");
}

#[tokio::test]
async fn semantic_search_fts_fallback_on_connect() {
    let client = connect().await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("semantic_search")
                .with_arguments(rmcp::object!({ "query": "Greeter" })),
        )
        .await
        .expect("call semantic_search");

    let text = tool_text(&result);
    assert!(
        text.contains("\"mode\": \"fts\""),
        "expected fts mode: {text}"
    );
    assert!(
        text.contains("Services::Greeter") || text.contains("Greeter design"),
        "{text}"
    );

    client.cancel().await.expect("shutdown");
}

#[tokio::test]
async fn add_architectural_decision_links_about_and_skips_vault() {
    let client = connect().await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("add_architectural_decision").with_arguments(
                rmcp::object!({
                    "title": "Prefer Greeter facade",
                    "body": "All greeting copy goes through Greeter.",
                    "about": ["symbol:code:Services::Greeter"],
                    "write_vault": false
                }),
            ),
        )
        .await
        .expect("call add_architectural_decision");

    let created = tool_text(&result);
    assert!(created.contains("Prefer Greeter facade"), "{created}");
    assert!(created.contains("\"vault_written\": false"), "{created}");

    let neighbors = client
        .call_tool(
            CallToolRequestParams::new("graph_neighbors").with_arguments(rmcp::object!({
                "node": "symbol:code:Services::Greeter",
                "depth": 1
            })),
        )
        .await
        .expect("call graph_neighbors");
    let text = tool_text(&neighbors);
    assert!(text.contains("about"), "missing about edge: {text}");
    assert!(
        text.contains("Prefer Greeter facade") || text.contains("decision:system:"),
        "missing ADR neighbor: {text}"
    );

    client.cancel().await.expect("shutdown");
}

#[tokio::test]
async fn promote_mention_creates_explains_edge() {
    let client = connect().await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("promote_mention").with_arguments(rmcp::object!({
                "document": "document:notes:Design.md",
                "symbol": "symbol:code:Services::Greeter"
            })),
        )
        .await
        .expect("call promote_mention");

    let text = tool_text(&result);
    assert!(text.contains("\"relation\": \"explains\""), "{text}");
    assert!(text.contains("Services::Greeter"), "{text}");

    let neighbors = client
        .call_tool(
            CallToolRequestParams::new("graph_neighbors").with_arguments(rmcp::object!({
                "node": "symbol:code:Services::Greeter",
                "depth": 1
            })),
        )
        .await
        .expect("neighbors");
    let graph = tool_text(&neighbors);
    assert!(graph.contains("explains"), "missing explains edge: {graph}");

    client.cancel().await.expect("shutdown");
}

#[tokio::test]
async fn exposes_status_and_schema_resources() {
    let client = connect().await;
    let resources = client
        .list_resources(Default::default())
        .await
        .expect("list resources");
    let uris: Vec<&str> = resources
        .resources
        .iter()
        .map(|resource| resource.uri.as_str())
        .collect();
    assert!(uris.contains(&"codebrain://status"));
    assert!(uris.contains(&"codebrain://schema"));

    let status = client
        .read_resource(rmcp::model::ReadResourceRequestParams::new(
            "codebrain://status",
        ))
        .await
        .expect("read status");
    assert!(!status.contents.is_empty());

    let schema = client
        .read_resource(rmcp::model::ReadResourceRequestParams::new(
            "codebrain://schema",
        ))
        .await
        .expect("read schema");
    let schema_text = schema
        .contents
        .iter()
        .filter_map(|block| match block {
            rmcp::model::ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        schema_text.contains("resolves"),
        "missing resolves: {schema_text}"
    );
    assert!(
        schema_text.contains("kind:source:key"),
        "missing token format: {schema_text}"
    );

    client.cancel().await.expect("shutdown");
}
