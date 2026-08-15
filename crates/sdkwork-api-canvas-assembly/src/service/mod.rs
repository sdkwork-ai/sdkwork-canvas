//! Canvas product service construction (API_ASSEMBLY_SPEC §3/§6.1).
//! The assembly owns the canvas pages service bootstrap — repository, drive
//! port, and database lifecycle — so consuming gateways never import
//! `sdkwork-canvas-pages-service` or `sdkwork-canvas-pages-repository-sqlx`
//! directly.

pub mod database;
pub mod drive_app_sdk_facade;
pub mod drive_port;
