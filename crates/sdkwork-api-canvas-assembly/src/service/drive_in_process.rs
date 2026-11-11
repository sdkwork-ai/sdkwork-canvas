//! Canvas' in-process Drive page-content port (MODULE_SPEC.md §4.2).
//!
//! Canvas stores durable page bytes in sdkwork-drive and keeps the page pointer
//! plus its own metadata. Every SDKWork packaged deployment runs one backend
//! process against one PostgreSQL, so Canvas reaches Drive **in-process**: it
//! depends on `sdkwork-drive-workspace-service` and calls Drive's own
//! store/service APIs on the process pool the host gateway already owns. There
//! is no sibling endpoint variable, no service credential, and no extra hop.
//!
//! # What each replaced HTTP call did, and what replaces it
//!
//! | Canvas operation | Drive app-api route the old adapter called | In-process replacement |
//! | --- | --- | --- |
//! | create page content | `POST /uploader/uploads` + part upload + `GET /nodes/{id}/versions` | `DriveUploaderService::upload_bytes` (prepare + object write + complete) + `DriveNodeVersionStore::list_by_node` |
//! | update page content | `POST /upload-sessions` + part upload + complete + `GET /nodes/{id}/versions` | `DriveStorageKeyService::build_object_key` + `DriveObjectStore::put_object` + `DriveUploadService::complete_stored_version` |
//! | read page content | `GET /nodes/{id}/versions` + `POST /download-urls` + signed GET | `DriveNodeVersionStore::find_by_id` + `DriveStorageObjectStore::find_latest_active_by_node` + `DriveObjectStoreRuntime::resolve_locator` + `DriveObjectStore::read_object_range` |
//! | restore page version | `POST /nodes/{id}/versions/{versionId}/restore` | `SqlDriveNodeVersionStore::restore_version` |
//! | list versions | `GET /nodes/{id}/versions` | `DriveNodeVersionStore::list_by_node` |
//!
//! The uploader route enforced, in order: an active node/space in the caller's
//! tenant, `writer` on the target parent (or the space permission anchor),
//! `ensure_tenant_can_allocate_bytes`, then Drive's own uploader prepare logic
//! (idempotent by `task_id`, unique node name, default storage binding). This
//! port replays that sequence with Drive's own services:
//!
//! | Need | Drive API |
//! | --- | --- |
//! | node by tenant + id, active | `DriveUploaderStore::find_active_node` (`SqlUploaderStore`) |
//! | its space by tenant + id, active | `DriveUploaderStore::find_active_space` (`SqlUploaderStore`) |
//! | effective node role | `SqlDrivePermissionService::resolve_effective_node_access` + `allows_role` |
//! | space owner shortcut | `SqlDrivePermissionService::is_space_owner` |
//! | space permission anchor | `SqlDrivePermissionService::resolve_space_permission_anchor_node` |
//! | tenant byte budget | `ensure_tenant_can_allocate_bytes` + `SqlQuotaStore` |
//! | node versions | `SqlDriveNodeVersionStore` (`DriveNodeVersionStore`) |
//! | object locator + bytes | `SqlStorageObjectStore`, `DriveObjectStoreRuntime`, `DriveObjectStore` |
//!
//! # Scoping and authorization
//!
//! The tenant and organization always come from the Canvas request context
//! (`CanvasActorContext` -> command), never from an environment default. The
//! subject is the Canvas operator the page belongs to: reads require `reader`,
//! writes require `writer`, exactly the roles the Drive routes enforced. Canvas
//! page rows are already tenant- and actor-scoped, so the platform user is the
//! identity the page operation belongs to — the same choice sdkwork-mail made
//! for the message owner.
//!
//! # Error mapping (unchanged HTTP status codes)
//!
//! The replaced HTTP adapter mapped every generated-SDK failure (Drive `4xx`
//! and `5xx` alike) to `CanvasProductError::Internal`, i.e. Canvas answered
//! `500`. This port keeps that mapping: any Drive-side failure is `Internal`,
//! and the two conditions the adapter produced itself are preserved verbatim —
//! a requested version id that is absent from the node's version list is
//! `NotFound` ("Drive file version not found"), and a stale
//! `expected_drive_version_id` is `Conflict` (raised by the caller before the
//! port is reached).
//!
//! # Writing page content
//!
//! A page save is an append: the node already exists, so the bytes are staged at
//! a Drive storage locator and Drive publishes them as the next version. Canvas
//! resolves the locator from Drive's own key service
//! (`DriveStorageKeyService::build_object_key`) over the space's default provider
//! binding, writes the object through Drive's object-store runtime, and then
//! hands the locator to `DriveUploadService::complete_stored_version`, which
//! records the storage object, the node version, the node head snapshot and the
//! `node.version_committed` event in one transaction. Canvas never writes
//! `dr_drive_*` SQL itself (MODULE_SPEC.md §4.2).
//!
//! Authorization is reproduced exactly as on the Drive routes before Drive is
//! asked to mutate: `writer` on the node (or the space-owner shortcut), and the
//! stale-`expected_drive_version_id` check answers `Conflict` before any byte is
//! written.
//!
//! A page restore is Drive's own `SqlDriveNodeVersionStore::restore_version`:
//! Drive flips the selected version and its storage object back to `active` and
//! records a `drive.file_version.restored` event. That call is a lifecycle flip,
//! not a new version, exactly like the Drive app-api route it replaces.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use sdkwork_canvas_pages_service::domain::{DrivePageContentSnapshot, DriveVersionPage, PageInfo};
use sdkwork_canvas_pages_service::error::CanvasProductError;
use sdkwork_canvas_pages_service::ports::{
    CreateDrivePageContentCommand, DrivePageContentPort, ListDrivePageContentVersionsCommand,
    ReadDrivePageContentCommand, RestoreDrivePageContentVersionCommand,
    UpdateDrivePageContentCommand,
};
use sdkwork_drive_object_runtime::{DriveObjectStoreRuntime, ProviderAccessIntent};
use sdkwork_drive_storage_contract::{
    DriveByteRange, DriveObjectLocator, DriveObjectStoreError, PutObjectRequest,
    ReadObjectRangeRequest,
};
use sdkwork_drive_workspace_service::application::permission_service::SqlDrivePermissionService;
use sdkwork_drive_workspace_service::application::quota_enforcement::ensure_tenant_can_allocate_bytes;
use sdkwork_drive_workspace_service::application::storage_key_service::{
    BuildStorageObjectKeyCommand, DriveStorageKeyService,
};
use sdkwork_drive_workspace_service::application::upload_service::{
    CompleteStoredVersionUploadCommand, DriveUploadService,
};
use sdkwork_drive_workspace_service::domain::node_version::DriveNodeVersion;
use sdkwork_drive_workspace_service::infrastructure::sql::node_version_store::SqlDriveNodeVersionStore;
use sdkwork_drive_workspace_service::infrastructure::sql::quota_store::SqlQuotaStore;
use sdkwork_drive_workspace_service::infrastructure::sql::storage_object_store::{
    next_storage_object_version_no, SqlStorageObjectStore,
};
use sdkwork_drive_workspace_service::infrastructure::sql::upload_session_store::SqlUploadSessionStore;
use sdkwork_drive_workspace_service::infrastructure::sql::uploader_store::SqlUploaderStore;
use sdkwork_drive_workspace_service::ports::node_version_store::DriveNodeVersionStore;
use sdkwork_drive_workspace_service::ports::permission_store::ResolveEffectiveNodeAccessCommand;
use sdkwork_drive_workspace_service::ports::storage_object_store::DriveStorageObjectStore;
use sdkwork_drive_workspace_service::ports::uploader_store::{
    DriveUploaderNodeRecord, DriveUploaderStore,
};
use sdkwork_drive_workspace_service::uploader::{
    DriveUploaderService, PrepareUploaderUploadCommand, UploadBytesCommand, UploaderActor,
    UploaderRetention, UploaderTarget,
};
use sdkwork_drive_workspace_service::DriveServiceError;

