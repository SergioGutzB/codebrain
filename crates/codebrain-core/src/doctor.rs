use std::path::Path;

use serde::Serialize;

use crate::config::Config;
use codebrain_db::{SCHEMA_VERSION, apply_schema, collect_status, open_embedded};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
    pub healthy: bool,
}

impl DoctorReport {
    pub fn push(
        &mut self,
        name: impl Into<String>,
        status: CheckStatus,
        detail: impl Into<String>,
    ) {
        if status == CheckStatus::Fail {
            self.healthy = false;
        }
        self.checks.push(DoctorCheck {
            name: name.into(),
            status,
            detail: detail.into(),
        });
    }
}

/// Run environment health checks for `codebrain doctor`.
pub async fn run_doctor(config: &Config, migrate_if_needed: bool) -> anyhow::Result<DoctorReport> {
    let mut report = DoctorReport {
        checks: Vec::new(),
        healthy: true,
    };

    let db_path = config.database.resolved_path();
    check_db_path(&mut report, &db_path);

    for (name, source) in &config.sources {
        match source.kind {
            crate::config::SourceKindConfig::Jira => match source.atlassian_auth() {
                Ok(auth) => report.push(
                    format!("source:{name}"),
                    CheckStatus::Ok,
                    format!(
                        "jira {} (jql configured={}, max={})",
                        auth.base_url,
                        source.jql.is_some(),
                        source.max_issues
                    ),
                ),
                Err(error) => report.push(
                    format!("source:{name}"),
                    CheckStatus::Warn,
                    format!("jira auth not ready: {error}"),
                ),
            },
            crate::config::SourceKindConfig::Confluence => match source.atlassian_auth() {
                Ok(auth) => report.push(
                    format!("source:{name}"),
                    CheckStatus::Ok,
                    format!(
                        "confluence {} (cql configured={}, max={})",
                        auth.base_url,
                        source.cql.is_some() || source.jql.is_some(),
                        source.max_issues
                    ),
                ),
                Err(error) => report.push(
                    format!("source:{name}"),
                    CheckStatus::Warn,
                    format!("confluence auth not ready: {error}"),
                ),
            },
            crate::config::SourceKindConfig::Notion => match source.notion_auth() {
                Ok(_) => report.push(
                    format!("source:{name}"),
                    CheckStatus::Ok,
                    format!(
                        "notion (query configured={}, max={})",
                        source.query.is_some() || source.jql.is_some(),
                        source.max_issues
                    ),
                ),
                Err(error) => report.push(
                    format!("source:{name}"),
                    CheckStatus::Warn,
                    format!("notion auth not ready: {error}"),
                ),
            },
            _ => {
                let path = source.resolved_path();
                if path.exists() {
                    report.push(
                        format!("source:{name}"),
                        CheckStatus::Ok,
                        format!("{} ({:?})", path.display(), source.kind),
                    );
                } else {
                    report.push(
                        format!("source:{name}"),
                        CheckStatus::Warn,
                        format!("path does not exist yet: {}", path.display()),
                    );
                }
            }
        }
    }

    if config.sources.is_empty() {
        report.push(
            "sources",
            CheckStatus::Warn,
            "no sources configured — copy codebrain.example.toml and add git_repo / obsidian_vault",
        );
    }

    match open_embedded(&db_path).await {
        Ok(db) => {
            report.push(
                "database.open",
                CheckStatus::Ok,
                format!("opened {}", db_path.display()),
            );

            if migrate_if_needed {
                match apply_schema(&db).await {
                    Ok(()) => report.push(
                        "database.schema",
                        CheckStatus::Ok,
                        format!("schema v{SCHEMA_VERSION} applied"),
                    ),
                    Err(e) => report.push(
                        "database.schema",
                        CheckStatus::Fail,
                        format!("migrate failed: {e}"),
                    ),
                }
            }

            match collect_status(&db).await {
                Ok(status) => {
                    let st = if status.schema_ok {
                        CheckStatus::Ok
                    } else {
                        CheckStatus::Warn
                    };
                    report.push(
                        "database.version",
                        st,
                        format!(
                            "recorded={:?} expected={}",
                            status.schema_version, status.expected_schema_version
                        ),
                    );
                }
                Err(e) => report.push(
                    "database.version",
                    CheckStatus::Warn,
                    format!("could not read status (run init): {e}"),
                ),
            }

            check_embeddings(&mut report, config, Some(&db)).await;
        }
        Err(e) => {
            report.push(
                "database.open",
                CheckStatus::Fail,
                format!("failed to open {}: {e}", db_path.display()),
            );
            check_embeddings(&mut report, config, None).await;
        }
    }

    check_mcp(&mut report, config);
    check_adr(&mut report, config);

    Ok(report)
}

