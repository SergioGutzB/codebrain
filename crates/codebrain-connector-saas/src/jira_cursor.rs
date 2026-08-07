//! Jira incremental sync helpers (`updated` cursor → JQL).

use chrono::{DateTime, Local, Utc};

/// Meta key storing the last seen Jira `updated` timestamp (RFC3339) for a source.
pub fn jira_updated_cursor_key(source_name: &str) -> String {
    format!("jira.{source_name}.updated_cursor")
}

/// Build JQL for an incremental pull using a persisted `updated` cursor.
///
/// When `cursor` is `None`, returns the base JQL unchanged (full sync).
/// Otherwise wraps the base (ORDER BY stripped) with `updated >= "…"` and
/// sorts ascending so the high-water mark advances cleanly.
///
/// Jira interprets bare `"YYYY/MM/DD HH:MM"` in the **user/site timezone**, so the
/// cursor (stored as UTC) is formatted in local time before embedding in JQL.
pub fn jira_jql_with_updated_cursor(base_jql: &str, cursor: Option<&DateTime<Utc>>) -> String {
    let base = base_jql.trim();
    let Some(cursor) = cursor else {
        return base.to_string();
    };
    let body = strip_order_by(base);
    let body = if body.is_empty() {
        "updated >= -3650d".to_string()
    } else {
        body
    };
    // Minute precision; content-hash skip dedupes boundary re-fetches.
    let formatted = cursor
        .with_timezone(&Local)
        .format("%Y/%m/%d %H:%M")
        .to_string();
    format!("({body}) AND updated >= \"{formatted}\" ORDER BY updated ASC")
}

/// Strip a trailing `ORDER BY …` clause (case-insensitive).
fn strip_order_by(jql: &str) -> String {
    let lower = jql.to_ascii_lowercase();
    if let Some(idx) = lower.rfind(" order by ") {
        jql[..idx].trim().to_string()
    } else {
        jql.trim().to_string()
    }
}

/// Pick the newest `updated` timestamp from raw Jira issue timestamps.
pub fn max_jira_updated<'a, I>(values: I) -> Option<DateTime<Utc>>
where
    I: IntoIterator<Item = &'a str>,
{
    values.into_iter().filter_map(parse_jira_updated).max()
}

pub fn parse_jira_updated(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            // Jira sometimes returns +0000 without colon.
            DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.3f%z")
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    #[test]
    fn full_sync_keeps_base_jql() {
        let base = "assignee = currentUser() ORDER BY updated DESC";
        assert_eq!(jira_jql_with_updated_cursor(base, None), base);
    }

    #[test]
    fn incremental_wraps_and_orders_asc() {
        let base = "project = PPS AND updated >= -30d ORDER BY updated DESC";
        let cursor = Utc.with_ymd_and_hms(2026, 8, 7, 15, 30, 0).unwrap();
        let jql = jira_jql_with_updated_cursor(base, Some(&cursor));
        let local = cursor
            .with_timezone(&Local)
            .format("%Y/%m/%d %H:%M")
            .to_string();
        assert_eq!(
            jql,
            format!(
                "(project = PPS AND updated >= -30d) AND updated >= \"{local}\" ORDER BY updated ASC"
            )
        );
    }

    #[test]
    fn cursor_key_is_namespaced() {
        assert_eq!(
            jira_updated_cursor_key("tickets"),
            "jira.tickets.updated_cursor"
        );
    }

    #[test]
    fn max_updated_picks_latest() {
        let max = max_jira_updated([
            "2026-08-01T10:00:00.000+0000",
            "2026-08-07T12:00:00.000+0000",
            "2026-08-05T09:00:00.000+0000",
        ])
        .unwrap();
        assert_eq!(max, Utc.with_ymd_and_hms(2026, 8, 7, 12, 0, 0).unwrap());
    }
}
