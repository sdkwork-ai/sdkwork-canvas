use sqlx::any::AnyPoolOptions;
use sdkwork_database_config::DatabaseConfig;

use sdkwork_canvas_pages_repository_sqlx::install_sqlite_schema;
use sdkwork_canvas_pages_repository_sqlx::canvas_store::SqlCanvasStore;
use sdkwork_canvas_pages_service::service::CanvasPagesService;

use super::drive_port::CanvasApiDrivePort;

pub async fn assemble_canvas_service_from_env(
) -> Result<CanvasPagesService<SqlCanvasStore, CanvasApiDrivePort>, String> {
    sqlx::any::install_default_drivers();
    let config = DatabaseConfig::from_env("canvas")
        .map_err(|error| format!("resolve Canvas Database config failed: {error}"))?;
    let pool = AnyPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout())
        .idle_timeout(config.idle_timeout())
        .max_lifetime(config.max_lifetime())
        .connect(&config.url)
        .await
        .map_err(|error| format!("connect Canvas Database failed: {error}"))?;
    install_sqlite_schema(&pool)
        .await
        .map_err(|error| format!("install canvas schema failed: {error}"))?;

    Ok(CanvasPagesService::new(
        SqlCanvasStore::new(pool),
        CanvasApiDrivePort::from_env(),
    ))
}
