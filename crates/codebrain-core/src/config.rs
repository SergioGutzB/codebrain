use std::collections::HashMap;
use std::path::{Path, PathBuf};

use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

use crate::paths::{default_data_dir, expand_path};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub database: DatabaseConfig,
    #[serde(default)]
    pub sources: HashMap<String, SourceConfig>,
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
    #[serde(default)]
    pub index: IndexConfig,
    #[serde(default)]
    pub linker: LinkerConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub adr: AdrConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Directory for the embedded SurrealKV store.
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_data_dir().join("db").to_string_lossy().into_owned(),
        }
    }
}

impl DatabaseConfig {
    pub fn resolved_path(&self) -> PathBuf {
        expand_path(&self.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub kind: SourceKindConfig,
    /// Filesystem root for git/obsidian; for Jira, optional base URL override.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub languages: Vec<String>,
    /// Jira JQL used when `kind = jira`.
    #[serde(default)]
    pub jql: Option<String>,
    /// Confluence CQL used when `kind = confluence` (falls back to `jql` if unset).
    #[serde(default)]
    pub cql: Option<String>,
    /// Notion search query used when `kind = notion` (falls back to `jql` if unset).
    #[serde(default)]
    pub query: Option<String>,
    /// Max issues/pages to pull per SaaS index run.
    #[serde(default = "default_max_issues")]
    pub max_issues: usize,
    /// Env var holding Jira site URL (default `JIRA_BASE_URL`).
    #[serde(default = "default_jira_base_url_env")]
    pub base_url_env: String,
    /// Env var holding Jira email (default `JIRA_EMAIL`).
    #[serde(default = "default_jira_email_env")]
    pub email_env: String,
    /// Env var holding Jira API token (default `JIRA_API_TOKEN`).
    #[serde(default = "default_jira_token_env")]
    pub token_env: String,
}

fn default_max_issues() -> usize {
    100
}

fn default_jira_base_url_env() -> String {
    "JIRA_BASE_URL".into()
}

fn default_jira_email_env() -> String {
    "JIRA_EMAIL".into()
}

fn default_jira_token_env() -> String {
    "JIRA_API_TOKEN".into()
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            kind: SourceKindConfig::GitRepo,
            path: String::new(),
            languages: Vec::new(),
            jql: None,
            cql: None,
            query: None,
            max_issues: default_max_issues(),
            base_url_env: default_jira_base_url_env(),
            email_env: default_jira_email_env(),
            token_env: default_jira_token_env(),
        }
    }
}

impl SourceConfig {
    pub fn resolved_path(&self) -> PathBuf {
        expand_path(&self.path)
    }

    pub fn jira_auth(&self) -> anyhow::Result<codebrain_connector_saas::JiraAuth> {
        self.atlassian_auth()
    }

    /// Atlassian Cloud auth shared by Jira and Confluence (same site API token).
    pub fn atlassian_auth(&self) -> anyhow::Result<codebrain_connector_saas::JiraAuth> {
        let base_url = if self.path.trim().starts_with("http") {
            self.path.trim().trim_end_matches('/').to_string()
        } else {
            std::env::var(&self.base_url_env).map_err(|_| {
                anyhow::anyhow!(
                    "set {} or sources.*.path to your Atlassian site URL",
                    self.base_url_env
                )
            })?
        };
        let email = std::env::var(&self.email_env)
            .map_err(|_| anyhow::anyhow!("missing env {}", self.email_env))?;
        let api_token = std::env::var(&self.token_env)
            .map_err(|_| anyhow::anyhow!("missing env {}", self.token_env))?;
        Ok(codebrain_connector_saas::JiraAuth {
            base_url: base_url.trim_end_matches('/').to_string(),
            email,
            api_token,
        })
    }

    pub fn confluence_cql(&self) -> String {
        self.cql
            .clone()
            .or_else(|| self.jql.clone())
            .unwrap_or_else(|| "type = page ORDER BY lastmodified DESC".into())
    }

