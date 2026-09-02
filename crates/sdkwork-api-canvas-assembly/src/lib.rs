//! API assembly for sdkwork-canvas.
//! Application bootstrap lives in `bootstrap.rs`; route inventory is in `assembly-manifest.json`.
// SDKWORK-ASSEMBLY-LIB-CUSTOM: exports beyond the canonical materializer template.

mod bootstrap;
mod generated;
pub mod service;

pub use bootstrap::{assemble_api_router, ApiAssembly, ApiAssemblyContribution, assemble_api_router_from_env, web_module};

pub fn assembly_route_count() -> usize {
    generated::ROUTE_CRATE_COUNT
}
