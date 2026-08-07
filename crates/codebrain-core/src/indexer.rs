use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use codebrain_connector::{Connector, EdgeType, ExtractBatch, IndexContext, WorkItem};
use codebrain_connector_code::{CodeConnector, Language};
use codebrain_connector_obsidian::ObsidianConnector;
use codebrain_connector_saas::{
    ConfluenceConnector, JiraConnector, NotionConnector, find_issue_keys,
};
use codebrain_db::{
    Database, NodeAddress, NodeKind, count_table, delete_code_file, delete_document,
    existing_document_hashes, existing_file_hashes, find_symbol_fqn, list_documents_for_resolution,
    list_symbol_fqns_for_file, list_symbols_for_mentions, persist_code_batch,
    persist_document_batch, promote_mention, relate_call, relate_cross_reference, relate_import,
    relate_mention, relate_reference, relate_resolves, resolve_wikilink, upsert_code_source,
    upsert_confluence_source, upsert_jira_source, upsert_notion_source, upsert_obsidian_source,
};
use serde::Serialize;
use tokio::task::JoinSet;

use crate::linker::{MentionIndex, find_mentions_indexed};
use crate::semantic::{embed_extract_batch, prepare_embedding_store, remove_document_chunks};
use crate::{Config, EmbeddingsProvider, SourceConfig, SourceKindConfig};
use codebrain_embed::{Embedder, EmbedderConfig, EmbedderKind, build_embedder};

#[derive(Debug, Clone, Default, Serialize)]
pub struct IndexReport {
    pub sources: Vec<SourceIndexReport>,
}

impl IndexReport {
    pub fn discovered(&self) -> usize {
        self.sources.iter().map(|source| source.discovered).sum()
    }

    pub fn indexed(&self) -> usize {
        self.sources.iter().map(|source| source.indexed).sum()
    }

