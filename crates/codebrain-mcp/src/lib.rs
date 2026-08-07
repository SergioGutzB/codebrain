//! MCP server exposing the CodeBrain graph to Cursor / Claude Code over stdio or HTTP.

mod legend;
mod server;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, bail};
use codebrain_core::{
    Config, QueryBudget, embedder_from_config, run_reindex_worker, spawn_watchers,
};
use codebrain_db::{apply_schema, open_embedded};
use codebrain_embed::Embedder;
use rmcp::ServiceExt;
use rmcp::transport::stdio;
use tokio::sync::mpsc;

pub use legend::{SCHEMA_URI, STATUS_URI, graph_legend};
pub use server::CodeBrainServer;

/// Start the MCP server using the configured transport and block until shutdown.
pub async fn serve(config: &Config) -> anyhow::Result<()> {
    let db_path = config.database.resolved_path();
    let embedder = embedder_from_config(config)?;
    match config.mcp.transport.as_str() {
        "stdio" => serve_stdio(config, &db_path, QueryBudget::default(), embedder).await,
        "http" | "streamable_http" => {
            serve_http(config, &db_path, QueryBudget::default(), embedder).await
        }
        other => bail!("unsupported mcp.transport {other:?}; expected \"stdio\" or \"http\""),
    }
}

async fn open_ready_db(db_path: &Path) -> anyhow::Result<codebrain_db::Database> {
    let db = open_embedded(db_path).await?;
    apply_schema(&db).await?;
    Ok(db)
}

async fn maybe_spawn_watchers(
    config: &Config,
    db: &codebrain_db::Database,
) -> anyhow::Result<Vec<tokio::task::JoinHandle<()>>> {
    let mut watch_handles = Vec::new();
    if config.index.watch {
        let (job_tx, job_rx) = mpsc::channel(64);
        let worker_db = db.clone();
        let worker_config = config.clone();
        watch_handles.push(tokio::spawn(async move {
            run_reindex_worker(worker_db, worker_config, job_rx).await;
        }));
        watch_handles.push(spawn_watchers(config.clone(), job_tx)?);
        tracing::info!("background watch + partial reindex enabled");
    }
    Ok(watch_handles)
}

async fn serve_stdio(
    config: &Config,
    db_path: &Path,
    budget: QueryBudget,
    embedder: Arc<dyn Embedder>,
) -> anyhow::Result<()> {
    tracing::info!(
        path = %db_path.display(),
        provider = embedder.kind(),
        watch = config.index.watch,
        "starting CodeBrain MCP server (stdio)"
    );
    let db = open_ready_db(db_path).await?;
    let watch_handles = maybe_spawn_watchers(config, &db).await?;

    let server = CodeBrainServer::new(db, config.clone(), budget, embedder);
    let running = server.serve(stdio()).await?;
    running.waiting().await?;

    for handle in watch_handles {
        handle.abort();
    }
    Ok(())
}

async fn serve_http(
    config: &Config,
    db_path: &Path,
    budget: QueryBudget,
    embedder: Arc<dyn Embedder>,
) -> anyhow::Result<()> {
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };

    let bind: SocketAddr = config
        .mcp
        .bind
        .parse()
        .with_context(|| format!("invalid mcp.bind address {:?}", config.mcp.bind))?;
    ensure_bind_allowed(config, bind)?;

    tracing::info!(
        path = %db_path.display(),
        provider = embedder.kind(),
        watch = config.index.watch,
        %bind,
        "starting CodeBrain MCP server (streamable HTTP)"
    );

    let db = open_ready_db(db_path).await?;
    let watch_handles = maybe_spawn_watchers(config, &db).await?;

    let server_template = CodeBrainServer::new(db, config.clone(), budget, embedder);
    let service = StreamableHttpService::new(
        move || Ok(server_template.clone()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("bind MCP HTTP on {bind}"))?;
    tracing::info!(url = %format!("http://{bind}/mcp"), "MCP streamable HTTP listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down HTTP MCP server");
        })
        .await
        .context("HTTP MCP server failed")?;

    for handle in watch_handles {
        handle.abort();
    }
    Ok(())
}

fn ensure_bind_allowed(config: &Config, bind: SocketAddr) -> anyhow::Result<()> {
    if config.mcp.allow_remote {
        return Ok(());
    }
    if bind.ip().is_loopback() {
        return Ok(());
    }
    bail!(
        "refusing non-loopback MCP bind {bind}; set mcp.allow_remote = true only on trusted networks"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use codebrain_core::McpConfig;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn rejects_remote_bind_by_default() {
        let config = Config {
            mcp: McpConfig {
                transport: "http".into(),
                bind: "0.0.0.0:8765".into(),
                allow_remote: false,
            },
            ..Config::default()
        };
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8765);
        assert!(ensure_bind_allowed(&config, addr).is_err());
    }

    #[test]
    fn allows_loopback_bind() {
        let config = Config::default();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8765);
        assert!(ensure_bind_allowed(&config, addr).is_ok());
    }
}