use crate::service::drive_page_content_mapping::{
    build_snapshot, build_version_summary, PageSnapshotInput,
};

/// The Drive subject type a signed-in Canvas user is authorized as
/// (`sdkwork-routes-drive-app-api` `validate_subject_type`).
const DRIVE_USER_SUBJECT_TYPE: &str = "user";

/// The role Drive's node version read routes require (`reader`).
const DRIVE_READER_ROLE: &str = "reader";

/// The role Drive's uploader/version mutation routes require (`writer`).
const DRIVE_WRITER_ROLE: &str = "writer";

/// Canvas' stable application identity for uploads it generates itself. The
/// Drive route derived this from the caller's token; an in-process caller is
/// the Canvas module, and the resource binding still names the page id.
const CANVAS_APP_ID: &str = "sdkwork-canvas";
const CANVAS_RESOURCE_TYPE: &str = "canvas_page";
const CANVAS_UPLOAD_PROFILE: &str = "generic";
const CANVAS_CONTENT_SCENE: &str = "canvas_page_content";
const CANVAS_CONTENT_SOURCE: &str = "canvas_product_service";

/// Content type / schema the replaced adapter reported when reading a stored
/// page body (it never echoed the Drive row's own metadata).
const READ_CONTENT_TYPE: &str = "application/json";
const READ_CONTENT_SCHEMA_VERSION: &str = "canvas/v1";

/// Page size the old adapter used while walking the Drive version listing.
const VERSION_PAGE_LIMIT: i64 = 100;

/// Upper bound on version-listing pages, so a corrupt or runaway version table
/// cannot wedge a request in an unbounded loop.
const MAX_VERSION_PAGES: i64 = 1_000;

/// Drive stores and validates checksums in the `sha256:<64 lowercase hex>`
/// shape; object-store writes carry the bare digest.
const CHECKSUM_PREFIX: &str = "sha256:";

/// How many digest characters the idempotency key and object id embed. Long
/// enough to keep two page bodies apart, short enough that the key stays a
/// Drive-sized operation id.
const PAGE_CONTENT_DIGEST_CHARS: usize = 16;

/// Inputs the port resolves before Drive publishes the next page-content
/// version. Kept as one value object so the completion command is built in one
/// place and can be asserted without a database.
struct PageContentCompletion<'a> {
    tenant_id: &'a str,
    space_id: &'a str,
    node_id: &'a str,
    page_id: &'a str,
    operator_id: &'a str,
    content_type: &'a str,
    storage_provider_id: &'a str,
    bucket: &'a str,
    object_key: &'a str,
    content_length: i64,
    checksum_sha256_hex: String,
}