    /// Notion integration token (`NOTION_TOKEN` / `NOTION_API_KEY`, or `token_env`).
    pub fn notion_auth(&self) -> anyhow::Result<codebrain_connector_saas::NotionAuth> {
        let candidates = if self.token_env != default_jira_token_env() {
            vec![self.token_env.clone()]
        } else {
            vec![
                "NOTION_TOKEN".into(),
                "NOTION_API_KEY".into(),
                self.token_env.clone(),
            ]
        };
        for key in candidates {
            if let Ok(token) = std::env::var(&key) {
                let token = token.trim().to_string();
                if !token.is_empty() {
                    return Ok(codebrain_connector_saas::NotionAuth { token });
                }
            }
        }
        anyhow::bail!(
            "set NOTION_TOKEN (or NOTION_API_KEY / token_env) for Notion integration secret"
        )
    }

    pub fn notion_query(&self) -> String {
        self.query
            .clone()
            .or_else(|| self.jql.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKindConfig {
    GitRepo,
    ObsidianVault,
    Notion,
    Confluence,
    Jira,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    pub provider: EmbeddingsProvider,
    pub model: String,
    pub dimension: u32,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            provider: EmbeddingsProvider::None,
            model: "all-MiniLM-L6-v2".into(),
            dimension: 384,
            base_url: None,
            api_key_env: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingsProvider {
    Fastembed,
    OpenaiCompatible,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    pub watch: bool,
    pub batch_size: usize,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default = "default_excludes")]
    pub exclude: Vec<String>,
}

fn default_debounce_ms() -> u64 {
    750
}

fn default_excludes() -> Vec<String> {
    vec![
        "**/node_modules/**".into(),
        "**/target/**".into(),
        "**/.git/**".into(),
        "**/.obsidian/**".into(),
    ]
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            watch: false,
            batch_size: 64,
            debounce_ms: default_debounce_ms(),
            exclude: default_excludes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkerConfig {
    pub mention_threshold: f32,
    pub auto_promote_explains: bool,
}

impl Default for LinkerConfig {
    fn default() -> Self {
        Self {
            mention_threshold: 0.75,
            auto_promote_explains: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// `stdio` (default) or `http` (streamable HTTP for local teams).
    pub transport: String,
    /// Bind address for HTTP transport (loopback recommended).
    #[serde(default = "default_mcp_bind")]
    pub bind: String,
    /// When false (default), HTTP refuses non-loopback binds.
    #[serde(default)]
    pub allow_remote: bool,
}

fn default_mcp_bind() -> String {
    "127.0.0.1:8765".into()
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            transport: "stdio".into(),
            bind: default_mcp_bind(),
            allow_remote: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdrConfig {
    /// When true, also write a Markdown note into the configured vault source.
    pub write_vault: bool,
    /// Name of an `obsidian_vault` source that receives ADR notes.
    #[serde(default = "default_adr_vault_source")]
    pub vault_source: String,
    /// Folder inside the vault (created if missing).
    #[serde(default = "default_adr_directory")]
    pub directory: String,
    #[serde(default = "default_adr_created_by")]
    pub created_by: String,
}

fn default_adr_vault_source() -> String {
    "notes".into()
}

fn default_adr_directory() -> String {
    "ADR".into()
}

fn default_adr_created_by() -> String {
    "agent".into()
}

impl Default for AdrConfig {
    fn default() -> Self {
        Self {
            write_vault: false,
            vault_source: default_adr_vault_source(),
            directory: default_adr_directory(),
            created_by: default_adr_created_by(),
        }
    }
}

/// Load configuration from defaults < optional TOML file < `CODEBRAIN_*` env.
pub fn load_config(config_file: Option<&Path>) -> anyhow::Result<Config> {
    let mut figment = Figment::new().merge(Serialized::defaults(Config::default()));

    if let Some(path) = config_file {
        if path.exists() {
            figment = figment.merge(Toml::file(path));
        }
    }

    figment = figment.merge(Env::prefixed("CODEBRAIN_").split("__"));

    Ok(figment.extract()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_deserializes() {
        let cfg = Config::default();
        assert_eq!(cfg.embeddings.dimension, 384);
        assert!(!cfg.index.exclude.is_empty());
    }
}