fn check_mcp(report: &mut DoctorReport, config: &Config) {
    match config.mcp.transport.as_str() {
        "stdio" => report.push(
            "mcp.transport",
            CheckStatus::Ok,
            "stdio (Cursor / Claude Code default)",
        ),
        "http" | "streamable_http" => match config.mcp.bind.parse::<std::net::SocketAddr>() {
            Ok(addr) if addr.ip().is_loopback() || config.mcp.allow_remote => {
                let detail = if addr.ip().is_loopback() {
                    format!("http://{addr}/mcp (loopback)")
                } else {
                    format!("http://{addr}/mcp (allow_remote=true)")
                };
                report.push("mcp.transport", CheckStatus::Ok, detail);
            }
            Ok(addr) => report.push(
                "mcp.transport",
                CheckStatus::Fail,
                format!("non-loopback bind {addr} refused unless mcp.allow_remote = true"),
            ),
            Err(error) => report.push(
                "mcp.transport",
                CheckStatus::Fail,
                format!("invalid mcp.bind {:?}: {error}", config.mcp.bind),
            ),
        },
        other => report.push(
            "mcp.transport",
            CheckStatus::Fail,
            format!("unsupported mcp.transport {other:?}; use stdio or http"),
        ),
    }
}

fn check_adr(report: &mut DoctorReport, config: &Config) {
    if !config.adr.write_vault {
        report.push(
            "adr.write_vault",
            CheckStatus::Ok,
            "write_vault=false (ADR tool will not touch the vault FS)",
        );
        return;
    }
    match config.sources.get(&config.adr.vault_source) {
        Some(source) if source.kind == crate::config::SourceKindConfig::ObsidianVault => {
            let path = source.resolved_path();
            if path.is_dir() {
                report.push(
                    "adr.write_vault",
                    CheckStatus::Ok,
                    format!(
                        "write_vault=true → {}/{}/",
                        config.adr.vault_source, config.adr.directory
                    ),
                );
            } else {
                report.push(
                    "adr.write_vault",
                    CheckStatus::Warn,
                    format!(
                        "write_vault=true but vault path missing: {}",
                        path.display()
                    ),
                );
            }
        }
        Some(_) => report.push(
            "adr.write_vault",
            CheckStatus::Fail,
            format!(
                "adr.vault_source {:?} must be kind = obsidian_vault",
                config.adr.vault_source
            ),
        ),
        None => report.push(
            "adr.write_vault",
            CheckStatus::Fail,
            format!(
                "adr.vault_source {:?} is not configured under [sources]",
                config.adr.vault_source
            ),
        ),
    }
}