/// Build the command Canvas hands to `DriveUploadService::complete_stored_version`.
///
/// The idempotency key is derived from the page and the body digest, so a retry
/// of the same save names the same Drive operation while a different body never
/// does.
fn page_content_completion_command(
    input: PageContentCompletion<'_>,
) -> CompleteStoredVersionUploadCommand {
    CompleteStoredVersionUploadCommand {
        tenant_id: input.tenant_id.to_string(),
        space_id: input.space_id.to_string(),
        node_id: input.node_id.to_string(),
        storage_provider_id: input.storage_provider_id.to_string(),
        bucket: input.bucket.to_string(),
        object_key: input.object_key.to_string(),
        content_type: input.content_type.to_string(),
        content_length: input.content_length,
        checksum_sha256_hex: input.checksum_sha256_hex.clone(),
        idempotency_key: page_content_idempotency_key(input.page_id, &input.checksum_sha256_hex),
        operator_id: input.operator_id.to_string(),
    }
}

/// Stable `(page, body)` operation id for one page-content version.
fn page_content_idempotency_key(page_id: &str, checksum_sha256_hex: &str) -> String {
    let digest = checksum_sha256_hex
        .strip_prefix(CHECKSUM_PREFIX)
        .unwrap_or(checksum_sha256_hex);
    let short: String = digest.chars().take(PAGE_CONTENT_DIGEST_CHARS).collect();
    format!("{CANVAS_APP_ID}-page-content-{page_id}-{short}")
}

/// Object key segment that identifies one page body inside the node's version
/// path.
fn page_content_object_id(page_id: &str, checksum_sha256_hex: &str) -> String {
    let digest = checksum_sha256_hex
        .strip_prefix(CHECKSUM_PREFIX)
        .unwrap_or(checksum_sha256_hex);
    let short: String = digest.chars().take(PAGE_CONTENT_DIGEST_CHARS).collect();
    format!("{CANVAS_APP_ID}-page-{page_id}-{short}")
}

/// Drive page-content port backed by Drive's own services on the host pool.
#[derive(Clone)]
pub struct InProcessDrivePageContentPort {
    pool: PgPool,
}

impl InProcessDrivePageContentPort {
    /// Build the port over the process database pool the host gateway owns.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Load an active node inside the request tenant.
    async fn load_active_node(
        &self,
        tenant_id: &str,
        node_id: &str,
        context: &'static str,
    ) -> Result<DriveUploaderNodeRecord, CanvasProductError> {
        SqlUploaderStore::new(self.pool.clone())
            .find_active_node(tenant_id, node_id)
            .await
            .map_err(|error| drive_error(context, error))?
            .ok_or_else(|| {
                CanvasProductError::Internal(format!(
                    "{context}: Drive node {node_id} is not active in this tenant"
                ))
            })
    }

    /// Require the node's space to be active in the same tenant, as the Drive
    /// route did before authorizing on the node.
    async fn ensure_space_active(
        &self,
        tenant_id: &str,
        node: &DriveUploaderNodeRecord,
        context: &'static str,
    ) -> Result<(), CanvasProductError> {
        let space = SqlUploaderStore::new(self.pool.clone())
            .find_active_space(tenant_id, &node.space_id)
            .await
            .map_err(|error| drive_error(context, error))?;
        if space.is_none() {
            return Err(CanvasProductError::Internal(format!(
                "{context}: Drive space {} is not active in this tenant",
                node.space_id
            )));
        }
        Ok(())
    }

    /// Reproduce Drive's `ensure_ctx_node_role(...)`: resolve the effective role
    /// from Drive's own permission service and test it against the required one.
    async fn ensure_node_role(
        &self,
        tenant_id: &str,
        space_id: &str,
        node_id: &str,
        subject_id: &str,
        required_role: &str,
        context: &'static str,
    ) -> Result<(), CanvasProductError> {
        let service = SqlDrivePermissionService::new(self.pool.clone());
        let access = service
            .resolve_effective_node_access(ResolveEffectiveNodeAccessCommand {
                tenant_id: tenant_id.to_string(),
                space_id: space_id.to_string(),
                node_id: node_id.to_string(),
                subject_type: DRIVE_USER_SUBJECT_TYPE.to_string(),
                subject_id: subject_id.to_string(),
            })
            .await
            .map_err(|error| drive_error(context, error))?;
        if !access.allows_role(required_role) {
            return Err(CanvasProductError::Internal(format!(
                "{context}: Drive node {node_id} does not grant {required_role} to the Canvas operator"
            )));
        }
        Ok(())
    }

    /// Reproduce Drive's `acl::ensure_parent_writer` for the uploader route:
    /// space owners pass, everyone else needs `writer` on the target parent, or
    /// on the space permission anchor when the upload targets the space root.
    async fn ensure_parent_writer(
        &self,
        tenant_id: &str,
        space_id: &str,
        parent_node_id: Option<&str>,
        subject_id: &str,
        context: &'static str,
    ) -> Result<(), CanvasProductError> {
        let service = SqlDrivePermissionService::new(self.pool.clone());
        if service
            .is_space_owner(tenant_id, space_id, DRIVE_USER_SUBJECT_TYPE, subject_id)
            .await
            .map_err(|error| drive_error(context, error))?
        {
            return Ok(());
        }
        let anchor_node_id = match parent_node_id {
            Some(parent_node_id) => parent_node_id.to_string(),
            None => service
                .resolve_space_permission_anchor_node(tenant_id, space_id)
                .await
                .map_err(|error| drive_error(context, error))?,
        };
        self.ensure_node_role(
            tenant_id,
            space_id,
            &anchor_node_id,
            subject_id,
            DRIVE_WRITER_ROLE,
            context,
        )
        .await
    }

