use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use codebrain_core::{
    CheckStatus, Config, default_config_path, index_configured_sources, load_config, run_doctor,
    run_reindex_worker, spawn_watchers,
};
use codebrain_db::{apply_schema, collect_status, open_embedded};

#[derive(Debug, Parser)]
#[command(
    name = "codebrain",
    version,
    about = "Omnichannel knowledge graph for code + docs, exposed via MCP"
)]
struct Cli {
    /// Path to codebrain.toml (defaults to platform config dir or ./codebrain.toml)
    #[arg(short, long, global = true, env = "CODEBRAIN_CONFIG")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create data directory, write example config if missing, and apply schema
    Init {
        /// Overwrite an existing config file with the example template
        #[arg(long)]
        force_config: bool,
    },
    /// Run environment and database health checks
    Doctor {
        /// Apply schema migrations if the database opens successfully
        #[arg(long)]
        migrate: bool,
    },
    /// Show database status and table counts
    Status,
    /// Index configured sources (Phase 1+)
    Index {
        /// Limit indexing to a single named source from config
        #[arg(long)]
        source: Option<String>,
        /// Reindex everything even when content hashes match (needed after enabling embeddings)
        #[arg(long)]
        force: bool,
    },
    /// Start the MCP server on stdio (for Cursor / Claude Code)
    Serve,
    /// Watch configured sources and partially reindex on change
    Watch,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let config_path = resolve_config_path(cli.config.as_deref());

    match cli.command {
        Commands::Init { force_config } => cmd_init(&config_path, force_config).await?,
        Commands::Doctor { migrate } => cmd_doctor(&config_path, migrate).await?,
        Commands::Status => cmd_status(&config_path).await?,
        Commands::Index { source, force } => {
            cmd_index(&config_path, source.as_deref(), force).await?
        }
        Commands::Serve => {
            let config = load_config_or_default(&config_path)?;
            codebrain_mcp::serve(&config).await?;
        }
        Commands::Watch => cmd_watch(&config_path).await?,
    }

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        // `codebrain serve` speaks MCP over stdout; diagnostics must never share that channel.
        .with_writer(std::io::stderr)
        .init();
}

fn resolve_config_path(explicit: Option<&std::path::Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    let cwd = PathBuf::from("codebrain.toml");
    if cwd.exists() {
        return cwd;
    }
    default_config_path()
}

async fn cmd_init(config_path: &std::path::Path, force_config: bool) -> anyhow::Result<()> {
    let example = include_str!("../../../codebrain.example.toml");

    if let Some(parent) = config_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    if force_config || !config_path.exists() {
        tokio::fs::write(config_path, example).await?;
        println!("wrote config template → {}", config_path.display());
    } else {
        println!("config already exists → {}", config_path.display());
    }

    let config = load_config(Some(config_path))?;
    let db_path = config.database.resolved_path();
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let db = open_embedded(&db_path).await?;
    apply_schema(&db).await?;
    println!("database ready → {}", db_path.display());
    println!("schema v{} applied", codebrain_db::SCHEMA_VERSION);
    println!();
    println!("Next:");
    println!("  1. Edit {} and add your sources", config_path.display());
    println!("  2. Run: codebrain doctor");
    println!("  3. Run: codebrain index");
    Ok(())
}

async fn cmd_doctor(config_path: &std::path::Path, migrate: bool) -> anyhow::Result<()> {
    let config = load_config_or_default(config_path)?;
    let report = run_doctor(&config, migrate).await?;

    for check in &report.checks {
        let mark = match check.status {
            CheckStatus::Ok => "ok  ",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "FAIL",
        };
        println!("[{mark}] {:<22} {}", check.name, check.detail);
    }

    if report.healthy {
        println!("\ndoctor: healthy");
        Ok(())
    } else {
        anyhow::bail!("doctor: unhealthy — fix FAIL checks above")
    }
}

async fn cmd_status(config_path: &std::path::Path) -> anyhow::Result<()> {
    let config = load_config_or_default(config_path)?;
    let db = open_embedded(config.database.resolved_path()).await?;
    let status = collect_status(&db).await?;

    println!(
        "schema: recorded={:?} expected={} ok={}",
        status.schema_version, status.expected_schema_version, status.schema_ok
    );
    println!("tables:");
    for table in status.tables {
        println!("  {:>24}  {:>8}", table.table, table.count);
    }
    Ok(())
}

async fn cmd_index(
    config_path: &std::path::Path,
    source: Option<&str>,
    force: bool,
) -> anyhow::Result<()> {
    let config = load_config_or_default(config_path)?;
    let db = open_embedded(config.database.resolved_path()).await?;
    apply_schema(&db).await?;
    let report = index_configured_sources(&db, &config, source, force).await?;

    for source in &report.sources {
        println!(
            "{}: discovered={} indexed={} skipped={} removed={} symbols={} imports={} calls={} documents={} references={} mentions={} explains={} resolves={} broken_links={} chunks={}",
            source.source,
            source.discovered,
            source.indexed,
            source.skipped,
            source.removed,
            source.symbols,
            source.imports,
            source.calls,
            source.documents,
            source.references,
            source.mentions,
            source.explains,
            source.resolves,
            source.broken_links,
            source.chunks
        );
    }
    println!(
        "total: discovered={} indexed={} skipped={}",
        report.discovered(),
        report.indexed(),
        report.skipped()
    );
    Ok(())
}

async fn cmd_watch(config_path: &std::path::Path) -> anyhow::Result<()> {
    let config = load_config_or_default(config_path)?;
    let db_path = config.database.resolved_path();
    let db = open_embedded(&db_path).await?;
    apply_schema(&db).await?;

    let (job_tx, job_rx) = tokio::sync::mpsc::channel(64);
    let worker = tokio::spawn(run_reindex_worker(db, config.clone(), job_rx));
    let watcher = spawn_watchers(config, job_tx)?;

    tracing::info!("watching sources; Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down watchers");
    watcher.abort();
    worker.abort();
    Ok(())
}

fn load_config_or_default(config_path: &std::path::Path) -> anyhow::Result<Config> {
    if config_path.exists() {
        load_config(Some(config_path))
    } else {
        Ok(Config::default())
    }
}
