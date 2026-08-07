use std::sync::Arc;

use codebrain_core::{
    AddDecisionRequest, Config, FusionWeights, PromoteMentionRequest, QueryBudget,
    add_architectural_decision, explore_context, neighborhood, promote_mention_edge,
    semantic_search, sources, symbols,
};
use codebrain_db::{Database, NodeAddress, collect_status};
use codebrain_embed::Embedder;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ListResourcesResult, ProtocolVersion,
    ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
    ResourceContents, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, tool, tool_handler, tool_router};
use serde::Deserialize;

const STATUS_URI: &str = crate::legend::STATUS_URI;
const SCHEMA_URI: &str = crate::legend::SCHEMA_URI;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListSourcesArgs {
    /// Optional filter by source kind and/or source name:
    /// `git_repo`, `obsidian_vault`, `jira`, `confluence`, `notion`, or a configured source name.
    #[serde(default)]
    pub source_kinds: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchSymbolsArgs {
    /// Substring matched against symbol name and fully-qualified name.
    pub query: String,
    /// Maximum hits to return (clamped by the server budget).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional filter by source kind and/or source name.
    #[serde(default)]
    pub source_kinds: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExploreContextArgs {
    /// Free-text topic, symbol, or note title to gather context for.
    pub query: String,
    /// Maximum hits per channel (clamped by the server budget).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional filter by source kind and/or source name.
    #[serde(default)]
    pub source_kinds: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphNeighborsArgs {
    /// Node token in `kind:source:key` form, e.g. `symbol:code:Services::Greeter`.
    pub node: String,
    /// Hops to expand (clamped by the server budget).
    #[serde(default)]
    pub depth: Option<usize>,
    /// Maximum nodes to return (clamped by the server budget).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SemanticSearchArgs {
    /// Natural-language query (cross-language when embeddings are enabled).
    pub query: String,
    /// Maximum hits to return (clamped by the server budget).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional filter by source kind and/or source name.
    #[serde(default)]
    pub source_kinds: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddArchitecturalDecisionArgs {
    /// Short decision title (also used as the stable ADR id).
    pub title: String,
    /// Rationale and context for the decision.
    pub body: String,
    /// Node tokens this decision is ABOUT (`symbol:…`, `file:…`, `document:…`).
    #[serde(default)]
    pub about: Vec<String>,
    /// When set, overrides `[adr].write_vault` for this call.
    #[serde(default)]
    pub write_vault: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PromoteMentionArgs {
    /// Document token that owns the mention, e.g. `document:notes:Design.md`.
    pub document: String,
    /// Symbol token that was mentioned, e.g. `symbol:code:Services::Greeter`.
    pub symbol: String,
}

/// MCP surface over an already-indexed CodeBrain graph.
#[derive(Clone)]
pub struct CodeBrainServer {
    db: Arc<Database>,
    config: Arc<Config>,
    budget: QueryBudget,
    embedder: Arc<dyn Embedder>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CodeBrainServer {
    pub fn new(
        db: Database,
        config: Config,
        budget: QueryBudget,
        embedder: Arc<dyn Embedder>,
    ) -> Self {
        Self {
            db: Arc::new(db),
            config: Arc::new(config),
            budget,
            embedder,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "List indexed sources (code, vaults, Jira, Confluence, Notion) with counts. Optional source_kinds filter: git_repo|obsidian_vault|jira|confluence|notion or a source name."
    )]
    pub async fn list_sources(
        &self,
        Parameters(args): Parameters<ListSourcesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let kinds = args.source_kinds.as_deref();
        let summaries = sources(&self.db, kinds).await.map_err(internal)?;
        json_result(&summaries)
    }

    #[tool(
        description = "Search indexed code symbols by name or fully-qualified name. Returns file paths and line ranges. Optional source_kinds filter."
    )]
    pub async fn search_symbols(
        &self,
        Parameters(args): Parameters<SearchSymbolsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let hits = symbols(
            &self.db,
            &args.query,
            self.budget,
            args.limit,
            args.source_kinds.as_deref(),
        )
        .await
        .map_err(internal)?;
        json_result(&hits)
    }

    #[tool(
        description = "Gather cross-channel context for a topic: matching code symbols, related notes, and the graph edges around them. Optional source_kinds filter."
    )]
    pub async fn explore_context(
        &self,
        Parameters(args): Parameters<ExploreContextArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bundle = explore_context(
            &self.db,
            &args.query,
            self.budget,
            args.limit,
            args.source_kinds.as_deref(),
        )
        .await
        .map_err(internal)?;
        json_result(&bundle)
    }

    #[tool(
        description = "Expand the graph around a node token (`symbol:source:fqn`, `file:source:path`, `document:source:path`, or `decision:system:title`)."
    )]
    pub async fn graph_neighbors(
        &self,
        Parameters(args): Parameters<GraphNeighborsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let address = NodeAddress::parse_token(&args.node).ok_or_else(|| {
            McpError::invalid_params(
                "node must look like kind:source:key, e.g. symbol:code:Services::Greeter",
                None,
            )
        })?;
        let graph = neighborhood(&self.db, &address, self.budget, args.depth, args.limit)
            .await
            .map_err(internal)?;
        json_result(&graph)
    }

    #[tool(
        description = "Hybrid semantic search over code + notes. Uses embeddings when configured; otherwise falls back to full-text search without failing."
    )]
    pub async fn semantic_search(
        &self,
        Parameters(args): Parameters<SemanticSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let result = semantic_search(
            &self.db,
            &args.query,
            &self.embedder,
            self.budget,
            args.limit,
            FusionWeights::default(),
            args.source_kinds.as_deref(),
        )
        .await
        .map_err(internal)?;
        json_result(&result)
    }

    #[tool(
        description = "Record an architectural decision in the graph (ABOUT edges to symbols/files/notes). Optionally write a Markdown ADR into the configured Obsidian vault when write_vault is true."
    )]
    pub async fn add_architectural_decision(
        &self,
        Parameters(args): Parameters<AddArchitecturalDecisionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut about = Vec::with_capacity(args.about.len());
        for token in &args.about {
            let address = NodeAddress::parse_token(token).ok_or_else(|| {
                McpError::invalid_params(
                    format!("about entry must look like kind:source:key, got {token:?}"),
                    None,
                )
            })?;
            about.push(address);
        }

        let result = add_architectural_decision(
            &self.db,
            &self.config,
            AddDecisionRequest {
                title: args.title,
                body: args.body,
                about,
                write_vault: args.write_vault,
            },
        )
        .await
        .map_err(internal)?;
        json_result(&result)
    }

    #[tool(
        description = "Promote a document→symbol MENTIONS edge into a stronger EXPLAINS edge after human/agent review. Requires an existing mentions link."
    )]
    pub async fn promote_mention(
        &self,
        Parameters(args): Parameters<PromoteMentionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let document = NodeAddress::parse_token(&args.document).ok_or_else(|| {
            McpError::invalid_params("document must look like document:source:path", None)
        })?;
        let symbol = NodeAddress::parse_token(&args.symbol).ok_or_else(|| {
            McpError::invalid_params("symbol must look like symbol:source:fqn", None)
        })?;
        let result = promote_mention_edge(&self.db, PromoteMentionRequest { document, symbol })
            .await
            .map_err(internal)?;
        json_result(&result)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CodeBrainServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new("codebrain", env!("CARGO_PKG_VERSION"))
                .with_title("CodeBrain")
                .with_description(
                    "Local knowledge graph unifying code, Obsidian, and SaaS documents",
                ),
        )
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_instructions(
            "CodeBrain exposes a local knowledge graph unifying source code, Obsidian notes, and SaaS (Jira/Confluence/Notion). \
             Prefer semantic_search for natural-language questions, explore_context for a topic bundle, \
             then graph_neighbors to walk edges (defines, calls, imports, references, mentions, explains, about, resolves). \
             Pass source_kinds to restrict channels (e.g. [\"jira\"] or [\"git_repo\"]). \
             Use add_architectural_decision to capture ADRs (write_vault defaults from config, false never touches the vault). \
             Use promote_mention to upgrade reviewed MENTIONS into EXPLAINS. \
             Read resource codebrain://schema for node/edge legend; codebrain://status for DB counts. \
             Node tokens are kind:source:key."
                .to_string(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                Resource::new(STATUS_URI, "codebrain-status".to_string()),
                Resource::new(SCHEMA_URI, "codebrain-schema".to_string()),
            ],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let text = match request.uri.as_str() {
            STATUS_URI => {
                let status = collect_status(&self.db).await.map_err(internal)?;
                serde_json::to_string_pretty(&status).map_err(internal)?
            }
            SCHEMA_URI => {
                serde_json::to_string_pretty(&crate::legend::graph_legend()).map_err(internal)?
            }
            _ => {
                return Err(McpError::resource_not_found(
                    "unknown resource",
                    Some(serde_json::json!({ "uri": request.uri })),
                ));
            }
        };
        Ok(ReadResourceResult::new(vec![ResourceContents::text(text, request.uri)]).into())
    }
}

fn json_result<T: serde::Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(value).map_err(internal)?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}
