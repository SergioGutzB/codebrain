use std::path::PathBuf;

use directories::ProjectDirs;

/// Expand `~` and environment variables in a path string.
pub fn expand_path(raw: &str) -> PathBuf {
    let cow = shellexpand::full(raw).unwrap_or_else(|_| raw.into());
    PathBuf::from(cow.as_ref())
}

/// Default data directory: `$XDG_DATA_HOME/codebrain` or platform equivalent.
pub fn default_data_dir() -> PathBuf {
    if let Some(dirs) = ProjectDirs::from("dev", "CodeBrain", "codebrain") {
        dirs.data_dir().to_path_buf()
    } else {
        PathBuf::from(".codebrain")
    }
}

/// Default config path next to the data dir's parent config location, or cwd.
pub fn default_config_path() -> PathBuf {
    if let Some(dirs) = ProjectDirs::from("dev", "CodeBrain", "codebrain") {
        dirs.config_dir().join("codebrain.toml")
    } else {
        PathBuf::from("codebrain.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_home() {
        let path = expand_path("~/codebrain-test");
        assert!(!path.to_string_lossy().contains('~'));
    }
}
