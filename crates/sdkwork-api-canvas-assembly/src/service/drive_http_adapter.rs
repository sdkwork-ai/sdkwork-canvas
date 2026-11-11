//! Opt-in split-deployment Drive adapter (MODULE_SPEC.md §4.2.3).
//!
//! This module is **never** selected by default. Canvas reaches Drive
//! in-process (`drive_in_process.rs`); a deployment that really runs Drive in
//! another process or container opts in with:
//!
//! ```text
//! SDKWORK_CANVAS_DRIVE_MODE=http
//! SDKWORK_DRIVE_FACADE_URL=https://drive.sdkwork.com
//! ```
//!
//! The opt-in is validated at startup: [`HttpDrivePageContentPort::from_env`]
//! fails when neither the configured endpoint nor the process service registry
//! yields a Drive base URL, so a misspelled split deployment aborts the boot
//! instead of failing the first page save. The variable is recorded in
//! `specs/backend-module-call-boundary.allow.json` with its reason.

use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use reqwest::Client;
use sha2::{Digest, Sha256};

use sdkwork_canvas_pages_service::domain::{
    DrivePageContentSnapshot, DriveVersionPage, DriveVersionSummary, PageInfo,
};
use sdkwork_canvas_pages_service::error::CanvasProductError;
use sdkwork_canvas_pages_service::ports::{
    CreateDrivePageContentCommand, DrivePageContentPort, ListDrivePageContentVersionsCommand,
    ReadDrivePageContentCommand, RestoreDrivePageContentVersionCommand,
    UpdateDrivePageContentCommand,
};
use sdkwork_drive_app_sdk_generated_rust::{
    CompleteUploadSessionRequest, CompletedUploadPart, CreateDownloadUrlRequest,
    CreateUploadSessionRequest, FileVersion, MarkUploaderPartUploadedRequest, NodeCommandRequest,
    PrepareUploaderUploadRequest, PresignUploadPartRequest, SdkworkAppClient, SdkworkError,
};
use sdkwork_utils_rust::service_base_url::registry_base_url_or;
use sdkwork_utils_rust::string::{is_blank, trim};

use crate::service::drive_page_content_mapping::{
    build_snapshot, build_version_summary, PageSnapshotInput,
};

/// The only endpoint variable an HTTP-mode Canvas deployment may read. Read
/// here (not in `drive_port.rs`) so the default path cannot reference it.
pub(crate) const DRIVE_FACADE_URL_ENV: &str = "SDKWORK_DRIVE_FACADE_URL";

const CANVAS_RESOURCE_TYPE: &str = "canvas_page";
const CANVAS_UPLOAD_PROFILE: &str = "generic";
const CANVAS_CONTENT_SCENE: &str = "canvas_page_content";
const CANVAS_CONTENT_SOURCE: &str = "canvas_product_service";

/// Split-deployment Drive adapter backed by the generated Drive App SDK.
#[derive(Clone)]
pub struct HttpDrivePageContentPort {
    client: SdkworkAppClient,
    http: Client,
}

impl HttpDrivePageContentPort {
    /// Builds the adapter for an explicit `SDKWORK_CANVAS_DRIVE_MODE=http`
    /// deployment.
    ///
    /// # Errors
    ///
    /// Fails at startup when no Drive base URL is configured or discoverable.
    pub fn from_env() -> Result<Self, String> {
        let configured = std::env::var(DRIVE_FACADE_URL_ENV).ok().unwrap_or_default();
        // DISCOVERY_SPEC §12.1: registry-first, configured URL as degradation.
        let base_url = registry_base_url_or("sdkwork-drive", &trim(&configured));
        if is_blank(Some(base_url.as_str())) {
            return Err(format!(
                "SDKWORK_CANVAS_DRIVE_MODE=http requires {DRIVE_FACADE_URL_ENV} (or a healthy \
                 sdkwork-drive entry in the process service registry); refusing to boot without \
                 a Drive endpoint"
            ));
        }
        let client = SdkworkAppClient::new_with_base_url(base_url)
            .map_err(|error| format!("initialize Drive SDK client failed: {error}"))?;
        if let Ok(token) = std::env::var("SDKWORK_ACCESS_TOKEN") {
            let trimmed = trim(&token);
            if !is_blank(Some(trimmed.as_str())) {
                client.set_access_token(trimmed.as_str());
            }
        }
        let http = Client::builder()
            .build()
            .map_err(|error| format!("initialize Drive upload HTTP client failed: {error}"))?;
        Ok(Self { client, http })
    }
}