    /// All versions of a node, newest first, bounded like the old adapter's
    /// paginated listing.
    async fn list_all_versions(
        &self,
        tenant_id: &str,
        node_id: &str,
        context: &'static str,
    ) -> Result<Vec<DriveNodeVersion>, CanvasProductError> {
        let store = SqlDriveNodeVersionStore::new(self.pool.clone());
        let mut items = Vec::new();
        let mut offset = 0_i64;
        for _ in 0..MAX_VERSION_PAGES {
            let batch = store
                .list_by_node(tenant_id, node_id, VERSION_PAGE_LIMIT, offset)
                .await
                .map_err(|error| drive_error(context, error))?;
            let received = batch.len();
            items.extend(batch);
            if (received as i64) < VERSION_PAGE_LIMIT {
                return Ok(items);
            }
            offset += VERSION_PAGE_LIMIT;
        }
        Err(CanvasProductError::Internal(format!(
            "{context}: Drive node {node_id} has too many versions to list"
        )))
    }

    /// Latest version of a node, reported the way the old adapter did.
    async fn latest_version(
        &self,
        tenant_id: &str,
        node_id: &str,
        context: &'static str,
    ) -> Result<DriveNodeVersion, CanvasProductError> {
        self.list_all_versions(tenant_id, node_id, context)
            .await?
            .into_iter()
            .max_by_key(|version| version.version_no)
            .ok_or_else(|| CanvasProductError::NotFound("Drive file version not found".to_string()))
    }

    /// One specific version, reported the way the old adapter did.
    async fn find_version(
        &self,
        tenant_id: &str,
        node_id: &str,
        version_id: &str,
        context: &'static str,
    ) -> Result<DriveNodeVersion, CanvasProductError> {
        SqlDriveNodeVersionStore::new(self.pool.clone())
            .find_by_id(tenant_id, node_id, version_id)
            .await
            .map_err(|error| drive_error(context, error))?
            .ok_or_else(|| CanvasProductError::NotFound("Drive file version not found".to_string()))
    }

    /// Stage `body` at a Drive storage locator and publish it as the node's next
    /// version through Drive's own upload service.
    ///
    /// The caller has already authorized the operator; this method only resolves
    /// Drive's storage target, writes the bytes and completes the version.
    #[allow(clippy::too_many_arguments)]
    async fn publish_page_content(
        &self,
        tenant_id: &str,
        space_id: &str,
        node_id: &str,
        page_id: &str,
        operator_id: &str,
        content_type: &str,
        body: Vec<u8>,
        context: &'static str,
    ) -> Result<DriveNodeVersion, CanvasProductError> {
        let content_length = i64::try_from(body.len()).map_err(|_| {
            CanvasProductError::Internal(format!("{context}: page content is too large"))
        })?;
        let checksum_sha256_hex = sha256_prefixed(&body);
        // Drive allocates version numbers across the storage-object table; the
        // key embeds the same number so the object sorts with its version.
        let version_no = next_storage_object_version_no(&self.pool, tenant_id, node_id)
            .await
            .map_err(|error| drive_error(context, error))?;
        let object_id = page_content_object_id(page_id, &checksum_sha256_hex);
        let object_key = DriveStorageKeyService::build_object_key(BuildStorageObjectKeyCommand {
            tenant_id,
            space_id,
            node_id,
            version_no,
            object_id: &object_id,
        })
        .map_err(|error| CanvasProductError::Internal(format!("{context}: {error}")))?;
        let (storage_provider_id, bucket) = SqlUploaderStore::new(self.pool.clone())
            .find_default_storage_provider(tenant_id, space_id)
            .await
            .map_err(|error| drive_error(context, error))?
            .ok_or_else(|| {
                CanvasProductError::Internal(format!(
                    "{context}: Drive space {space_id} has no active storage provider binding"
                ))
            })?;
        let object_store = DriveObjectStoreRuntime::new(self.pool.clone())
            .resolve(&storage_provider_id, ProviderAccessIntent::Write)
            .await
            .map_err(|error| object_store_error(context, error))?;
        object_store
            .put_object(PutObjectRequest {
                locator: DriveObjectLocator {
                    bucket: bucket.clone(),
                    object_key: object_key.clone(),
                },
                content_type: Some(content_type.to_string()),
                metadata: BTreeMap::new(),
                body,
                checksum_sha256_hex: Some(
                    checksum_sha256_hex
                        .strip_prefix(CHECKSUM_PREFIX)
                        .unwrap_or(&checksum_sha256_hex)
                        .to_string(),
                ),
            })
            .await
            .map_err(|error| object_store_error(context, error))?;
        DriveUploadService::new(SqlUploadSessionStore::new(self.pool.clone()))
            .complete_stored_version(page_content_completion_command(PageContentCompletion {
                tenant_id,
                space_id,
                node_id,
                page_id,
                operator_id,
                content_type,
                storage_provider_id: &storage_provider_id,
                bucket: &bucket,
                object_key: &object_key,
                content_length,
                checksum_sha256_hex,
            }))
            .await
            .map_err(|error| drive_error(context, error))
    }

