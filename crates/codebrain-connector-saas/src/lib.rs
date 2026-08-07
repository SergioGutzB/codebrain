//! SaaS connectors — Jira + Confluence + Notion (v1.1).

mod adf;
mod client;
mod confluence;
mod confluence_client;
mod error;
mod html;
mod jira;
mod jira_cursor;
mod keys;
mod notion;
mod notion_client;
mod notion_text;

pub use client::{JiraAuth, JiraClient, JiraIssue, issue_content_hash, render_issue_body};
pub use confluence::ConfluenceConnector;
pub use confluence_client::{
    ConfluenceClient, ConfluencePage, page_content_hash as confluence_page_content_hash,
    render_page_body as render_confluence_page_body,
};
pub use error::{Result, SaasError};
pub use jira::JiraConnector;
pub use jira_cursor::{
    jira_jql_with_updated_cursor, jira_updated_cursor_key, max_jira_updated, parse_jira_updated,
};
pub use keys::find_issue_keys;
pub use notion::NotionConnector;
pub use notion_client::{
    NotionAuth, NotionClient, NotionPage, page_content_hash as notion_page_content_hash,
    render_page_body as render_notion_page_body,
};

/// Shared Atlassian Cloud auth (Jira + Confluence API token).
pub type AtlassianAuth = JiraAuth;