#[async_trait]
impl DrivePageContentPort for HttpDrivePageContentPort {
    async fn create_page_content(
        &self,
        command: CreateDrivePageContentCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError> {
        let body = serialize_page_content(&command.content)?;
        let (node_id, space_id, version) = self
            .create_node_and_upload(
                &command.tenant_id,
                &command.organization_id,
                &command.operator_id,
                &command.page_id,
                &command.title,
                &command.drive_space_id,
                command.folder_drive_node_id.as_deref(),
                &command.content_type,
                &body,
            )
            .await?;
        Ok(build_snapshot(PageSnapshotInput {
            drive_space_id: &space_id,
            drive_node_id: &node_id,
            drive_version_id: &version.id,
            drive_version_no: parse_version_no(&version),
            content_type: &command.content_type,
            content_schema_version: &command.content_schema_version,
            content_hash: Some(version.checksum_sha256_hex.clone()),
            content: command.content,
        }))
    }

    async fn update_page_content(
        &self,
        command: UpdateDrivePageContentCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError> {
        if let Some(expected) = command.expected_drive_version_id.as_deref() {
            let current = self
                .latest_file_version(&command.drive_node_id, &command.tenant_id)
                .await?;
            if current.id != expected {
                return Err(CanvasProductError::Conflict(
                    "page Drive version has changed".to_string(),
                ));
            }
        }
        let body = serialize_page_content(&command.content)?;
        let version = self
            .upload_new_version(
                &command.tenant_id,
                &command.operator_id,
                &command.drive_space_id,
                &command.drive_node_id,
                &command.page_id,
                &command.content_type,
                &body,
            )
            .await?;
        Ok(build_snapshot(PageSnapshotInput {
            drive_space_id: &command.drive_space_id,
            drive_node_id: &command.drive_node_id,
            drive_version_id: &version.id,
            drive_version_no: parse_version_no(&version),
            content_type: &command.content_type,
            content_schema_version: &command.content_schema_version,
            content_hash: Some(version.checksum_sha256_hex.clone()),
            content: command.content,
        }))
    }

    async fn read_page_content(
        &self,
        command: ReadDrivePageContentCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError> {
        let version = self
            .resolve_version(
                &command.drive_node_id,
                &command.tenant_id,
                &command.current_drive_version_id,
            )
            .await?;
        let content = self
            .download_json_content(&command.tenant_id, &command.drive_node_id)
            .await?;
        Ok(build_snapshot(PageSnapshotInput {
            drive_space_id: &command.drive_space_id,
            drive_node_id: &command.drive_node_id,
            drive_version_id: &version.id,
            drive_version_no: parse_version_no(&version),
            content_type: "application/json",
            content_schema_version: "canvas/v1",
            content_hash: Some(version.checksum_sha256_hex.clone()),
            content,
        }))
    }

    async fn restore_page_content_version(
        &self,
        command: RestoreDrivePageContentVersionCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError> {
        if let Some(expected) = command.expected_current_drive_version_id.as_deref() {
            let current = self
                .latest_file_version(&command.drive_node_id, &command.tenant_id)
                .await?;
            if current.id != expected {
                return Err(CanvasProductError::Conflict(
                    "page Drive version has changed".to_string(),
                ));
            }
        }
        let before = self
            .latest_file_version(&command.drive_node_id, &command.tenant_id)
            .await?;
        self.client
            .drive()
            .versions_restore(
                &command.drive_node_id,
                &command.drive_version_id,
                &NodeCommandRequest::default(),
            )
            .await
            .map_err(map_drive_error("restore page content version"))?;
        let after = self
            .latest_file_version(&command.drive_node_id, &command.tenant_id)
            .await?;
        if after.version_no <= before.version_no {
            return Err(CanvasProductError::Internal(
                "Drive restore did not advance node version".to_string(),
            ));
        }
        let content = self
            .download_json_content(&command.tenant_id, &command.drive_node_id)
            .await?;
        Ok(build_snapshot(PageSnapshotInput {
            drive_space_id: &command.drive_space_id,
            drive_node_id: &command.drive_node_id,
            drive_version_id: &after.id,
            drive_version_no: parse_version_no(&after),
            content_type: &after.content_type,
            content_schema_version: "canvas/v1",
            content_hash: Some(after.checksum_sha256_hex.clone()),
            content,
        }))
    }

    async fn list_page_content_versions(
        &self,
        command: ListDrivePageContentVersionsCommand,
    ) -> Result<DriveVersionPage, CanvasProductError> {
        let mut items = self
            .list_all_versions(&command.drive_node_id, &command.tenant_id)
            .await?;
        items.sort_by(|left, right| right.version_no.cmp(&left.version_no));
        let start = ((command.page.max(1) - 1) * command.page_size.max(1)) as usize;
        let end = start.saturating_add(command.page_size.max(1) as usize);
        let page_items: Vec<DriveVersionSummary> = items
            .iter()
            .skip(start)
            .take(command.page_size.max(1) as usize)
            .map(map_file_version_summary)
            .collect();
        let has_more = end < items.len();
        Ok(DriveVersionPage {
            items: page_items,
            page_info: PageInfo {
                page: command.page,
                page_size: command.page_size,
                has_more,
                next_cursor: if has_more {
                    Some((command.page + 1).to_string())
                } else {
                    None
                },
            },
        })
    }
}

impl HttpDrivePageContentPort {
    async fn create_node_and_upload(
        &self,
        tenant_id: &str,
        organization_id: &str,
        operator_id: &str,
        page_id: &str,
        title: &str,
        drive_space_id: &str,
        parent_node_id: Option<&str>,
        content_type: &str,
        body: &[u8],
    ) -> Result<(String, String, FileVersion), CanvasProductError> {
        let epoch_ms = current_epoch_ms();
        let upload_item_id = format!("canvas-upload-{page_id}-{epoch_ms}");
        let task_id = format!("canvas-task-{page_id}-{epoch_ms}");
        let checksum = sha256_prefixed(body);
        let chunk_size = body.len().max(8) as i64;
        let prepared = self
            .client
            .drive()
            .uploader_uploads_create(&PrepareUploaderUploadRequest {
                id: upload_item_id.clone(),
                task_id,
                app_resource_type: CANVAS_RESOURCE_TYPE.to_string(),
                app_resource_id: page_id.to_string(),
                upload_profile_code: Some(CANVAS_UPLOAD_PROFILE.to_string()),
                file_fingerprint: checksum.clone(),
                original_file_name: format!("{title}.json"),
                content_type: content_type.to_string(),
                content_length: body.len().to_string(),
                chunk_size_bytes: chunk_size.to_string(),
                space_id: Some(drive_space_id.to_string()),
                parent_node_id: parent_node_id.map(str::to_string),
                retention: None,
                now_epoch_ms: Some(epoch_ms.to_string()),
                scene: Some(CANVAS_CONTENT_SCENE.to_string()),
                source: Some(CANVAS_CONTENT_SOURCE.to_string()),
                share_token: None,
            })
            .await
            .map_err(map_drive_error("prepare Drive uploader upload"))?;
        let upload_session_id = prepared.upload_session.id.clone();
        let node_id = prepared.upload_item.node_id.clone();
        let space_id = prepared.upload_item.space_id.clone();
        let _ = organization_id;
        self.upload_single_part(
            tenant_id,
            operator_id,
            &upload_session_id,
            Some(&prepared.upload_session.storage_upload_id),
            Some(&prepared.upload_item.id),
            content_type,
            body,
            &checksum,
        )
        .await?;
        let version = self.latest_file_version(&node_id, tenant_id).await?;
        Ok((node_id, space_id, version))
    }

    async fn upload_new_version(
        &self,
        tenant_id: &str,
        operator_id: &str,
        drive_space_id: &str,
        drive_node_id: &str,
        page_id: &str,
        content_type: &str,
        body: &[u8],
    ) -> Result<FileVersion, CanvasProductError> {
        let epoch_ms = current_epoch_ms();
        let session_id = format!("canvas-session-{page_id}-{epoch_ms}");
        let checksum = sha256_prefixed(body);
        let session = self
            .client
            .drive()
            .upload_sessions_create(&CreateUploadSessionRequest {
                session_id,
                space_id: drive_space_id.to_string(),
                node_id: drive_node_id.to_string(),
                // CANVAS_CONTENT_SCENE uploads keep the Drive binding resolution order
                // (bucket -> space -> space-type -> tenant); the contract field is
                // optional and an omitted preference is the previous behaviour.
                storage_provider_id: None,
                bucket: None,
                object_key: None,
                idempotency_key: format!("canvas-content-{page_id}-{epoch_ms}"),
                expires_at_epoch_ms: (epoch_ms + 3_600_000).to_string(),
            })
            .await
            .map_err(map_drive_error("create Drive upload session"))?;
        self.upload_single_part(
            tenant_id,
            operator_id,
            &session.id,
            Some(&session.storage_upload_id),
            None,
            content_type,
            body,
            &checksum,
        )
        .await?;
        self.latest_file_version(drive_node_id, tenant_id).await
    }

    async fn upload_single_part(
        &self,
        _tenant_id: &str,
        _operator_id: &str,
        upload_session_id: &str,
        storage_upload_id: Option<&str>,
        upload_item_id: Option<&str>,
        content_type: &str,
        body: &[u8],
        checksum: &str,
    ) -> Result<(), CanvasProductError> {
        let presigned = self
            .client
            .drive()
            .upload_sessions_parts_update(
                upload_session_id,
                1,
                &PresignUploadPartRequest {
                    upload_id: storage_upload_id.map(str::to_string),
                    requested_ttl_seconds: Some(900),
                },
            )
            .await
            .map_err(map_drive_error("presign Drive upload part"))?;
        let mut request = self
            .http
            .request(
                reqwest::Method::from_bytes(presigned.method.as_bytes())
                    .unwrap_or(reqwest::Method::PUT),
                &presigned.upload_url,
            )
            .body(body.to_vec());
        for (key, value) in presigned.headers {
            request = request.header(key, value);
        }
        let response = request.send().await.map_err(|error| {
            CanvasProductError::Internal(format!("upload Drive object bytes failed: {error}"))
        })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(CanvasProductError::Internal(format!(
                "upload Drive object bytes failed with status {status}: {body}"
            )));
        }
        let etag = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim_matches('"').to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "\"uploaded\"".to_string());
        if let Some(upload_item_id) = upload_item_id {
            self.client
                .drive()
                .uploader_uploads_parts_update(
                    upload_item_id,
                    1,
                    &MarkUploaderPartUploadedRequest {
                        upload_session_id: upload_session_id.to_string(),
                        offset_bytes: "0".to_string(),
                        size_bytes: body.len().to_string(),
                        etag: etag.clone(),
                        checksum_sha256_hex: Some(
                            checksum
                                .strip_prefix("sha256:")
                                .unwrap_or(checksum)
                                .to_string(),
                        ),
                        uploaded_at_epoch_ms: Some(current_epoch_ms().to_string()),
                    },
                )
                .await
                .map_err(map_drive_error("mark Drive upload part uploaded"))?;
        }
        self.client
            .drive()
            .upload_sessions_complete(
                upload_session_id,
                &CompleteUploadSessionRequest {
                    upload_id: storage_upload_id.map(str::to_string),
                    content_type: content_type.to_string(),
                    content_length: body.len().to_string(),
                    checksum_sha256_hex: checksum.to_string(),
                    parts: vec![CompletedUploadPart { part_no: 1, etag }],
                },
            )
            .await
            .map_err(map_drive_error("complete Drive upload session"))?;
        Ok(())
    }

    async fn latest_file_version(
        &self,
        node_id: &str,
        _tenant_id: &str,
    ) -> Result<FileVersion, CanvasProductError> {
        let versions = self.list_all_versions(node_id, _tenant_id).await?;
        versions
            .into_iter()
            .max_by_key(parse_version_no)
            .ok_or_else(|| CanvasProductError::NotFound("Drive file version not found".to_string()))
    }

    async fn resolve_version(
        &self,
        node_id: &str,
        _tenant_id: &str,
        drive_version_id: &str,
    ) -> Result<FileVersion, CanvasProductError> {
        let response = self
            .client
            .drive()
            .versions_list(node_id, Some("100"), None)
            .await
            .map_err(map_drive_error("list Drive file versions"))?;
        response
            .items
            .into_iter()
            .find(|version| version.id == drive_version_id)
            .ok_or_else(|| CanvasProductError::NotFound("Drive file version not found".to_string()))
    }

    async fn list_all_versions(
        &self,
        node_id: &str,
        _tenant_id: &str,
    ) -> Result<Vec<FileVersion>, CanvasProductError> {
        let mut items = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let response = self
                .client
                .drive()
                .versions_list(node_id, Some("100"), page_token.as_deref())
                .await
                .map_err(map_drive_error("list Drive file versions"))?;
            items.extend(response.items);
            page_token = response
                .page_info
                .next_cursor
                .filter(|token| !is_blank(Some(token.as_str())));
            if page_token.is_none() {
                break;
            }
        }
        Ok(items)
    }

    async fn download_json_content(
        &self,
        _tenant_id: &str,
        node_id: &str,
    ) -> Result<serde_json::Value, CanvasProductError> {
        let download = self
            .client
            .drive()
            .download_urls_create(&CreateDownloadUrlRequest {
                node_id: node_id.to_string(),
                requested_ttl_seconds: Some(900),
            })
            .await
            .map_err(map_drive_error("create Drive download url"))?;
        let response = self
            .http
            .get(&download.download_url)
            .send()
            .await
            .map_err(|error| {
                CanvasProductError::Internal(format!("download Drive content failed: {error}"))
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(CanvasProductError::Internal(format!(
                "download Drive content failed with status {status}: {body}"
            )));
        }
        let bytes = response.bytes().await.map_err(|error| {
            CanvasProductError::Internal(format!("read Drive content bytes failed: {error}"))
        })?;
        serde_json::from_slice(&bytes).map_err(|error| {
            CanvasProductError::Internal(format!("parse Drive page content json failed: {error}"))
        })
    }
}

fn serialize_page_content(content: &serde_json::Value) -> Result<Vec<u8>, CanvasProductError> {
    serde_json::to_vec(content).map_err(|error| {
        CanvasProductError::Internal(format!("serialize page content failed: {error}"))
    })
}

fn parse_version_no(version: &FileVersion) -> i64 {
    version.version_no.parse::<i64>().unwrap_or(0)
}

fn map_file_version_summary(version: &FileVersion) -> DriveVersionSummary {
    build_version_summary(&version.id, parse_version_no(version), &version.created_at)
}

fn sha256_prefixed(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    format!("sha256:{digest:x}")
}

fn current_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn map_drive_error(context: &'static str) -> impl Fn(SdkworkError) -> CanvasProductError {
    move |error| CanvasProductError::Internal(format!("{context}: {error}"))
}