    /// Reject a save whose base version is no longer the node's latest, exactly
    /// as the replaced adapter did before it wrote anything.
    async fn ensure_expected_version_is_current(
        &self,
        tenant_id: &str,
        node_id: &str,
        expected_drive_version_id: Option<&str>,
        context: &'static str,
    ) -> Result<(), CanvasProductError> {
        let Some(expected) = expected_drive_version_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        let current = self.latest_version(tenant_id, node_id, context).await?;
        if current.id != expected {
            return Err(CanvasProductError::Conflict(
                "page Drive version has changed".to_string(),
            ));
        }
        Ok(())
    }

    /// Read the committed bytes of a node's active object through the Drive
    /// object-store runtime (the same locator resolution the download route
    /// uses) instead of minting a download grant and calling it over HTTP.
    async fn read_active_object_bytes(
        &self,
        tenant_id: &str,
        node_id: &str,
        context: &'static str,
    ) -> Result<Vec<u8>, CanvasProductError> {
        let object = SqlStorageObjectStore::new(self.pool.clone())
            .find_latest_active_by_node(tenant_id, node_id)
            .await
            .map_err(|error| drive_error(context, error))?
            .ok_or_else(|| {
                CanvasProductError::Internal(format!(
                    "{context}: Drive node {node_id} has no active storage object"
                ))
            })?;
        let expected_length = u64::try_from(object.content_length).map_err(|_| {
            CanvasProductError::Internal(format!(
                "{context}: Drive node {node_id} has an invalid content length"
            ))
        })?;
        let object_store = DriveObjectStoreRuntime::new(self.pool.clone())
            .resolve_locator(&object.storage_provider_id, &object.bucket)
            .await
            .map_err(|error| object_store_error(context, error))?;
        if expected_length == 0 {
            return Ok(Vec::new());
        }
        let (range, mut chunks) = object_store
            .read_object_range(ReadObjectRangeRequest {
                locator: DriveObjectLocator {
                    bucket: object.bucket.clone(),
                    object_key: object.object_key.clone(),
                },
                range: DriveByteRange {
                    start_inclusive: 0,
                    end_inclusive: expected_length - 1,
                },
            })
            .await
            .map_err(|error| object_store_error(context, error))?;
        if range.content_length != expected_length {
            return Err(CanvasProductError::Internal(format!(
                "{context}: Drive object length does not match committed metadata for node {node_id}"
            )));
        }
        // Do not trust persisted metadata enough to pre-allocate the whole
        // object; grow with the stream and verify the total afterwards.
        let mut bytes = Vec::with_capacity((expected_length.min(1 << 20)) as usize);
        while let Some(chunk) = chunks
            .next_chunk()
            .await
            .map_err(|error| object_store_error(context, error))?
        {
            bytes.extend_from_slice(&chunk);
        }
        if bytes.len() as u64 != expected_length {
            return Err(CanvasProductError::Internal(format!(
                "{context}: Drive content stream length does not match committed metadata for node {node_id}"
            )));
        }
        Ok(bytes)
    }

    async fn read_content_json(
        &self,
        tenant_id: &str,
        node_id: &str,
        context: &'static str,
    ) -> Result<Value, CanvasProductError> {
        let bytes = self
            .read_active_object_bytes(tenant_id, node_id, context)
            .await?;
        serde_json::from_slice(&bytes).map_err(|error| {
            CanvasProductError::Internal(format!(
                "{context}: parse page content json failed: {error}"
            ))
        })
    }
}