async fn check_embeddings(
    report: &mut DoctorReport,
    config: &Config,
    db: Option<&codebrain_db::Database>,
) {
    match config.embeddings.provider {
        crate::config::EmbeddingsProvider::None => {
            report.push(
                "embeddings",
                CheckStatus::Ok,
                "provider=none (semantic_search degrades to FTS + graph)",
            );
        }
        other => {
            report.push(
                "embeddings.config",
                CheckStatus::Ok,
                format!(
                    "provider={other:?} model={} dimension={}",
                    config.embeddings.model, config.embeddings.dimension
                ),
            );
            match db {
                Some(db) => match codebrain_db::read_embedding_dimension(db).await {
                    Ok(Some(recorded)) if recorded == config.embeddings.dimension => {
                        report.push(
                            "embeddings.dimension",
                            CheckStatus::Ok,
                            format!("index dimension matches config ({recorded})"),
                        );
                    }
                    Ok(Some(recorded)) => report.push(
                        "embeddings.dimension",
                        CheckStatus::Fail,
                        format!(
                            "index dimension {recorded} != config {}; re-index after fixing embeddings.dimension",
                            config.embeddings.dimension
                        ),
                    ),
                    Ok(None) => report.push(
                        "embeddings.dimension",
                        CheckStatus::Warn,
                        "no embedding meta yet — run `codebrain index` with embeddings enabled",
                    ),
                    Err(error) => report.push(
                        "embeddings.dimension",
                        CheckStatus::Warn,
                        format!("could not read embedding meta: {error}"),
                    ),
                },
                None => report.push(
                    "embeddings.dimension",
                    CheckStatus::Warn,
                    "database unavailable; cannot verify embedding dimension",
                ),
            }
        }
    }
}

fn check_db_path(report: &mut DoctorReport, db_path: &Path) {
    if let Some(parent) = db_path.parent() {
        if parent.exists() || std::fs::create_dir_all(parent).is_ok() {
            report.push(
                "database.path",
                CheckStatus::Ok,
                format!("{}", db_path.display()),
            );
        } else {
            report.push(
                "database.path",
                CheckStatus::Fail,
                format!("cannot create parent for {}", db_path.display()),
            );
        }
    } else {
        report.push(
            "database.path",
            CheckStatus::Fail,
            format!("invalid database path: {}", db_path.display()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EmbeddingsConfig, EmbeddingsProvider};
    use codebrain_db::{apply_schema, open_memory, record_embedding_meta};

    #[tokio::test]
    async fn detects_embedding_dimension_mismatch() {
        let db = open_memory().await.expect("db");
        apply_schema(&db).await.expect("schema");
        record_embedding_meta(&db, "fastembed", "all-MiniLM-L6-v2", 384)
            .await
            .expect("meta");

        let mut report = DoctorReport {
            checks: Vec::new(),
            healthy: true,
        };
        let config = Config {
            embeddings: EmbeddingsConfig {
                provider: EmbeddingsProvider::Fastembed,
                model: "all-MiniLM-L6-v2".into(),
                dimension: 1536,
                ..EmbeddingsConfig::default()
            },
            ..Config::default()
        };
        check_embeddings(&mut report, &config, Some(&db)).await;
        assert!(
            !report.healthy,
            "expected unhealthy report, got checks={:?}",
            report.checks
        );
        assert!(report.checks.iter().any(|check| {
            check.name == "embeddings.dimension" && check.status == CheckStatus::Fail
        }));
    }

    #[test]
    fn rejects_http_remote_bind_without_allow() {
        let mut report = DoctorReport {
            checks: Vec::new(),
            healthy: true,
        };
        let config = Config {
            mcp: crate::config::McpConfig {
                transport: "http".into(),
                bind: "0.0.0.0:8765".into(),
                allow_remote: false,
            },
            ..Config::default()
        };
        check_mcp(&mut report, &config);
        assert!(!report.healthy);
        assert!(
            report.checks.iter().any(|check| {
                check.name == "mcp.transport" && check.status == CheckStatus::Fail
            })
        );
    }

    #[test]
    fn adr_write_vault_false_is_ok() {
        let mut report = DoctorReport {
            checks: Vec::new(),
            healthy: true,
        };
        check_adr(&mut report, &Config::default());
        assert!(report.healthy);
        assert!(
            report.checks.iter().any(|check| {
                check.name == "adr.write_vault" && check.status == CheckStatus::Ok
            })
        );
    }
}
