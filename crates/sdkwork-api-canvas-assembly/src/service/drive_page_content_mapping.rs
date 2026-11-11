//! Drive version -> Canvas page-content snapshot mapping.
//!
//! One mapping is shared by the in-process Drive port and the opt-in HTTP
//! adapter so switching `SDKWORK_CANVAS_DRIVE_MODE` can never change a Canvas
//! API response (MODULE_SPEC.md §4.2.7: no duplicated DTO mapping).
//!
//! The version-summary shape is deliberately the pre-migration one: the Drive
//! app-api `FileVersion` DTO did not expose `version_kind`, so the old adapter
//! derived it from the version number. Reproducing that derivation keeps the
//! `versionKind` values callers already receive.

use serde_json::Value;

use sdkwork_canvas_pages_service::domain::{DrivePageContentSnapshot, DriveVersionSummary};

/// Canonical Drive URI Canvas persists for a page-content node.
pub(crate) fn drive_uri(drive_space_id: &str, drive_node_id: &str) -> String {
    format!("drive://spaces/{drive_space_id}/nodes/{drive_node_id}")
}

/// Everything needed to present one Drive file version as page content.
pub(crate) struct PageSnapshotInput<'a> {
    pub drive_space_id: &'a str,
    pub drive_node_id: &'a str,
    pub drive_version_id: &'a str,
    pub drive_version_no: i64,
    pub content_type: &'a str,
    pub content_schema_version: &'a str,
    pub content_hash: Option<String>,
    pub content: Value,
}

/// Builds the snapshot both transports return.
pub(crate) fn build_snapshot(input: PageSnapshotInput<'_>) -> DrivePageContentSnapshot {
    let (snippet, word_count, task_count) = extract_content_metrics(&input.content);
    DrivePageContentSnapshot {
        drive_space_id: input.drive_space_id.to_string(),
        drive_node_id: input.drive_node_id.to_string(),
        drive_uri: drive_uri(input.drive_space_id, input.drive_node_id),
        drive_version_id: input.drive_version_id.to_string(),
        drive_version_no: input.drive_version_no,
        content_type: input.content_type.to_string(),
        content_schema_version: input.content_schema_version.to_string(),
        content_hash: input.content_hash,
        snippet,
        word_count,
        task_count,
        content: input.content,
    }
}

/// Builds one `DriveVersionSummary` exactly as the pre-migration adapter did.
pub(crate) fn build_version_summary(
    drive_version_id: &str,
    drive_version_no: i64,
    created_at: &str,
) -> DriveVersionSummary {
    DriveVersionSummary {
        drive_version_id: drive_version_id.to_string(),
        drive_version_no,
        version_kind: if drive_version_no <= 1 {
            "initial".to_string()
        } else {
            "auto".to_string()
        },
        version_label: None,
        change_summary: None,
        created_at: created_at.to_string(),
    }
}

/// Derives the list/search metrics Canvas stores beside the content.
pub(crate) fn extract_content_metrics(content: &Value) -> (Option<String>, i64, i64) {
    let snippet = content
        .get("title")
        .or_else(|| content.get("snippet"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let word_count = content
        .get("wordCount")
        .or_else(|| content.get("word_count"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let task_count = content
        .get("taskCount")
        .or_else(|| content.get("task_count"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    (snippet, word_count, task_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_kind_keeps_the_pre_migration_derivation() {
        assert_eq!(build_version_summary("v1", 1, "t").version_kind, "initial");
        assert_eq!(build_version_summary("v2", 2, "t").version_kind, "auto");
        assert_eq!(build_version_summary("v0", 0, "t").version_kind, "initial");
    }

    #[test]
    fn summaries_carry_no_optional_labels() {
        let summary = build_version_summary("v3", 3, "2026-01-01T00:00:00Z");
        assert!(summary.version_label.is_none());
        assert!(summary.change_summary.is_none());
        assert_eq!(summary.created_at, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn snapshot_metrics_come_from_the_canvas_content_shape() {
        let content = json!({
            "title": "Launch plan",
            "wordCount": 12,
            "taskCount": 3,
        });
        let snapshot = build_snapshot(PageSnapshotInput {
            drive_space_id: "space-1",
            drive_node_id: "node-1",
            drive_version_id: "version-1",
            drive_version_no: 2,
            content_type: "application/json",
            content_schema_version: "canvas/v1",
            content_hash: Some("sha256:abc".to_string()),
            content,
        });
        assert_eq!(snapshot.drive_uri, "drive://spaces/space-1/nodes/node-1");
        assert_eq!(snapshot.snippet.as_deref(), Some("Launch plan"));
        assert_eq!(snapshot.word_count, 12);
        assert_eq!(snapshot.task_count, 3);
        assert_eq!(snapshot.content_hash.as_deref(), Some("sha256:abc"));
    }

    #[test]
    fn missing_metrics_default_to_zero_without_a_snippet() {
        let content = json!({ "snippet": "summary" });
        let snapshot = build_snapshot(PageSnapshotInput {
            drive_space_id: "space-1",
            drive_node_id: "node-1",
            drive_version_id: "version-1",
            drive_version_no: 1,
            content_type: "application/json",
            content_schema_version: "canvas/v1",
            content_hash: None,
            content,
        });
        assert_eq!(snapshot.snippet.as_deref(), Some("summary"));
        assert_eq!(snapshot.word_count, 0);
        assert_eq!(snapshot.task_count, 0);
    }
}
