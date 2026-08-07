//! Capture architectural decisions into the graph, with optional vault write-back.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use chrono::Utc;
use codebrain_db::{
    ArchitectureDecision, Database, NodeAddress, decision_address, relate_about,
    upsert_architecture_decision,
};
use serde::Serialize;

use crate::{AdrConfig, Config, SourceKindConfig};

#[derive(Debug, Clone)]
pub struct AddDecisionRequest {
    pub title: String,
    pub body: String,
    pub about: Vec<NodeAddress>,
    /// When set, overrides `config.adr.write_vault` for this call.
    pub write_vault: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddDecisionResult {
    pub decision: ArchitectureDecision,
    pub about_linked: usize,
    pub vault_written: bool,
    pub token: String,
}

/// Persist an ADR node, `ABOUT` edges, and optionally a Markdown note in the vault.
pub async fn add_architectural_decision(
    db: &Database,
    config: &Config,
    request: AddDecisionRequest,
) -> anyhow::Result<AddDecisionResult> {
    let title = request.title.trim();
    let body = request.body.trim();
    if title.is_empty() {
        bail!("ADR title must not be empty");
    }
    if body.is_empty() {
        bail!("ADR body must not be empty");
    }

    let write_vault = request.write_vault.unwrap_or(config.adr.write_vault);
    let vault_path = if write_vault {
        Some(write_adr_markdown(config, title, body, &request.about)?)
    } else {
        None
    };

    let decision = upsert_architecture_decision(
        db,
        title,
        body,
        &config.adr.created_by,
        vault_path.as_deref(),
        Utc::now(),
    )
    .await
    .context("persist architecture decision")?;

    let about_linked = relate_about(db, &decision.title, &request.about)
        .await
        .context("create ABOUT edges")?;

    Ok(AddDecisionResult {
        token: decision_address(&decision.title).to_token(),
        decision,
        about_linked,
        vault_written: vault_path.is_some(),
    })
}

fn write_adr_markdown(
    config: &Config,
    title: &str,
    body: &str,
    about: &[NodeAddress],
) -> anyhow::Result<String> {
    let (vault_root, relative) = adr_vault_relative_path(&config.adr, config, title)?;
    let absolute = vault_root.join(&relative);
    if let Some(parent) = absolute.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create ADR directory {}", parent.display()))?;
    }

    let markdown = render_adr_markdown(title, body, &config.adr.created_by, about);
    std::fs::write(&absolute, markdown)
        .with_context(|| format!("write ADR note {}", absolute.display()))?;
    Ok(relative)
}

fn adr_vault_relative_path(
    adr: &AdrConfig,
    config: &Config,
    title: &str,
) -> anyhow::Result<(PathBuf, String)> {
    let source = config.sources.get(&adr.vault_source).with_context(|| {
        format!(
            "ADR vault source {:?} is not configured under [sources]",
            adr.vault_source
        )
    })?;
    if source.kind != SourceKindConfig::ObsidianVault {
        bail!(
            "ADR vault source {:?} must be kind = obsidian_vault",
            adr.vault_source
        );
    }
    let root = source.resolved_path();
    if !root.is_dir() {
        bail!("ADR vault root does not exist: {}", root.display());
    }
    let filename = format!("{}.md", slugify_title(title));
    let directory = adr.directory.trim_matches('/').trim_matches('\\');
    let relative = if directory.is_empty() {
        filename
    } else {
        format!("{directory}/{filename}")
    };
    Ok((root, relative.replace('\\', "/")))
}

fn render_adr_markdown(title: &str, body: &str, created_by: &str, about: &[NodeAddress]) -> String {
    let created = Utc::now().to_rfc3339();
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("title: {title}\n"));
    out.push_str("tags: [adr]\n");
    out.push_str(&format!("created: {created}\n"));
    out.push_str(&format!("created_by: {created_by}\n"));
    out.push_str("---\n\n");
    out.push_str(&format!("# {title}\n\n"));
    out.push_str(body);
    out.push_str("\n\n");
    if !about.is_empty() {
        out.push_str("## About\n\n");
        for target in about {
            out.push_str(&format!("- `{}`\n", target.to_token()));
        }
        out.push('\n');
    }
    out
}

