//! Debounced filesystem watchers that enqueue partial reindex jobs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::{Config, SourceKindConfig, reindex_source_paths};
use codebrain_db::Database;

/// Partial reindex work produced after debounce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReindexJob {
    pub source: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Default)]
pub struct PendingChanges {
    by_source: HashMap<String, HashSet<String>>,
}

impl PendingChanges {
    pub fn push(&mut self, source: impl Into<String>, path: impl Into<String>) {
        self.by_source
            .entry(source.into())
            .or_default()
            .insert(path.into());
    }

    pub fn is_empty(&self) -> bool {
        self.by_source.values().all(HashSet::is_empty)
    }

    pub fn drain_jobs(&mut self) -> Vec<ReindexJob> {
        let mut jobs = Vec::new();
        for (source, paths) in self.by_source.drain() {
            if paths.is_empty() {
                continue;
            }
            let mut paths: Vec<_> = paths.into_iter().collect();
            paths.sort_unstable();
            jobs.push(ReindexJob { source, paths });
        }
        jobs.sort_by(|left, right| left.source.cmp(&right.source));
        jobs
    }
}

/// Spawn recursive watchers for every configured source and coalesce events.
///
/// Returns a join handle for the debounce loop. Dropping `job_tx` / closing the
/// notify channel ends the loop. The watcher itself is kept alive inside the task.
pub fn spawn_watchers(
    config: Config,
    job_tx: mpsc::Sender<ReindexJob>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let debounce = Duration::from_millis(config.index.debounce_ms.max(50));
    let (fs_tx, mut fs_rx) = mpsc::unbounded_channel::<FsEvent>();

    let mut watcher =
        notify::recommended_watcher(move |result: Result<Event, notify::Error>| match result {
            Ok(event) => {
                let kind = event.kind;
                for path in event.paths {
                    let _ = fs_tx.send(FsEvent { path, kind });
                }
            }
            Err(error) => tracing::warn!(%error, "filesystem watch error"),
        })
        .context("create filesystem watcher")?;

    let mut roots = Vec::new();
    for (name, source) in &config.sources {
        let root = source.resolved_path();
        if !root.is_dir() {
            tracing::warn!(
                source = %name,
                path = %root.display(),
                "skipping watch; root is not a directory"
            );
            continue;
        }
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .with_context(|| format!("watch source {name} at {}", root.display()))?;
        roots.push(WatchedRoot {
            name: name.clone(),
            root,
            kind: source.kind,
            languages: source.languages.clone(),
            excludes: config.index.exclude.clone(),
        });
        tracing::info!(source = %name, path = %source.resolved_path().display(), "watching source");
    }

    if roots.is_empty() {
        anyhow::bail!("no watchable sources configured");
    }

    let pending = Arc::new(Mutex::new(PendingChanges::default()));
    let pending_flush = Arc::clone(&pending);

    let handle = tokio::spawn(async move {
        // Keep the watcher alive for the lifetime of this task.
        let _watcher: RecommendedWatcher = watcher;
        let mut flush = tokio::time::interval_at(tokio::time::Instant::now() + debounce, debounce);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                event = fs_rx.recv() => {
                    let Some(event) = event else { break };
                    if matches!(event.kind, EventKind::Access(_)) {
                        continue;
                    }
                    for root in &roots {
                        if let Some(relative) = classify_event(root, &event.path) {
                            if let Ok(mut guard) = pending.lock() {
                                guard.push(root.name.clone(), relative);
                            }
                        }
                    }
                }
                _ = flush.tick() => {
                    let jobs = match pending_flush.lock() {
                        Ok(mut guard) => guard.drain_jobs(),
                        Err(_) => break,
                    };
                    for job in jobs {
                        tracing::info!(
                            source = %job.source,
                            paths = job.paths.len(),
                            "enqueue partial reindex"
                        );
                        if job_tx.send(job).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }
    });

    Ok(handle)
}

/// Consume reindex jobs without blocking the MCP stdio loop.
pub async fn run_reindex_worker(
    db: Database,
    config: Config,
    mut jobs: mpsc::Receiver<ReindexJob>,
) {
    while let Some(job) = jobs.recv().await {
        match reindex_source_paths(&db, &config, &job.source, &job.paths).await {
            Ok(report) => {
                tracing::info!(
                    source = %report.source,
                    indexed = report.indexed,
                    removed = report.removed,
                    skipped = report.skipped,
                    "partial reindex complete"
                );
            }
            Err(error) => {
                tracing::error!(
                    source = %job.source,
                    paths = ?job.paths,
                    %error,
                    "partial reindex failed"
                );
            }
        }
    }
}

#[derive(Debug, Clone)]
struct WatchedRoot {
    name: String,
    root: PathBuf,
    kind: SourceKindConfig,
    languages: Vec<String>,
    excludes: Vec<String>,
}

#[derive(Debug)]
struct FsEvent {
    path: PathBuf,
    kind: EventKind,
}

fn classify_event(root: &WatchedRoot, absolute: &Path) -> Option<String> {
    let relative = absolute.strip_prefix(&root.root).ok()?;
    let normalized = relative.to_string_lossy().replace('\\', "/");
    if normalized.is_empty() || normalized.contains("..") {
        return None;
    }
    if excluded(&normalized, &root.excludes) {
        return None;
    }
    match root.kind {
        SourceKindConfig::ObsidianVault => {
            if normalized.ends_with(".md") {
                Some(normalized)
            } else {
                None
            }
        }
        SourceKindConfig::GitRepo => {
            if is_code_path(&normalized, &root.languages) {
                Some(normalized)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_code_path(path: &str, languages: &[String]) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    let Some(extension) = extension else {
        return false;
    };
    let allowed = if languages.is_empty() {
        ["rs", "ts", "tsx", "py", "rb"].as_slice()
    } else {
        // Map language names to extensions roughly; unknown languages ignore filter.
        return languages.iter().any(|language| match language.as_str() {
            "rust" => extension == "rs",
            "typescript" => extension == "ts" || extension == "tsx",
            "python" => extension == "py",
            "ruby" => extension == "rb",
            _ => false,
        });
    };
    allowed.iter().any(|candidate| *candidate == extension)
}

fn excluded(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let trimmed = pattern.trim_matches('*').trim_matches('/');
        !trimmed.is_empty() && path.contains(trimmed.trim_end_matches("/**"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_changes_coalesce_and_sort() {
        let mut pending = PendingChanges::default();
        pending.push("notes", "A.md");
        pending.push("notes", "B.md");
        pending.push("notes", "A.md");
        pending.push("code", "lib.rs");

        let jobs = pending.drain_jobs();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].source, "code");
        assert_eq!(jobs[0].paths, vec!["lib.rs".to_string()]);
        assert_eq!(jobs[1].source, "notes");
        assert_eq!(jobs[1].paths, vec!["A.md".to_string(), "B.md".to_string()]);
        assert!(pending.is_empty());
    }

    #[test]
    fn classify_markdown_and_code() {
        let vault = WatchedRoot {
            name: "notes".into(),
            root: PathBuf::from("/vault"),
            kind: SourceKindConfig::ObsidianVault,
            languages: Vec::new(),
            excludes: vec!["**/.obsidian/**".into()],
        };
        assert_eq!(
            classify_event(&vault, Path::new("/vault/Design.md")).as_deref(),
            Some("Design.md")
        );
        assert_eq!(
            classify_event(&vault, Path::new("/vault/.obsidian/app.json")),
            None
        );

        let code = WatchedRoot {
            name: "code".into(),
            root: PathBuf::from("/repo"),
            kind: SourceKindConfig::GitRepo,
            languages: vec!["ruby".into()],
            excludes: vec!["**/target/**".into()],
        };
        assert_eq!(
            classify_event(&code, Path::new("/repo/app.rb")).as_deref(),
            Some("app.rb")
        );
        assert_eq!(classify_event(&code, Path::new("/repo/app.ts")), None);
    }
}