#[async_trait]
impl DrivePageContentPort for InProcessDrivePageContentPort {
    async fn create_page_content(
        &self,
        command: CreateDrivePageContentCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError> {
        const CONTEXT: &str = "create Drive page content";
        let tenant_id = require_text(&command.tenant_id, "tenant id", CONTEXT)?;
        let operator_id = require_text(&command.operator_id, "operator id", CONTEXT)?;
        let drive_space_id = require_text(&command.drive_space_id, "drive space id", CONTEXT)?;
        let page_id = require_text(&command.page_id, "page id", CONTEXT)?;
        let body = serde_json::to_vec(&command.content).map_err(|error| {
            CanvasProductError::Internal(format!(
                "{CONTEXT}: serialize page content failed: {error}"
            ))
        })?;
        let content_length = i64::try_from(body.len()).map_err(|_| {
            CanvasProductError::Internal(format!("{CONTEXT}: page content is too large"))
        })?;
        // Same order as the uploader route: authorize the target first, then
        // check the tenant byte budget, then let Drive prepare the upload.
        self.ensure_parent_writer(
            &tenant_id,
            &drive_space_id,
            command.folder_drive_node_id.as_deref(),
            &operator_id,
            CONTEXT,
        )
        .await?;
        ensure_tenant_can_allocate_bytes(
            &SqlQuotaStore::new(self.pool.clone()),
            &tenant_id,
            content_length,
        )
        .await
        .map_err(|error| drive_error(CONTEXT, error))?;

        // The object store must be resolved before `upload_bytes` writes the
        // prepared object; Drive's own uploader resolves the same default
        // storage binding for the space, so the locator matches.
        let provider_id = SqlUploaderStore::new(self.pool.clone())
            .find_default_storage_provider(&tenant_id, &drive_space_id)
            .await
            .map_err(|error| drive_error(CONTEXT, error))?
            .map(|(provider_id, _bucket)| provider_id)
            .ok_or_else(|| {
                CanvasProductError::Internal(format!(
                    "{CONTEXT}: Drive space {drive_space_id} has no active storage provider binding"
                ))
            })?;
        let object_store = DriveObjectStoreRuntime::new(self.pool.clone())
            .resolve(&provider_id, ProviderAccessIntent::Write)
            .await
            .map_err(|error| object_store_error(CONTEXT, error))?;

        let epoch_ms = current_epoch_ms();
        let checksum_sha256_hex = sha256_prefixed(&body);
        let uploader = DriveUploaderService::new(SqlUploaderStore::new(self.pool.clone()));
        let upload_item = uploader
            .upload_bytes(
                &*object_store,
                UploadBytesCommand {
                    prepare: PrepareUploaderUploadCommand {
                        id: format!("{CANVAS_APP_ID}-upload-{page_id}-{epoch_ms}"),
                        task_id: format!("{CANVAS_APP_ID}-task-{page_id}-{epoch_ms}"),
                        tenant_id: tenant_id.clone(),
                        organization_id: non_blank(Some(command.organization_id.as_str())),
                        actor: UploaderActor::User {
                            user_id: operator_id.clone(),
                        },
                        app_id: CANVAS_APP_ID.to_string(),
                        app_resource_type: CANVAS_RESOURCE_TYPE.to_string(),
                        app_resource_id: page_id.to_string(),
                        scene: Some(CANVAS_CONTENT_SCENE.to_string()),
                        source: Some(CANVAS_CONTENT_SOURCE.to_string()),
                        upload_profile_code: CANVAS_UPLOAD_PROFILE.to_string(),
                        file_fingerprint: checksum_sha256_hex,
                        original_file_name: format!("{}.json", command.title),
                        content_type: command.content_type.clone(),
                        content_length,
                        chunk_size_bytes: content_length.max(8),
                        target: UploaderTarget::Space {
                            space_id: drive_space_id,
                            parent_node_id: command.folder_drive_node_id.clone(),
                            share_token: None,
                        },
                        retention: UploaderRetention::LongTerm,
                        operator_id,
                        now_epoch_ms: epoch_ms,
                    },
                    body,
                    uploaded_at_epoch_ms: epoch_ms,
                },
            )
            .await
            .map_err(|error| drive_error(CONTEXT, error))?;

        let version = self
            .latest_version(&tenant_id, &upload_item.node_id, CONTEXT)
            .await?;
        Ok(build_snapshot(PageSnapshotInput {
            drive_space_id: &upload_item.space_id,
            drive_node_id: &upload_item.node_id,
            drive_version_id: &version.id,
            drive_version_no: version.version_no,
            content_type: &command.content_type,
            content_schema_version: &command.content_schema_version,
            content_hash: Some(version.checksum_sha256_hex),
            content: command.content,
        }))
    }

    async fn update_page_content(
        &self,
        command: UpdateDrivePageContentCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError> {
        const CONTEXT: &str = "update Drive page content";
        let tenant_id = require_text(&command.tenant_id, "tenant id", CONTEXT)?;
        let operator_id = require_text(&command.operator_id, "operator id", CONTEXT)?;
        let page_id = require_text(&command.page_id, "page id", CONTEXT)?;
        let node_id = require_text(&command.drive_node_id, "drive node id", CONTEXT)?;
        let body = serde_json::to_vec(&command.content).map_err(|error| {
            CanvasProductError::Internal(format!(
                "{CONTEXT}: serialize page content failed: {error}"
            ))
        })?;
        let node = self.load_active_node(&tenant_id, &node_id, CONTEXT).await?;
        self.ensure_space_active(&tenant_id, &node, CONTEXT).await?;
        self.ensure_node_role(
            &tenant_id,
            &node.space_id,
            &node_id,
            &operator_id,
            DRIVE_WRITER_ROLE,
            CONTEXT,
        )
        .await?;
        self.ensure_expected_version_is_current(
            &tenant_id,
            &node_id,
            command.expected_drive_version_id.as_deref(),
            CONTEXT,
        )
        .await?;
        // The node's own space is authoritative for Drive; the snapshot keeps
        // the Canvas-recorded space id so its refs stay byte-identical.
        let version = self
            .publish_page_content(
                &tenant_id,
                &node.space_id,
                &node_id,
                &page_id,
                &operator_id,
                &command.content_type,
                body,
                CONTEXT,
            )
            .await?;
        Ok(build_snapshot(PageSnapshotInput {
            drive_space_id: &command.drive_space_id,
            drive_node_id: &node_id,
            drive_version_id: &version.id,
            drive_version_no: version.version_no,
            content_type: &command.content_type,
            content_schema_version: &command.content_schema_version,
            content_hash: Some(version.checksum_sha256_hex),
            content: command.content,
        }))
    }

    async fn read_page_content(
        &self,
        command: ReadDrivePageContentCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError> {
        const CONTEXT: &str = "read Drive page content";
        let tenant_id = require_text(&command.tenant_id, "tenant id", CONTEXT)?;
        let operator_id = require_text(&command.operator_id, "operator id", CONTEXT)?;
        let node_id = require_text(&command.drive_node_id, "drive node id", CONTEXT)?;
        let version_id = require_text(
            &command.current_drive_version_id,
            "drive version id",
            CONTEXT,
        )?;
        let node = self.load_active_node(&tenant_id, &node_id, CONTEXT).await?;
        self.ensure_space_active(&tenant_id, &node, CONTEXT).await?;
        self.ensure_node_role(
            &tenant_id,
            &node.space_id,
            &node_id,
            &operator_id,
            DRIVE_READER_ROLE,
            CONTEXT,
        )
        .await?;
        let version = self
            .find_version(&tenant_id, &node_id, &version_id, CONTEXT)
            .await?;
        let content = self
            .read_content_json(&tenant_id, &node_id, CONTEXT)
            .await?;
        Ok(build_snapshot(PageSnapshotInput {
            drive_space_id: &command.drive_space_id,
            drive_node_id: &node_id,
            drive_version_id: &version.id,
            drive_version_no: version.version_no,
            content_type: READ_CONTENT_TYPE,
            content_schema_version: READ_CONTENT_SCHEMA_VERSION,
            content_hash: Some(version.checksum_sha256_hex),
            content,
        }))
    }

    async fn restore_page_content_version(
        &self,
        command: RestoreDrivePageContentVersionCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError> {
        const CONTEXT: &str = "restore Drive page content version";
        let tenant_id = require_text(&command.tenant_id, "tenant id", CONTEXT)?;
        let operator_id = require_text(&command.operator_id, "operator id", CONTEXT)?;
        let node_id = require_text(&command.drive_node_id, "drive node id", CONTEXT)?;
        let version_id = require_text(&command.drive_version_id, "drive version id", CONTEXT)?;
        let node = self.load_active_node(&tenant_id, &node_id, CONTEXT).await?;
        self.ensure_space_active(&tenant_id, &node, CONTEXT).await?;
        self.ensure_node_role(
            &tenant_id,
            &node.space_id,
            &node_id,
            &operator_id,
            DRIVE_WRITER_ROLE,
            CONTEXT,
        )
        .await?;
        self.ensure_expected_version_is_current(
            &tenant_id,
            &node_id,
            command.expected_current_drive_version_id.as_deref(),
            CONTEXT,
        )
        .await?;
        // Drive's restore is a lifecycle flip: it re-activates the selected
        // version row and the storage object it points at, records
        // `drive.file_version.restored`, and leaves the node head alone - the
        // same contract as the app-api route this replaces. The snapshot reports
        // the restored version row and the body the node's active object serves
        // (Drive exposes no per-version body read, so this is the same body the
        // replaced adapter downloaded after its restore).
        let restored = SqlDriveNodeVersionStore::new(self.pool.clone())
            .restore_version(&tenant_id, &node_id, &version_id, &operator_id)
            .await
            .map_err(|error| drive_error(CONTEXT, error))?;
        let content = self
            .read_content_json(&tenant_id, &node_id, CONTEXT)
            .await?;
        Ok(build_snapshot(PageSnapshotInput {
            drive_space_id: &command.drive_space_id,
            drive_node_id: &node_id,
            drive_version_id: &restored.id,
            drive_version_no: restored.version_no,
            content_type: &restored.content_type,
            content_schema_version: READ_CONTENT_SCHEMA_VERSION,
            content_hash: Some(restored.checksum_sha256_hex),
            content,
        }))
    }

    async fn list_page_content_versions(
        &self,
        command: ListDrivePageContentVersionsCommand,
    ) -> Result<DriveVersionPage, CanvasProductError> {
        const CONTEXT: &str = "list Drive page versions";
        let tenant_id = require_text(&command.tenant_id, "tenant id", CONTEXT)?;
        let operator_id = require_text(&command.operator_id, "operator id", CONTEXT)?;
        let node_id = require_text(&command.drive_node_id, "drive node id", CONTEXT)?;
        let node = self.load_active_node(&tenant_id, &node_id, CONTEXT).await?;
        self.ensure_space_active(&tenant_id, &node, CONTEXT).await?;
        self.ensure_node_role(
            &tenant_id,
            &node.space_id,
            &node_id,
            &operator_id,
            DRIVE_READER_ROLE,
            CONTEXT,
        )
        .await?;

        let mut versions = self
            .list_all_versions(&tenant_id, &node_id, CONTEXT)
            .await?;
        versions.sort_by(|left, right| right.version_no.cmp(&left.version_no));

        // Pagination is unchanged from the replaced adapter: it listed every
        // version and paginated in memory.
        let page = command.page.max(1);
        let page_size = command.page_size.max(1);
        let start = ((page - 1) * page_size) as usize;
        let items = versions
            .iter()
            .skip(start)
            .take(page_size as usize)
            .map(|version| {
                build_version_summary(&version.id, version.version_no, &version.created_at)
            })
            .collect();
        let has_more = start.saturating_add(page_size as usize) < versions.len();
        Ok(DriveVersionPage {
            items,
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

fn require_text(
    value: &str,
    field: &'static str,
    context: &'static str,
) -> Result<String, CanvasProductError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CanvasProductError::Internal(format!(
            "{context}: {field} is required"
        )));
    }
    Ok(trimmed.to_string())
}

fn non_blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Map Drive's typed errors the way the replaced HTTP adapter did: every
/// Drive-side failure surfaced as `CanvasProductError::Internal`.
fn drive_error(context: &'static str, error: DriveServiceError) -> CanvasProductError {
    CanvasProductError::Internal(format!("{context}: {}", error.message()))
}

fn object_store_error(context: &'static str, error: DriveObjectStoreError) -> CanvasProductError {
    CanvasProductError::Internal(format!("{context}: {error}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn production_source() -> &'static str {
        include_str!("drive_in_process.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the module has a production section")
    }

    #[test]
    fn the_port_calls_drive_services_in_process() {
        // Regression guard for MODULE_SPEC.md §4.2: the default path resolves
        // the node and its space, resolves the operator's effective role with
        // Drive's own permission service, and reads bytes through Drive's
        // object-store runtime.
        let source = production_source();
        for needle in [
            "find_active_node(",
            "find_active_space(",
            "resolve_effective_node_access(",
            "allows_role(",
            "resolve_locator(",
            "read_object_range(",
            "ensure_tenant_can_allocate_bytes(",
            "upload_bytes(",
        ] {
            assert!(source.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn the_roles_match_the_drive_routes() {
        let source = production_source();
        assert!(source.contains("const DRIVE_READER_ROLE: &str = \"reader\";"));
        assert!(source.contains("const DRIVE_WRITER_ROLE: &str = \"writer\";"));
    }

    #[test]
    fn the_port_has_no_http_client_and_no_canvas_owned_sql() {
        let source = production_source();
        assert!(
            !source.contains("reqwest"),
            "the in-process port must not use HTTP"
        );
        assert!(
            !source.contains("sqlx::query"),
            "Canvas must not own Drive SQL (MODULE_SPEC.md §4.2)"
        );
        assert!(
            !source.contains("_URL\""),
            "the in-process port must not read an endpoint variable"
        );
    }

    #[test]
    fn the_tenant_and_subject_come_from_the_request() {
        let source = production_source();
        assert!(
            source.contains("tenant_id: tenant_id.to_string()"),
            "the permission command must carry the request tenant"
        );
        assert!(
            source.contains("subject_id: subject_id.to_string()"),
            "the permission command must carry the Canvas operator"
        );
        assert!(
            !source.contains("SDKWORK_DRIVE_SUBJECT"),
            "the subject must never come from an environment default"
        );
    }

    #[test]
    fn a_page_save_reaches_drive_as_a_complete_stored_version_command() {
        // The replaced stub failed closed. The save path now stages the bytes at
        // a Drive locator and hands Drive's upload service the exact completion
        // command: request tenant, node, the request operator, and a page/body
        // idempotency key.
        let checksum = sha256_prefixed(b"{\"blocks\":[]}");
        let command = page_content_completion_command(PageContentCompletion {
            tenant_id: "tenant-1",
            space_id: "space-1",
            node_id: "node-1",
            page_id: "page-1",
            operator_id: "operator-7",
            content_type: "application/json",
            storage_provider_id: "provider-1",
            bucket: "bucket-1",
            object_key: "sdkwork-drive/v1/objects/content",
            content_length: 13,
            checksum_sha256_hex: checksum.clone(),
        });
        assert_eq!(command.tenant_id, "tenant-1");
        assert_eq!(command.space_id, "space-1");
        assert_eq!(command.node_id, "node-1");
        assert_eq!(command.operator_id, "operator-7");
        assert_eq!(
            command.idempotency_key,
            page_content_idempotency_key("page-1", &checksum)
        );
        assert_eq!(
            command.idempotency_key,
            format!(
                "sdkwork-canvas-page-content-page-1-{}",
                &checksum["sha256:".len()..][..PAGE_CONTENT_DIGEST_CHARS]
            ),
            "the key must name one page body, not the attempt's clock"
        );
        assert_eq!(command.checksum_sha256_hex, checksum);
        assert_eq!(command.content_length, 13);
    }

    #[test]
    fn a_different_page_body_gets_a_different_idempotency_key() {
        assert_ne!(
            page_content_idempotency_key("page-1", &sha256_prefixed(b"{}")),
            page_content_idempotency_key("page-1", &sha256_prefixed(b"{\"a\":1}"))
        );
    }

    #[test]
    fn both_page_content_writes_call_drive_in_process() {
        // Regression guard for the removed fail-closed stubs: neither operation
        // may answer from Canvas, and neither may name the HTTP opt-in as its
        // own remedy.
        let source = production_source();
        for needle in [
            ".complete_stored_version(",
            ".restore_version(",
            "DriveStorageKeyService::build_object_key(",
            ".put_object(",
        ] {
            assert!(source.contains(needle), "missing {needle}");
        }
        assert!(
            !source.contains("SDKWORK_CANVAS_DRIVE_MODE=http"),
            "the in-process default must not send callers to the HTTP opt-in"
        );
    }

    #[test]
    fn checksum_uses_the_drive_wire_shape() {
        assert!(sha256_prefixed(b"abc").starts_with("sha256:"));
        assert_eq!(sha256_prefixed(b"abc").len(), "sha256:".len() + 64);
    }
}