fn slugify_title(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { "adr".into() } else { slug }
}

/// Resolve a vault absolute path for tests / diagnostics without writing.
pub fn adr_note_path(config: &Config, title: &str) -> anyhow::Result<PathBuf> {
    let (root, relative) = adr_vault_relative_path(&config.adr, config, title)?;
    Ok(root.join(Path::new(&relative)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use codebrain_connector::{ExtractBatch, FileNode, SymbolNode};
    use codebrain_db::{NodeKind, apply_schema, open_memory, persist_code_batch};
    use std::collections::HashMap;
    use std::fs;
    use tempfile::tempdir;

    fn seeded_config(vault: &Path) -> Config {
        Config {
            sources: HashMap::from([(
                "notes".into(),
                crate::SourceConfig {
                    kind: SourceKindConfig::ObsidianVault,
                    path: vault.to_string_lossy().into_owned(),
                    languages: Vec::new(),
                    ..Default::default()
                },
            )]),
            adr: AdrConfig {
                write_vault: false,
                vault_source: "notes".into(),
                directory: "ADR".into(),
                created_by: "test".into(),
            },
            ..Config::default()
        }
    }

    #[tokio::test]
    async fn write_vault_false_never_touches_vault_fs() {
        let vault = tempdir().expect("vault");
        let config = seeded_config(vault.path());
        let db = open_memory().await.expect("db");
        apply_schema(&db).await.expect("schema");

        let result = add_architectural_decision(
            &db,
            &config,
            AddDecisionRequest {
                title: "Keep vault pristine".into(),
                body: "No markdown write-back in this mode.".into(),
                about: Vec::new(),
                write_vault: Some(false),
            },
        )
        .await
        .expect("adr");

        assert!(!result.vault_written);
        assert!(result.decision.vault_path.is_none());
        assert!(
            fs::read_dir(vault.path())
                .expect("read vault")
                .next()
                .is_none(),
            "vault must stay empty when write_vault=false"
        );
    }

    #[tokio::test]
    async fn write_vault_true_creates_markdown_and_about_edge() {
        let vault = tempdir().expect("vault");
        let mut config = seeded_config(vault.path());
        config.adr.write_vault = true;

        let db = open_memory().await.expect("db");
        apply_schema(&db).await.expect("schema");
        let batch = ExtractBatch {
            files: vec![FileNode {
                path: "a.rb".into(),
                language: Some("ruby".into()),
                content_hash: "h".into(),
                mtime: Utc::now(),
            }],
            symbols: vec![SymbolNode {
                file_path: "a.rb".into(),
                name: "Greeter".into(),
                fqn: "Services::Greeter".into(),
                kind: "class".into(),
                signature: None,
                start_line: 1,
                end_line: 2,
                content_hash: "s".into(),
            }],
            ..ExtractBatch::default()
        };
        persist_code_batch(&db, "code", "/tmp", &batch)
            .await
            .expect("code");

        let result = add_architectural_decision(
            &db,
            &config,
            AddDecisionRequest {
                title: "Use Greeter facade".into(),
                body: "All greeting copy goes through Greeter.".into(),
                about: vec![NodeAddress {
                    kind: NodeKind::Symbol,
                    source: "code".into(),
                    key: "Services::Greeter".into(),
                }],
                write_vault: None,
            },
        )
        .await
        .expect("adr");

        assert!(result.vault_written);
        assert_eq!(result.about_linked, 1);
        let note = vault.path().join("ADR/use-greeter-facade.md");
        assert!(note.is_file(), "missing {}", note.display());
        let contents = fs::read_to_string(&note).expect("read note");
        assert!(contents.contains("Use Greeter facade"));
        assert!(contents.contains("symbol:code:Services::Greeter"));
    }
}
