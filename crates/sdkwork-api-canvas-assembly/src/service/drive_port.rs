use sdkwork_canvas_pages_service::infrastructure::drive::MemoryDrivePageContentPort;
use sdkwork_canvas_pages_service::ports::DrivePageContentPort;
use sdkwork_utils_rust::string::{is_blank, trim};

use super::drive_app_sdk_facade::SdkDriveAppFacadePageContentPort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DrivePortSelectionInput {
    pub use_memory_drive: Option<bool>,
    pub facade_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DrivePortSelection {
    Memory,
    Facade { facade_url: String },
    Unconfigured,
}

pub(crate) fn select_drive_port(input: DrivePortSelectionInput) -> DrivePortSelection {
    if input.use_memory_drive == Some(false) {
        if let Some(facade_url) = input
            .facade_url
            .as_ref()
            .map(|value| trim(value))
            .filter(|value| !is_blank(Some(value.as_str())))
        {
            return DrivePortSelection::Facade { facade_url };
        }
        return DrivePortSelection::Unconfigured;
    }

    DrivePortSelection::Memory
}

#[derive(Clone)]
pub enum CanvasApiDrivePort {
    Memory(MemoryDrivePageContentPort),
    Facade(SdkDriveAppFacadePageContentPort),
    Unconfigured(DevDrivePageContentPort),
}

impl CanvasApiDrivePort {
    pub fn from_env() -> Self {
        let use_memory_drive = std::env::var("SDKWORK_CANVAS_USE_MEMORY_DRIVE")
            .ok()
            .map(|value| parse_truthy(&value));
        let facade_url = std::env::var("SDKWORK_DRIVE_FACADE_URL").ok();
        match select_drive_port(DrivePortSelectionInput {
            use_memory_drive,
            facade_url,
        }) {
            DrivePortSelection::Memory => Self::Memory(MemoryDrivePageContentPort::default()),
            DrivePortSelection::Facade { facade_url } => {
                Self::Facade(
                    SdkDriveAppFacadePageContentPort::from_env(facade_url)
                        .expect("initialize Drive facade adapter failed"),
                )
            }
            DrivePortSelection::Unconfigured => Self::Unconfigured(DevDrivePageContentPort),
        }
    }
}

fn parse_truthy(value: &str) -> bool {
    matches!(
        trim(value).to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[derive(Clone, Default)]
pub struct DevDrivePageContentPort;

#[async_trait::async_trait]
impl DrivePageContentPort for CanvasApiDrivePort {
    async fn create_page_content(
        &self,
        command: sdkwork_canvas_pages_service::ports::CreateDrivePageContentCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DrivePageContentSnapshot,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        match self {
            Self::Memory(port) => port.create_page_content(command).await,
            Self::Facade(port) => port.create_page_content(command).await,
            Self::Unconfigured(port) => port.create_page_content(command).await,
        }
    }

    async fn read_page_content(
        &self,
        command: sdkwork_canvas_pages_service::ports::ReadDrivePageContentCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DrivePageContentSnapshot,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        match self {
            Self::Memory(port) => port.read_page_content(command).await,
            Self::Facade(port) => port.read_page_content(command).await,
            Self::Unconfigured(port) => port.read_page_content(command).await,
        }
    }

    async fn update_page_content(
        &self,
        command: sdkwork_canvas_pages_service::ports::UpdateDrivePageContentCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DrivePageContentSnapshot,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        match self {
            Self::Memory(port) => port.update_page_content(command).await,
            Self::Facade(port) => port.update_page_content(command).await,
            Self::Unconfigured(port) => port.update_page_content(command).await,
        }
    }

    async fn list_page_content_versions(
        &self,
        command: sdkwork_canvas_pages_service::ports::ListDrivePageContentVersionsCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DriveVersionPage,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        match self {
            Self::Memory(port) => port.list_page_content_versions(command).await,
            Self::Facade(port) => port.list_page_content_versions(command).await,
            Self::Unconfigured(port) => port.list_page_content_versions(command).await,
        }
    }

    async fn restore_page_content_version(
        &self,
        command: sdkwork_canvas_pages_service::ports::RestoreDrivePageContentVersionCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DrivePageContentSnapshot,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        match self {
            Self::Memory(port) => port.restore_page_content_version(command).await,
            Self::Facade(port) => port.restore_page_content_version(command).await,
            Self::Unconfigured(port) => port.restore_page_content_version(command).await,
        }
    }
}

#[async_trait::async_trait]
impl DrivePageContentPort for DevDrivePageContentPort {
    async fn create_page_content(
        &self,
        _command: sdkwork_canvas_pages_service::ports::CreateDrivePageContentCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DrivePageContentSnapshot,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        Err(sdkwork_canvas_pages_service::error::CanvasProductError::Internal(
            "Drive integration is not configured; set SDKWORK_DRIVE_FACADE_URL, leave SDKWORK_CANVAS_USE_MEMORY_DRIVE unset for local memory Drive, or set SDKWORK_CANVAS_USE_MEMORY_DRIVE=1"
                .to_string(),
        ))
    }

    async fn read_page_content(
        &self,
        _command: sdkwork_canvas_pages_service::ports::ReadDrivePageContentCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DrivePageContentSnapshot,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        Err(sdkwork_canvas_pages_service::error::CanvasProductError::Internal(
            "Drive integration is not configured".to_string(),
        ))
    }

    async fn update_page_content(
        &self,
        _command: sdkwork_canvas_pages_service::ports::UpdateDrivePageContentCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DrivePageContentSnapshot,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        Err(sdkwork_canvas_pages_service::error::CanvasProductError::Internal(
            "Drive integration is not configured".to_string(),
        ))
    }

    async fn list_page_content_versions(
        &self,
        _command: sdkwork_canvas_pages_service::ports::ListDrivePageContentVersionsCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DriveVersionPage,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        Err(sdkwork_canvas_pages_service::error::CanvasProductError::Internal(
            "Drive integration is not configured".to_string(),
        ))
    }

    async fn restore_page_content_version(
        &self,
        _command: sdkwork_canvas_pages_service::ports::RestoreDrivePageContentVersionCommand,
    ) -> Result<
        sdkwork_canvas_pages_service::domain::DrivePageContentSnapshot,
        sdkwork_canvas_pages_service::error::CanvasProductError,
    > {
        Err(sdkwork_canvas_pages_service::error::CanvasProductError::Internal(
            "Drive integration is not configured".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_memory_drive_for_local_development() {
        assert_eq!(
            select_drive_port(DrivePortSelectionInput {
                use_memory_drive: None,
                facade_url: None,
            }),
            DrivePortSelection::Memory
        );
    }

    #[test]
    fn explicit_memory_flag_still_uses_memory_drive() {
        assert_eq!(
            select_drive_port(DrivePortSelectionInput {
                use_memory_drive: Some(true),
                facade_url: Some("https://drive.example.com".to_string()),
            }),
            DrivePortSelection::Memory
        );
    }

    #[test]
    fn disabled_memory_without_facade_url_is_unconfigured() {
        assert_eq!(
            select_drive_port(DrivePortSelectionInput {
                use_memory_drive: Some(false),
                facade_url: None,
            }),
            DrivePortSelection::Unconfigured
        );
    }

    #[test]
    fn disabled_memory_with_facade_url_selects_drive_sdk_facade() {
        assert_eq!(
            select_drive_port(DrivePortSelectionInput {
                use_memory_drive: Some(false),
                facade_url: Some(" https://drive.example.com ".to_string()),
            }),
            DrivePortSelection::Facade {
                facade_url: "https://drive.example.com".to_string(),
            }
        );
    }
}
