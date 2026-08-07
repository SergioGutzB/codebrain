use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use codebrain_connector::{IndexContext, WorkItem};
use ignore::WalkBuilder;

use crate::error::{CodeConnectorError, Result};
use crate::language::Language;

pub async fn discover(
    ctx: &IndexContext,
    enabled_languages: &HashSet<Language>,
) -> Result<Vec<WorkItem>> {
    let ctx = ctx.clone();
    let enabled_languages = enabled_languages.clone();
    tokio::task::spawn_blocking(move || discover_blocking(&ctx, &enabled_languages)).await?
}

fn discover_blocking(
    ctx: &IndexContext,
    enabled_languages: &HashSet<Language>,
) -> Result<Vec<WorkItem>> {
    if !ctx.root_path.is_dir() {
        return Err(CodeConnectorError::InvalidRoot(ctx.root_path.clone()));
    }

    let mut builder = WalkBuilder::new(&ctx.root_path);
    builder
        .standard_filters(true)
        .hidden(false)
        .follow_links(false);

    let mut items = Vec::new();
    for entry in builder.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(%error, "skipping inaccessible path during discovery");
                continue;
            }
        };
        let path = entry.path();
        if !entry.file_type().is_some_and(|kind| kind.is_file())
            || excluded(path, &ctx.root_path, &ctx.excludes)
        {
            continue;
        }

        let Some(language) = Language::from_path(path) else {
            continue;
        };
        if !enabled_languages.is_empty() && !enabled_languages.contains(&language) {
            continue;
        }

        items.push(work_item(path, &ctx.root_path)?);
    }
    items.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    Ok(items)
}

fn work_item(path: &Path, root: &Path) -> Result<WorkItem> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| CodeConnectorError::InvalidPath(path.to_path_buf()))?;
    let id = relative
        .to_str()
        .ok_or_else(|| CodeConnectorError::InvalidPath(relative.to_path_buf()))?
        .replace('\\', "/");
    let metadata = path.metadata().map_err(|source| CodeConnectorError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    Ok(WorkItem {
        id,
        path: path.to_string_lossy().into_owned(),
        content_hash: Some(hash_file(path)?),
        mtime: Some(DateTime::<Utc>::from(mtime)),
    })
}

fn hash_file(path: &Path) -> Result<String> {
    const BUFFER_SIZE: usize = 64 * 1024;
    let mut file = File::open(path).map_err(|source| CodeConnectorError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; BUFFER_SIZE];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| CodeConnectorError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn excluded(path: &Path, root: &Path, patterns: &[String]) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let normalized = relative.to_string_lossy().replace('\\', "/");
    patterns
        .iter()
        .any(|pattern| simple_glob_match(pattern, &normalized))
}

fn simple_glob_match(pattern: &str, path: &str) -> bool {
    let trimmed = pattern.trim_matches('*').trim_matches('/');
    !trimmed.is_empty() && path.contains(trimmed.trim_end_matches("/**"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn discovers_supported_files_and_hashes_content() {
        let root = tempdir().expect("temp dir");
        fs::write(root.path().join("lib.rs"), "fn main() {}").expect("write fixture");
        fs::write(root.path().join("notes.md"), "# ignored").expect("write fixture");
        let ctx = IndexContext {
            source_name: "fixture".into(),
            root_path: root.path().to_path_buf(),
            excludes: Vec::new(),
        };

        let items = discover(&ctx, &HashSet::new()).await.expect("discover");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "lib.rs");
        assert!(items[0].content_hash.is_some());
    }
}