    pub fn skipped(&self) -> usize {
        self.sources.iter().map(|source| source.skipped).sum()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SourceIndexReport {
    pub source: String,
    pub discovered: usize,
    pub indexed: usize,
    pub skipped: usize,
    pub removed: usize,
    pub symbols: usize,
    pub imports: usize,
    pub calls: usize,
    pub documents: usize,
    pub references: usize,
    pub mentions: usize,
    pub explains: usize,
    pub resolves: usize,
    pub broken_links: usize,
    pub chunks: usize,
}

pub async fn index_configured_sources(
    db: &Database,
    config: &Config,
    source_filter: Option<&str>,
    force: bool,
) -> anyhow::Result<IndexReport> {
    let mut selected: Vec<_> = config
        .sources
        .iter()
        .filter(|(name, _)| source_filter.is_none_or(|filter| filter == name.as_str()))
        .collect();
    // Code first so vault mention linking sees symbols in the same run.
    selected.sort_by_key(|(_, source)| match source.kind {
        SourceKindConfig::GitRepo => 0,
        SourceKindConfig::Jira => 1,
        SourceKindConfig::Confluence => 2,
        SourceKindConfig::Notion => 3,
        SourceKindConfig::ObsidianVault => 4,
    });

    if selected.is_empty() {
        match source_filter {
            Some(name) => bail!("configured source not found: {name}"),
            None => bail!("no sources configured"),
        }
    }

    let embedder =
        build_embedder(&embedder_config_from(config)).context("build embedder from config")?;
    prepare_embedding_store(db, &embedder)
        .await
        .context("prepare embedding store")?;

    // Enabling embeddings after a graph-only index leaves content hashes unchanged and
    // would otherwise skip every file — force a full pass when the chunk table is empty.
    let mut force = force;
    if !force && embedder.enabled() {
        let chunks = count_table(db, "chunk").await.unwrap_or(0);
        if chunks == 0 {
            tracing::info!(
                "embeddings enabled with empty chunk table; forcing full reindex to build vectors"
            );
            force = true;
        }
    }

    let mut report = IndexReport::default();
    for (name, source) in selected {
        match source.kind {
            SourceKindConfig::GitRepo => {
                report.sources.push(
                    index_code_source(db, config, name, source, &embedder, None, force).await?,
                );
            }
            SourceKindConfig::ObsidianVault => {
                report.sources.push(
                    index_obsidian_source(db, config, name, source, &embedder, None, force).await?,
                );
            }
            SourceKindConfig::Jira => {
                report.sources.push(
                    index_jira_source(db, config, name, source, &embedder, None, force).await?,
                );
            }
            SourceKindConfig::Confluence => {
                report.sources.push(
                    index_confluence_source(db, config, name, source, &embedder, None, force)
                        .await?,
                );
            }
            SourceKindConfig::Notion => {
                report.sources.push(
                    index_notion_source(db, config, name, source, &embedder, None, force).await?,
                );
            }
        }
    }

    // After code + tickets exist, wire RESOLVES from issue keys found in source files.
    if let Err(error) = link_jira_resolves(db, config, &mut report).await {
        tracing::warn!(%error, "jira resolves linking failed");
    }
    // Confluence/Notion pages that cite issue keys → references to Jira documents.
    if let Err(error) = link_saas_doc_jira_refs(db, config, &mut report).await {
        tracing::warn!(%error, "saas→jira reference linking failed");
    }

    if report.sources.is_empty() {
        bail!(
            "no indexable sources selected (git_repo / obsidian_vault / jira / confluence / notion)"
        );
    }
    Ok(report)
}

/// Reindex only the given relative paths for one named source (watch / ADR write-back).
pub async fn reindex_source_paths(
    db: &Database,
    config: &Config,
    source_name: &str,
    relative_paths: &[String],
) -> anyhow::Result<SourceIndexReport> {
    let source = config
        .sources
        .get(source_name)
        .with_context(|| format!("configured source not found: {source_name}"))?;

    let paths: Vec<String> = relative_paths
        .iter()
        .map(|path| normalize_relative(path))
        .filter(|path| !path.is_empty())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let embedder =
        build_embedder(&embedder_config_from(config)).context("build embedder from config")?;
    prepare_embedding_store(db, &embedder)
        .await
        .context("prepare embedding store")?;

    match source.kind {
        SourceKindConfig::GitRepo => {
            index_code_source(
                db,
                config,
                source_name,
                source,
                &embedder,
                Some(&paths),
                false,
            )
            .await
        }
        SourceKindConfig::ObsidianVault => {
            index_obsidian_source(
                db,
                config,
                source_name,
                source,
                &embedder,
                Some(&paths),
                false,
            )
            .await
        }
        SourceKindConfig::Jira => {
            index_jira_source(
                db,
                config,
                source_name,
                source,
                &embedder,
                Some(&paths),
                false,
            )
            .await
        }
        SourceKindConfig::Confluence => {
            index_confluence_source(
                db,
                config,
                source_name,
                source,
                &embedder,
                Some(&paths),
                false,
            )
            .await
        }
        SourceKindConfig::Notion => {
            index_notion_source(
                db,
                config,
                source_name,
                source,
                &embedder,
                Some(&paths),
                false,
            )
            .await
        }
    }
}

fn normalize_relative(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_string()
}

async fn index_code_source(
    db: &Database,
    config: &Config,
    name: &str,
    source: &SourceConfig,
    embedder: &Arc<dyn Embedder>,
    only_paths: Option<&[String]>,
    force: bool,
) -> anyhow::Result<SourceIndexReport> {
    let root = source.resolved_path();
    let languages = configured_languages(source)?;
    let connector = Arc::new(CodeConnector::new(name, languages.clone()));
    let context = IndexContext {
        source_name: name.to_string(),
        root_path: root.clone(),
        excludes: config.index.exclude.clone(),
    };
    let root_string = root.to_string_lossy().into_owned();
    upsert_code_source(db, name, &root_string).await?;

    let existing = existing_file_hashes(db, name).await?;
    let (discovered_paths, changed, removed) = if let Some(paths) = only_paths {
        let mut discovered_paths = HashSet::new();
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for relative in paths {
            let absolute = root.join(relative);
            if absolute.is_file() {
                let item = work_item_from_path(&root, relative)
                    .with_context(|| format!("stat {relative}"))?;
                discovered_paths.insert(relative.clone());
                let hash = item.content_hash.as_deref();
                if force || existing.get(relative).map(String::as_str) != hash {
                    changed.push(item);
                }
            } else if existing.contains_key(relative) {
                removed.push(relative.clone());
            }
        }
        (discovered_paths, changed, removed)
    } else {
        let items = connector
            .discover(&context)
            .await
            .with_context(|| format!("discover source {name}"))?;
        let discovered_paths: HashSet<String> = items.iter().map(|item| item.id.clone()).collect();
        let removed: Vec<_> = existing
            .keys()
            .filter(|path| !discovered_paths.contains(path.as_str()))
            .cloned()
            .collect();
        let changed: Vec<_> = items
            .into_iter()
            .filter(|item| {
                let hash = item.content_hash.as_deref();
                force || existing.get(&item.id).map(String::as_str) != hash
            })
            .collect();
        (discovered_paths, changed, removed)
    };

    let mut report = SourceIndexReport {
        source: name.to_string(),
        discovered: if only_paths.is_some() {
            only_paths.map_or(0, |paths| paths.len())
        } else {
            discovered_paths.len()
        },
        indexed: changed.len(),
        skipped: if only_paths.is_some() {
            only_paths
                .map(|paths| paths.len().saturating_sub(changed.len() + removed.len()))
                .unwrap_or(0)
        } else {
            discovered_paths.len().saturating_sub(changed.len())
        },
        removed: removed.len(),
        ..SourceIndexReport::default()
    };
    for path in &removed {
        clear_file_symbol_chunks(db, name, path).await?;
        delete_code_file(db, name, path).await?;
    }
    if changed.is_empty() {
        return Ok(report);
    }

    let path_universe = if only_paths.is_some() {
        // Partial runs still need import targets that already exist on disk.
        let mut universe = existing.keys().cloned().collect::<HashSet<_>>();
        universe.extend(discovered_paths.iter().cloned());
        universe
    } else {
        discovered_paths.clone()
    };

    let extract_concurrency = extract_concurrency();
    let persist_chunk = config.index.batch_size.max(1);
    let batches = extract_bounded(connector, changed, extract_concurrency).await?;
    let mut edges = Vec::new();
    let mut symbol_files = HashMap::new();
    for chunk in batches.chunks(persist_chunk) {
        let merged = merge_extract_batches(chunk);
        for file in &merged.files {
            clear_file_symbol_chunks(db, name, &file.path).await?;
        }
        let persisted = persist_code_batch(db, name, &root_string, &merged).await?;
        report.symbols += persisted.symbols;
        report.chunks += embed_extract_batch(db, name, &merged, embedder).await?;
        for symbol in &merged.symbols {
            symbol_files.insert(symbol.fqn.clone(), symbol.file_path.clone());
        }
        edges.extend(merged.edges.iter().cloned());
    }

    for edge in edges {
        match edge.edge_type {
            EdgeType::Imports => {
                let Some(from) = edge.from_key.strip_prefix("file:") else {
                    continue;
                };
                let Some(raw_target) = edge.to_key.strip_prefix("import:") else {
                    continue;
                };
                if let Some(target) = resolve_import(from, raw_target, &path_universe)
                    && relate_import(db, name, from, &target).await?
                {
                    report.imports += 1;
                }
            }
            EdgeType::Calls => {
                let Some(from) = edge.from_key.strip_prefix("symbol:") else {
                    continue;
                };
                let Some(target_name) = edge.to_key.strip_prefix("call:") else {
                    continue;
                };
                let preferred_file = symbol_files.get(from).map(String::as_str);
                if let Some(target_fqn) =
                    find_symbol_fqn(db, name, target_name, preferred_file).await?
                    && target_fqn != from
                    && relate_call(db, name, from, &target_fqn).await?
                {
                    report.calls += 1;
                }
            }
            _ => {}
        }
    }

    Ok(report)
}

async fn index_obsidian_source(
    db: &Database,
    config: &Config,
    name: &str,
    source: &SourceConfig,
    embedder: &Arc<dyn Embedder>,
    only_paths: Option<&[String]>,
    force: bool,
) -> anyhow::Result<SourceIndexReport> {
    let root = source.resolved_path();
    let connector = Arc::new(ObsidianConnector::new(name));
    let context = IndexContext {
        source_name: name.to_string(),
        root_path: root.clone(),
        excludes: config.index.exclude.clone(),
    };
    let root_string = root.to_string_lossy().into_owned();
    upsert_obsidian_source(db, name, &root_string).await?;

    let existing = existing_document_hashes(db, name).await?;
    let (discovered_paths, changed, removed) = if let Some(paths) = only_paths {
        let mut discovered_paths = HashSet::new();
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for relative in paths {
            let absolute = root.join(relative);
            if absolute.is_file() {
                let item = work_item_from_path(&root, relative)
                    .with_context(|| format!("stat {relative}"))?;
                discovered_paths.insert(relative.clone());
                let hash = item.content_hash.as_deref();
                if force || existing.get(relative).map(String::as_str) != hash {
                    changed.push(item);
                }
            } else if existing.contains_key(relative) {
                removed.push(relative.clone());
            }
        }
        (discovered_paths, changed, removed)
    } else {
        let items = connector
            .discover(&context)
            .await
            .with_context(|| format!("discover vault {name}"))?;
        let discovered_paths: HashSet<String> = items.iter().map(|item| item.id.clone()).collect();
        let removed: Vec<_> = existing
            .keys()
            .filter(|path| !discovered_paths.contains(path.as_str()))
            .cloned()
            .collect();
        let changed: Vec<_> = items
            .into_iter()
            .filter(|item| {
                let hash = item.content_hash.as_deref();
                force || existing.get(&item.id).map(String::as_str) != hash
            })
            .collect();
        (discovered_paths, changed, removed)
    };

    let mut report = SourceIndexReport {
        source: name.to_string(),
        discovered: if only_paths.is_some() {
            only_paths.map_or(0, |paths| paths.len())
        } else {
            discovered_paths.len()
        },
        indexed: changed.len(),
        skipped: if only_paths.is_some() {
            only_paths
                .map(|paths| paths.len().saturating_sub(changed.len() + removed.len()))
                .unwrap_or(0)
        } else {
            discovered_paths.len().saturating_sub(changed.len())
        },
        removed: removed.len(),
        ..SourceIndexReport::default()
    };
    for path in &removed {
        remove_document_chunks(db, name, path).await?;
        delete_document(db, name, path).await?;
    }

    let mut edges = Vec::new();
    if !changed.is_empty() {
        let extract_concurrency = extract_concurrency();
        let persist_chunk = config.index.batch_size.max(1);
        let batches = extract_bounded(connector, changed, extract_concurrency).await?;
        for chunk in batches.chunks(persist_chunk) {
            let merged = merge_extract_batches(chunk);
            let persisted = persist_document_batch(db, name, &root_string, &merged).await?;
            report.documents += persisted.documents;
            report.chunks += embed_extract_batch(db, name, &merged, embedder).await?;
            edges.extend(merged.edges.iter().cloned());
        }
    }

    let documents = list_documents_for_resolution(db, name).await?;
    for edge in edges {
        if edge.edge_type != EdgeType::References {
            continue;
        }
        let Some(from) = edge.from_key.strip_prefix("document:") else {
            continue;
        };
        let Some(target) = edge.to_key.strip_prefix("wikilink:") else {
            continue;
        };
        match resolve_wikilink(target, &documents) {
            Some(to_path) => {
                if relate_reference(db, name, from, &to_path).await? {
                    report.references += 1;
                }
            }
            None => {
                report.broken_links += 1;
                tracing::warn!(
                    source = %name,
                    from,
                    target,
                    "unresolved wikilink (orphan reference recorded as broken_links)"
                );
            }
        }
    }

    // Always refresh mentions so a later code index + vault re-run can link.
    // On partial reindex, only recompute mentions for the touched documents.
    let symbols = list_symbols_for_mentions(db).await?;
    let mention_index = MentionIndex::build(&symbols, config.linker.mention_threshold);
    if mention_index.is_empty() {
        return Ok(report);
    }
    let mention_docs: Vec<_> = if let Some(paths) = only_paths {
        documents
            .iter()
            .filter(|document| paths.iter().any(|path| path == &document.path))
            .collect()
    } else {
        documents.iter().collect()
    };
    for document in mention_docs {
        let mentions = find_mentions_indexed(&document.body, &mention_index);
        for mention in mentions {
            if relate_mention(
                db,
                name,
                &document.path,
                &mention.symbol_source,
                &mention.symbol_fqn,
                mention.confidence,
                Some(&mention.evidence),
            )
            .await?
            {
                report.mentions += 1;
                if config.linker.auto_promote_explains {
                    let document_addr = NodeAddress {
                        kind: NodeKind::Document,
                        source: name.to_string(),
                        key: document.path.clone(),
                    };
                    let symbol_addr = NodeAddress {
                        kind: NodeKind::Symbol,
                        source: mention.symbol_source.clone(),
                        key: mention.symbol_fqn.clone(),
                    };
                    match promote_mention(db, &document_addr, &symbol_addr).await? {
                        Some(_) => report.explains += 1,
                        None => tracing::warn!(
                            source = %name,
                            document = %document.path,
                            symbol = %mention.symbol_fqn,
                            "auto_promote_explains enabled but mention was not found after relate"
                        ),
                    }
                }
            }
        }
    }

    Ok(report)
}

async fn index_jira_source(
    db: &Database,
    config: &Config,
    name: &str,
    source: &SourceConfig,
    embedder: &Arc<dyn Embedder>,
    only_paths: Option<&[String]>,
    force: bool,
) -> anyhow::Result<SourceIndexReport> {
    let auth = source.jira_auth()?;
    let jql = source.jql.clone().unwrap_or_else(|| {
        "assignee = currentUser() AND updated >= -30d ORDER BY updated DESC".into()
    });
    let connector = Arc::new(JiraConnector::new(
        name,
        auth.clone(),
        jql,
        source.max_issues,
    )?);
    let context = IndexContext {
        source_name: name.to_string(),
        root_path: std::path::PathBuf::from(&auth.base_url),
        excludes: config.index.exclude.clone(),
    };
    upsert_jira_source(db, name, &auth.base_url).await?;

    let items = connector
        .discover(&context)
        .await
        .with_context(|| format!("discover jira source {name}"))?;
    let discovered_paths: HashSet<String> = items.iter().map(|item| item.id.clone()).collect();
    let existing = existing_document_hashes(db, name).await?;

    let (changed, removed) = if let Some(paths) = only_paths {
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for key in paths {
            if let Some(item) = items.iter().find(|item| &item.id == key) {
                let hash = item.content_hash.as_deref();
                if force || existing.get(key).map(String::as_str) != hash {
                    changed.push(item.clone());
                }
            } else if existing.contains_key(key) {
                removed.push(key.clone());
            }
        }
        (changed, removed)
    } else {
        let removed: Vec<_> = existing
            .keys()
            .filter(|path| !discovered_paths.contains(path.as_str()))
            .cloned()
            .collect();
        let changed: Vec<_> = items
            .into_iter()
            .filter(|item| {
                let hash = item.content_hash.as_deref();
                force || existing.get(&item.id).map(String::as_str) != hash
            })
            .collect();
        (changed, removed)
    };

    let mut report = SourceIndexReport {
        source: name.to_string(),
        discovered: if only_paths.is_some() {
            only_paths.map_or(0, |paths| paths.len())
        } else {
            discovered_paths.len()
        },
        indexed: changed.len(),
        skipped: if only_paths.is_some() {
            only_paths
                .map(|paths| paths.len().saturating_sub(changed.len() + removed.len()))
                .unwrap_or(0)
        } else {
            discovered_paths.len().saturating_sub(changed.len())
        },
        removed: removed.len(),
        ..SourceIndexReport::default()
    };

    for path in &removed {
        remove_document_chunks(db, name, path).await?;
        delete_document(db, name, path).await?;
    }
    if changed.is_empty() {
        return Ok(report);
    }

    let batches = extract_bounded(connector, changed, config.index.batch_size).await?;
    for chunk in batches.chunks(config.index.batch_size.max(1)) {
        let merged = merge_extract_batches(chunk);
        let persisted = persist_document_batch(db, name, &auth.base_url, &merged).await?;
        report.documents += persisted.documents;
        report.chunks += embed_extract_batch(db, name, &merged, embedder).await?;
    }

    Ok(report)
}

async fn index_confluence_source(
    db: &Database,
    config: &Config,
    name: &str,
    source: &SourceConfig,
    embedder: &Arc<dyn Embedder>,
    only_paths: Option<&[String]>,
    force: bool,
) -> anyhow::Result<SourceIndexReport> {
    let auth = source.atlassian_auth()?;
    let cql = source.confluence_cql();
    let connector = Arc::new(ConfluenceConnector::new(
        name,
        auth.clone(),
        cql,
        source.max_issues,
    )?);
    let context = IndexContext {
        source_name: name.to_string(),
        root_path: std::path::PathBuf::from(&auth.base_url),
        excludes: config.index.exclude.clone(),
    };
    upsert_confluence_source(db, name, &auth.base_url).await?;

    let items = connector
        .discover(&context)
        .await
        .with_context(|| format!("discover confluence source {name}"))?;
    let discovered_paths: HashSet<String> = items.iter().map(|item| item.id.clone()).collect();
    let existing = existing_document_hashes(db, name).await?;

    let (changed, removed) = if let Some(paths) = only_paths {
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for key in paths {
            if let Some(item) = items.iter().find(|item| &item.id == key) {
                let hash = item.content_hash.as_deref();
                if force || existing.get(key).map(String::as_str) != hash {
                    changed.push(item.clone());
                }
            } else if existing.contains_key(key) {
                removed.push(key.clone());
            }
        }
        (changed, removed)
    } else {
        let removed: Vec<_> = existing
            .keys()
            .filter(|path| !discovered_paths.contains(path.as_str()))
            .cloned()
            .collect();
        let changed: Vec<_> = items
            .into_iter()
            .filter(|item| {
                let hash = item.content_hash.as_deref();
                force || existing.get(&item.id).map(String::as_str) != hash
            })
            .collect();
        (changed, removed)
    };

    let mut report = SourceIndexReport {
        source: name.to_string(),
        discovered: if only_paths.is_some() {
            only_paths.map_or(0, |paths| paths.len())
        } else {
            discovered_paths.len()
        },
        indexed: changed.len(),
        skipped: if only_paths.is_some() {
            only_paths
                .map(|paths| paths.len().saturating_sub(changed.len() + removed.len()))
                .unwrap_or(0)
        } else {
            discovered_paths.len().saturating_sub(changed.len())
        },
        removed: removed.len(),
        ..SourceIndexReport::default()
    };

    for path in &removed {
        remove_document_chunks(db, name, path).await?;
        delete_document(db, name, path).await?;
    }
    if !changed.is_empty() {
        let batches = extract_bounded(connector, changed, config.index.batch_size).await?;
        for chunk in batches.chunks(config.index.batch_size.max(1)) {
            let merged = merge_extract_batches(chunk);
            let persisted = persist_document_batch(db, name, &auth.base_url, &merged).await?;
            report.documents += persisted.documents;
            report.chunks += embed_extract_batch(db, name, &merged, embedder).await?;
        }
    }

    // Mentions: design pages often cite service/class names.
    let documents = list_documents_for_resolution(db, name).await?;
    let symbols = list_symbols_for_mentions(db).await?;
    let mention_index = MentionIndex::build(&symbols, config.linker.mention_threshold);
    if !mention_index.is_empty() {
        let mention_docs: Vec<_> = if let Some(paths) = only_paths {
            documents
                .iter()
                .filter(|document| paths.iter().any(|path| path == &document.path))
                .collect()
        } else {
            documents.iter().collect()
        };
        for document in mention_docs {
            let mentions = find_mentions_indexed(&document.body, &mention_index);
            for mention in mentions {
                if relate_mention(
                    db,
                    name,
                    &document.path,
                    &mention.symbol_source,
                    &mention.symbol_fqn,
                    mention.confidence,
                    Some(&mention.evidence),
                )
                .await?
                {
                    report.mentions += 1;
                    if config.linker.auto_promote_explains {
                        let document_addr = NodeAddress {
                            kind: NodeKind::Document,
                            source: name.to_string(),
                            key: document.path.clone(),
                        };
                        let symbol_addr = NodeAddress {
                            kind: NodeKind::Symbol,
                            source: mention.symbol_source.clone(),
                            key: mention.symbol_fqn.clone(),
                        };
                        if promote_mention(db, &document_addr, &symbol_addr)
                            .await?
                            .is_some()
                        {
                            report.explains += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(report)
}

async fn index_notion_source(
    db: &Database,
    config: &Config,
    name: &str,
    source: &SourceConfig,
    embedder: &Arc<dyn Embedder>,
    only_paths: Option<&[String]>,
    force: bool,
) -> anyhow::Result<SourceIndexReport> {
    let auth = source.notion_auth()?;
    let query = source.notion_query();
    let connector = Arc::new(NotionConnector::new(name, auth, query, source.max_issues)?);
    let context = IndexContext {
        source_name: name.to_string(),
        root_path: std::path::PathBuf::from("https://api.notion.com"),
        excludes: config.index.exclude.clone(),
    };
    upsert_notion_source(db, name, "https://api.notion.com").await?;

    let items = connector
        .discover(&context)
        .await
        .with_context(|| format!("discover notion source {name}"))?;
    let discovered_paths: HashSet<String> = items.iter().map(|item| item.id.clone()).collect();
    let existing = existing_document_hashes(db, name).await?;

    let (changed, removed) = if let Some(paths) = only_paths {
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for key in paths {
            if let Some(item) = items.iter().find(|item| &item.id == key) {
                let hash = item.content_hash.as_deref();
                if force || existing.get(key).map(String::as_str) != hash {
                    changed.push(item.clone());
                }
            } else if existing.contains_key(key) {
                removed.push(key.clone());
            }
        }
        (changed, removed)
    } else {
        let removed: Vec<_> = existing
            .keys()
            .filter(|path| !discovered_paths.contains(path.as_str()))
            .cloned()
            .collect();
        let changed: Vec<_> = items
            .into_iter()
            .filter(|item| {
                let hash = item.content_hash.as_deref();
                force || existing.get(&item.id).map(String::as_str) != hash
            })
            .collect();
        (changed, removed)
    };

    let mut report = SourceIndexReport {
        source: name.to_string(),
        discovered: if only_paths.is_some() {
            only_paths.map_or(0, |paths| paths.len())
        } else {
            discovered_paths.len()
        },
        indexed: changed.len(),
        skipped: if only_paths.is_some() {
            only_paths
                .map(|paths| paths.len().saturating_sub(changed.len() + removed.len()))
                .unwrap_or(0)
        } else {
            discovered_paths.len().saturating_sub(changed.len())
        },
        removed: removed.len(),
        ..SourceIndexReport::default()
    };

    for path in &removed {
        remove_document_chunks(db, name, path).await?;
        delete_document(db, name, path).await?;
    }
    if !changed.is_empty() {
        let batches = extract_bounded(connector, changed, config.index.batch_size).await?;
        for chunk in batches.chunks(config.index.batch_size.max(1)) {
            let merged = merge_extract_batches(chunk);
            let persisted =
                persist_document_batch(db, name, "https://api.notion.com", &merged).await?;
            report.documents += persisted.documents;
            report.chunks += embed_extract_batch(db, name, &merged, embedder).await?;
        }
    }

    let documents = list_documents_for_resolution(db, name).await?;
    let symbols = list_symbols_for_mentions(db).await?;
    let mention_index = MentionIndex::build(&symbols, config.linker.mention_threshold);
    if !mention_index.is_empty() {
        let mention_docs: Vec<_> = if let Some(paths) = only_paths {
            documents
                .iter()
                .filter(|document| paths.iter().any(|path| path == &document.path))
                .collect()
        } else {
            documents.iter().collect()
        };
        for document in mention_docs {
            let mentions = find_mentions_indexed(&document.body, &mention_index);
            for mention in mentions {
                if relate_mention(
                    db,
                    name,
                    &document.path,
                    &mention.symbol_source,
                    &mention.symbol_fqn,
                    mention.confidence,
                    Some(&mention.evidence),
                )
                .await?
                {
                    report.mentions += 1;
                    if config.linker.auto_promote_explains {
                        let document_addr = NodeAddress {
                            kind: NodeKind::Document,
                            source: name.to_string(),
                            key: document.path.clone(),
                        };
                        let symbol_addr = NodeAddress {
                            kind: NodeKind::Symbol,
                            source: mention.symbol_source.clone(),
                            key: mention.symbol_fqn.clone(),
                        };
                        if promote_mention(db, &document_addr, &symbol_addr)
                            .await?
                            .is_some()
                        {
                            report.explains += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(report)
}

async fn link_jira_resolves(
    db: &Database,
    config: &Config,
    report: &mut IndexReport,
) -> anyhow::Result<()> {
    let jira_sources: Vec<_> = config
        .sources
        .iter()
        .filter(|(_, source)| source.kind == SourceKindConfig::Jira)
        .map(|(name, _)| name.clone())
        .collect();
    if jira_sources.is_empty() {
        return Ok(());
    }

    let mut ticket_keys = HashSet::new();
    let mut key_to_source = HashMap::new();
    for jira_source in &jira_sources {
        for document in list_documents_for_resolution(db, jira_source).await? {
            ticket_keys.insert(document.path.clone());
            key_to_source.insert(document.path.clone(), jira_source.clone());
        }
    }
    if ticket_keys.is_empty() {
        return Ok(());
    }

    for (code_name, source) in &config.sources {
        if source.kind != SourceKindConfig::GitRepo {
            continue;
        }
        let root = source.resolved_path();
        let files = existing_file_hashes(db, code_name).await?;
        for relative in files.keys() {
            let absolute = root.join(relative);
            let Ok(content) = std::fs::read_to_string(&absolute) else {
                continue;
            };
            let keys = find_issue_keys(&content);
            if keys.is_empty() {
                continue;
            }
            let fqns = list_symbol_fqns_for_file(db, code_name, relative).await?;
            if fqns.is_empty() {
                continue;
            }
            for key in keys {
                if !ticket_keys.contains(&key) {
                    continue;
                }
                let Some(jira_source) = key_to_source.get(&key) else {
                    continue;
                };
                for fqn in &fqns {
                    if relate_resolves(db, code_name, fqn, jira_source, &key).await? {
                        if let Some(source_report) = report
                            .sources
                            .iter_mut()
                            .find(|item| item.source == *jira_source)
                        {
                            source_report.resolves += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn link_saas_doc_jira_refs(
    db: &Database,
    config: &Config,
    report: &mut IndexReport,
) -> anyhow::Result<()> {
    let jira_sources: Vec<_> = config
        .sources
        .iter()
        .filter(|(_, source)| source.kind == SourceKindConfig::Jira)
        .map(|(name, _)| name.clone())
        .collect();
    let doc_sources: Vec<_> = config
        .sources
        .iter()
        .filter(|(_, source)| {
            matches!(
                source.kind,
                SourceKindConfig::Confluence | SourceKindConfig::Notion
            )
        })
        .map(|(name, _)| name.clone())
        .collect();
    if jira_sources.is_empty() || doc_sources.is_empty() {
        return Ok(());
    }

    let mut ticket_keys = HashSet::new();
    let mut key_to_source = HashMap::new();
    for jira_source in &jira_sources {
        for document in list_documents_for_resolution(db, jira_source).await? {
            ticket_keys.insert(document.path.clone());
            key_to_source.insert(document.path.clone(), jira_source.clone());
        }
    }
    if ticket_keys.is_empty() {
        return Ok(());
    }

    for doc_source in &doc_sources {
        let pages = list_documents_for_resolution(db, doc_source).await?;
        for page in pages {
            for key in find_issue_keys(&page.body) {
                if !ticket_keys.contains(&key) {
                    continue;
                }
                let Some(jira_source) = key_to_source.get(&key) else {
                    continue;
                };
                if relate_cross_reference(db, doc_source, &page.path, jira_source, &key).await? {
                    if let Some(source_report) = report
                        .sources
                        .iter_mut()
                        .find(|item| item.source == *doc_source)
                    {
                        source_report.references += 1;
                    }
                }
            }
        }
    }
    Ok(())
}

fn merge_extract_batches(batches: &[ExtractBatch]) -> ExtractBatch {
    let mut merged = ExtractBatch::default();
    for batch in batches {
        merged.files.extend(batch.files.iter().cloned());
        merged.symbols.extend(batch.symbols.iter().cloned());
        merged.documents.extend(batch.documents.iter().cloned());
        merged.edges.extend(batch.edges.iter().cloned());
        merged.chunks.extend(batch.chunks.iter().cloned());
    }
    merged
}

fn extract_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(8)
        .clamp(4, 32)
}

fn work_item_from_path(root: &Path, relative: &str) -> anyhow::Result<WorkItem> {
    let absolute = root.join(relative);
    let metadata = absolute
        .metadata()
        .with_context(|| format!("metadata {}", absolute.display()))?;
    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    Ok(WorkItem {
        id: relative.replace('\\', "/"),
        path: absolute.to_string_lossy().into_owned(),
        content_hash: Some(hash_file(&absolute)?),
        mtime: Some(DateTime::<Utc>::from(mtime)),
    })
}

fn hash_file(path: &Path) -> anyhow::Result<String> {
    const BUFFER_SIZE: usize = 64 * 1024;
    let mut file =
        File::open(path).with_context(|| format!("open {} for hashing", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; BUFFER_SIZE];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("read {} for hashing", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

async fn extract_bounded<C>(
    connector: Arc<C>,
    items: Vec<WorkItem>,
    batch_size: usize,
) -> anyhow::Result<Vec<ExtractBatch>>
where
    C: Connector + 'static,
{
    let concurrency = batch_size.max(1);
    let mut batches = Vec::with_capacity(items.len());
    for window in items.chunks(concurrency) {
        let mut tasks = JoinSet::new();
        for item in window {
            let connector = Arc::clone(&connector);
            let item = item.clone();
            tasks.spawn(async move { connector.extract(&item).await });
        }
        while let Some(result) = tasks.join_next().await {
            batches.push(result.context("extraction task failed")??);
        }
    }
    Ok(batches)
}

fn configured_languages(source: &SourceConfig) -> anyhow::Result<Vec<Language>> {
    if source.languages.is_empty() {
        return Ok(Language::ALL.to_vec());
    }
    source
        .languages
        .iter()
        .map(|language| {
            Language::try_from(language.as_str())
                .with_context(|| format!("unsupported configured language: {language}"))
        })
        .collect()
}

fn embedder_config_from(config: &Config) -> EmbedderConfig {
    EmbedderConfig {
        kind: match config.embeddings.provider {
            EmbeddingsProvider::None => EmbedderKind::None,
            EmbeddingsProvider::Fastembed => EmbedderKind::Fastembed,
            EmbeddingsProvider::OpenaiCompatible => EmbedderKind::OpenaiCompatible,
        },
        model: config.embeddings.model.clone(),
        dimension: config.embeddings.dimension as usize,
        base_url: config.embeddings.base_url.clone(),
        api_key_env: config.embeddings.api_key_env.clone(),
    }
}

async fn clear_file_symbol_chunks(
    db: &Database,
    source_name: &str,
    path: &str,
) -> anyhow::Result<()> {
    let fqns = list_symbol_fqns_for_file(db, source_name, path).await?;
    for fqn in fqns {
        crate::semantic::remove_symbol_chunks(db, source_name, &fqn).await?;
    }
    Ok(())
}

fn resolve_import(from_file: &str, raw: &str, paths: &HashSet<String>) -> Option<String> {
    let from_parent = Path::new(from_file)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let candidates = if raw.starts_with("crate::") {
        let rest = raw.trim_start_matches("crate::").replace("::", "/");
        vec![rest]
    } else if raw.starts_with('.') {
        vec![normalize_path(&from_parent.join(raw))]
    } else if raw.contains('/') {
        vec![normalize_path(Path::new(raw))]
    } else {
        let dotted = raw.replace('.', "/");
        vec![
            normalize_path(&from_parent.join(&dotted)),
            normalize_path(Path::new(&dotted)),
        ]
    };

    let mut bases = Vec::new();
    for candidate in candidates {
        bases.push(candidate.clone());
        if let Some(parent) = Path::new(from_file).parent() {
            bases.push(normalize_path(&parent.join(&candidate)));
        }
    }

    for base in bases {
        let base = normalize_path(Path::new(&base));
        let candidates = [
            base.clone(),
            format!("{base}.rs"),
            format!("{base}.ts"),
            format!("{base}.tsx"),
            format!("{base}.py"),
            format!("{base}.rb"),
            format!("{base}/mod.rs"),
            format!("{base}/index.ts"),
            format!("{base}/index.tsx"),
            format!("{base}/__init__.py"),
        ];
        if let Some(found) = candidates
            .into_iter()
            .find(|candidate| paths.contains(candidate))
        {
            return Some(found);
        }
    }
    None
}

fn normalize_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            _ => {}
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use serde::Deserialize;

    use super::*;
    use crate::{IndexConfig, reindex_source_paths};
    use codebrain_db::{apply_schema, delete_document, open_memory};

    #[test]
    fn resolves_typescript_and_rust_imports() {
        let paths = HashSet::from([
            "src/app.ts".to_string(),
            "src/math.ts".to_string(),
            "src/auth.rs".to_string(),
        ]);

        assert_eq!(
            resolve_import("src/app.ts", "./math", &paths).as_deref(),
            Some("src/math.ts")
        );
        assert_eq!(
            resolve_import("src/lib.rs", "crate::auth", &paths).as_deref(),
            Some("src/auth.rs")
        );
    }

    #[test]
    fn resolves_ruby_require_relative() {
        let paths = HashSet::from([
            "ruby/app.rb".to_string(),
            "ruby/services/greeter.rb".to_string(),
        ]);

        assert_eq!(
            resolve_import("ruby/app.rb", "services/greeter", &paths).as_deref(),
            Some("ruby/services/greeter.rb")
        );
    }

    #[derive(Debug, Deserialize)]
    struct CountRow {
        count: i64,
    }

    #[tokio::test]
    async fn indexes_fixture_and_second_run_has_zero_content_writes() {
        let db = open_memory().await.expect("open db");
        apply_schema(&db).await.expect("schema");
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/repo-mini")
            .canonicalize()
            .expect("fixture path");
        let config = Config {
            sources: HashMap::from([(
                "fixture".into(),
                SourceConfig {
                    kind: SourceKindConfig::GitRepo,
                    path: fixture.to_string_lossy().into_owned(),
                    languages: vec![
                        "rust".into(),
                        "typescript".into(),
                        "python".into(),
                        "ruby".into(),
                    ],
                    ..Default::default()
                },
            )]),
            index: IndexConfig {
                batch_size: 2,
                ..IndexConfig::default()
            },
            ..Config::default()
        };

        let first = index_configured_sources(&db, &config, Some("fixture"), false)
            .await
            .expect("first index");
        let second = index_configured_sources(&db, &config, Some("fixture"), false)
            .await
            .expect("second index");

        assert_eq!(first.discovered(), 8);
        assert_eq!(first.indexed(), 8);
        assert!(first.sources[0].symbols >= 8);
        assert!(first.sources[0].imports >= 3);
        assert_eq!(second.indexed(), 0);
        assert_eq!(second.skipped(), 8);
        assert!(count(&db, "imports").await >= 3);

        let forced = index_configured_sources(&db, &config, Some("fixture"), true)
            .await
            .expect("forced index");
        assert_eq!(forced.indexed(), 8);
        assert_eq!(forced.skipped(), 0);
    }

    #[tokio::test]
    async fn indexes_vault_fixture_with_references_mentions_and_broken_links() {
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

        index_configured_sources(&db, &config, Some("code"), false)
            .await
            .expect("index code");
        let vault_report = index_configured_sources(&db, &config, Some("notes"), false)
            .await
            .expect("index vault");

        assert_eq!(vault_report.discovered(), 3);
        assert_eq!(vault_report.indexed(), 3);
        assert!(vault_report.sources[0].references >= 1);
        assert!(vault_report.sources[0].mentions >= 1);
        assert!(vault_report.sources[0].broken_links >= 1);
        assert!(count(&db, "references").await >= 1);
        assert!(count(&db, "mentions").await >= 1);

        let second = index_configured_sources(&db, &config, Some("notes"), false)
            .await
            .expect("second vault index");
        assert_eq!(second.indexed(), 0);
        assert_eq!(second.skipped(), 3);
    }

    #[tokio::test]
    async fn partial_reindex_touches_only_requested_markdown() {
        let db = open_memory().await.expect("open db");
        apply_schema(&db).await.expect("schema");

        let vault = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/vault-mini")
            .canonicalize()
            .expect("vault fixture");

        let config = Config {
            sources: HashMap::from([(
                "notes".into(),
                SourceConfig {
                    kind: SourceKindConfig::ObsidianVault,
                    path: vault.to_string_lossy().into_owned(),
                    languages: Vec::new(),
                    ..Default::default()
                },
            )]),
            index: IndexConfig {
                batch_size: 2,
                ..IndexConfig::default()
            },
            ..Config::default()
        };

        index_configured_sources(&db, &config, Some("notes"), false)
            .await
            .expect("full vault index");

        // Force a content change by reindexing a single known note after bumping its hash
        // via a temporary copy would be ideal; here we delete one doc then partial-restore.
        let target = "Note A.md";
        delete_document(&db, "notes", target)
            .await
            .expect("delete one note");

        let partial = reindex_source_paths(&db, &config, "notes", &[target.into()])
            .await
            .expect("partial reindex");

        assert_eq!(partial.discovered, 1);
        assert_eq!(partial.indexed, 1);
        assert_eq!(partial.documents, 1);
        assert_eq!(partial.removed, 0);

        let untouched = reindex_source_paths(&db, &config, "notes", &[target.into()])
            .await
            .expect("second partial");
        assert_eq!(untouched.indexed, 0);
        assert_eq!(untouched.skipped, 1);
    }

    async fn count(db: &Database, table: &str) -> i64 {
        let sql = format!("SELECT count() AS count FROM {table} GROUP ALL;");
        let mut response = db.query(sql).await.expect("query count");
        let rows: Vec<CountRow> = response.take(0).expect("take count");
        rows.first().map_or(0, |row| row.count)
    }
}
