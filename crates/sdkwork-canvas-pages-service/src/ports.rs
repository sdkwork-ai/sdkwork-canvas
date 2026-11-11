use crate::domain::{
    AiFeedback, AiJob, AiJobSource, AiSuggestion, CanvasActorContext, DrivePageContentSnapshot,
    DriveVersionPage, NewAiFeedback, NewAiJob, NewAiSuggestion, NewPage, NewWorkspace, Page,
    PageMetadataPatch, Workspace,
};
use crate::error::CanvasProductError;
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait CanvasRepository: Clone + Send + Sync + 'static {
    async fn insert_workspace(
        &self,
        workspace: NewWorkspace,
    ) -> Result<Workspace, CanvasProductError>;

    async fn find_workspace(
        &self,
        context: &CanvasActorContext,
        workspace_id: &str,
    ) -> Result<Workspace, CanvasProductError>;

    async fn list_workspaces(
        &self,
        context: &CanvasActorContext,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<Workspace>, CanvasProductError>;

    async fn insert_page(&self, page: NewPage) -> Result<Page, CanvasProductError>;

    async fn page_id_is_reserved(&self, page_id: &str) -> Result<bool, CanvasProductError>;

    async fn find_page(
        &self,
        context: &CanvasActorContext,
        page_id: &str,
    ) -> Result<Page, CanvasProductError>;

    async fn list_pages(
        &self,
        context: &CanvasActorContext,
        workspace_id: &str,
        page: i64,
        page_size: i64,
        q: Option<&str>,
    ) -> Result<Vec<Page>, CanvasProductError>;

    async fn list_root_pages(
        &self,
        context: &CanvasActorContext,
        workspace_id: &str,
        limit: i64,
    ) -> Result<Vec<Page>, CanvasProductError>;

    async fn search_pages(
        &self,
        context: &CanvasActorContext,
        workspace_id: Option<&str>,
        q: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<Page>, CanvasProductError>;

    async fn update_page_metadata(
        &self,
        context: &CanvasActorContext,
        page_id: &str,
        patch: &PageMetadataPatch,
        expected_version: Option<i64>,
    ) -> Result<Page, CanvasProductError>;

    async fn delete_page(
        &self,
        context: &CanvasActorContext,
        page_id: &str,
    ) -> Result<(), CanvasProductError>;

    async fn update_page_drive_snapshot(
        &self,
        context: &CanvasActorContext,
        page_id: &str,
        snapshot: &DrivePageContentSnapshot,
        expected_current_drive_version_id: &str,
    ) -> Result<Page, CanvasProductError>;

    async fn find_ai_job_by_idempotency_key(
        &self,
        context: &CanvasActorContext,
        idempotency_key: &str,
    ) -> Result<Option<AiJob>, CanvasProductError>;

    async fn insert_ai_job(&self, job: NewAiJob) -> Result<AiJob, CanvasProductError>;

    async fn list_ai_jobs(
        &self,
        context: &CanvasActorContext,
        workspace_id: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<AiJob>, CanvasProductError>;

    async fn find_ai_job(
        &self,
        context: &CanvasActorContext,
        ai_job_id: &str,
    ) -> Result<AiJob, CanvasProductError>;

    async fn cancel_ai_job(
        &self,
        context: &CanvasActorContext,
        ai_job_id: &str,
    ) -> Result<AiJob, CanvasProductError>;

    async fn claim_ai_job(
        &self,
        context: &CanvasActorContext,
        ai_job_id: &str,
    ) -> Result<AiJob, CanvasProductError>;

    async fn list_ai_job_sources(
        &self,
        context: &CanvasActorContext,
        ai_job_id: &str,
    ) -> Result<Vec<AiJobSource>, CanvasProductError>;

    async fn fail_ai_job(
        &self,
        context: &CanvasActorContext,
        ai_job_id: &str,
        error_code: &str,
        error_message: &str,
    ) -> Result<AiJob, CanvasProductError>;

    async fn complete_ai_job(
        &self,
        context: &CanvasActorContext,
        ai_job_id: &str,
        suggestions: Vec<NewAiSuggestion>,
    ) -> Result<AiJob, CanvasProductError>;

    async fn list_page_ai_suggestions(
        &self,
        context: &CanvasActorContext,
        page_id: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<AiSuggestion>, CanvasProductError>;

    async fn find_ai_suggestion(
        &self,
        context: &CanvasActorContext,
        ai_suggestion_id: &str,
    ) -> Result<AiSuggestion, CanvasProductError>;

    async fn decide_ai_suggestion(
        &self,
        context: &CanvasActorContext,
        ai_suggestion_id: &str,
        status: &str,
    ) -> Result<AiSuggestion, CanvasProductError>;

    async fn apply_ai_suggestion(
        &self,
        context: &CanvasActorContext,
        ai_suggestion_id: &str,
        page_id: &str,
        snapshot: &DrivePageContentSnapshot,
        expected_current_drive_version_id: &str,
    ) -> Result<AiSuggestion, CanvasProductError>;

    async fn find_ai_feedback(
        &self,
        context: &CanvasActorContext,
        ai_feedback_id: &str,
    ) -> Result<AiFeedback, CanvasProductError>;

    async fn insert_ai_feedback(
        &self,
        feedback: NewAiFeedback,
    ) -> Result<AiFeedback, CanvasProductError>;

    async fn list_ai_suggestion_feedback(
        &self,
        context: &CanvasActorContext,
        ai_suggestion_id: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<AiFeedback>, CanvasProductError>;
}

#[async_trait]
pub trait DrivePageContentPort: Clone + Send + Sync + 'static {
    async fn create_page_content(
        &self,
        command: CreateDrivePageContentCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError>;

    async fn update_page_content(
        &self,
        command: UpdateDrivePageContentCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError>;

    async fn read_page_content(
        &self,
        command: ReadDrivePageContentCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError>;

    async fn restore_page_content_version(
        &self,
        command: RestoreDrivePageContentVersionCommand,
    ) -> Result<DrivePageContentSnapshot, CanvasProductError>;

    async fn list_page_content_versions(
        &self,
        command: ListDrivePageContentVersionsCommand,
    ) -> Result<DriveVersionPage, CanvasProductError>;
}

#[derive(Debug, Clone)]
pub struct CreateDrivePageContentCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub operator_id: String,
    pub workspace_id: String,
    pub page_id: String,
    pub title: String,
    pub drive_space_id: String,
    pub parent_page_id: Option<String>,
    pub folder_drive_node_id: Option<String>,
    pub content: Value,
    pub content_type: String,
    pub content_schema_version: String,
    pub change_summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateDrivePageContentCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub operator_id: String,
    pub workspace_id: String,
    pub page_id: String,
    pub drive_space_id: String,
    pub drive_node_id: String,
    pub drive_uri: String,
    pub current_drive_version_id: String,
    pub content: Value,
    pub content_type: String,
    pub content_schema_version: String,
    pub change_summary: Option<String>,
    pub expected_drive_version_id: Option<String>,
    pub create_checkpoint: bool,
}

#[derive(Debug, Clone)]
pub struct ReadDrivePageContentCommand {
    pub tenant_id: String,
    pub organization_id: String,
    /// The Canvas operator the page belongs to. Drive's version read routes
    /// authorized that subject (`reader`); the in-process port reproduces the
    /// same check instead of trusting the caller.
    pub operator_id: String,
    pub page_id: String,
    pub drive_space_id: String,
    pub drive_node_id: String,
    pub drive_uri: String,
    pub current_drive_version_id: String,
}

#[derive(Debug, Clone)]
pub struct RestoreDrivePageContentVersionCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub operator_id: String,
    pub page_id: String,
    pub drive_space_id: String,
    pub drive_node_id: String,
    pub drive_uri: String,
    pub current_drive_version_id: String,
    pub drive_version_id: String,
    pub expected_current_drive_version_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListDrivePageContentVersionsCommand {
    pub tenant_id: String,
    pub organization_id: String,
    /// The Canvas operator the page belongs to. Drive's version list route
    /// authorized that subject (`reader`).
    pub operator_id: String,
    pub page_id: String,
    pub drive_space_id: String,
    pub drive_node_id: String,
    pub drive_uri: String,
    pub current_drive_version_id: String,
    pub page: i64,
    pub page_size: i64,
}
