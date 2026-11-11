//! Canvas product service construction (API_ASSEMBLY_SPEC §3/§6.1).
//! The assembly owns the canvas pages service bootstrap — repository, drive
//! port, and database lifecycle — so consuming gateways never import
//! `sdkwork-canvas-pages-service` or `sdkwork-canvas-pages-repository-sqlx`
//! directly.

pub mod database;
pub mod drive_http_adapter;
pub mod drive_in_process;
pub mod drive_page_content_mapping;
pub mod drive_port;
