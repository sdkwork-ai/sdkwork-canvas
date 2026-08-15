//! Gateway bootstrap for sdkwork-canvas.

use std::sync::Arc;

use axum::Router;
use sdkwork_canvas_pages_service::service::CanvasPagesService;
pub use sdkwork_web_bootstrap::ApiAssemblyContribution;
use sdkwork_web_bootstrap::{AlwaysReady, HttpRouteManifest};

/// Indivisible host-neutral API assembly contribution (web-bootstrap contract).
pub type ApiAssembly = ApiAssemblyContribution;

pub fn assemble_api_router<R, D>(service: CanvasPagesService<R, D>) -> ApiAssembly
where
    R: sdkwork_canvas_pages_service::ports::CanvasRepository,
    D: sdkwork_canvas_pages_service::ports::DrivePageContentPort,
{
    let app_router = sdkwork_routes_canvas_app_api::gateway_mount(service.clone());
    let backend_router = sdkwork_routes_canvas_backend_api::gateway_mount(service);
    let auth_router = sdkwork_routes_canvas_http_auth::gateway_mount();
    let router = Router::new()
        .merge(app_router)
        .merge(backend_router)
        .merge(auth_router);
    let routes = [
        sdkwork_routes_canvas_app_api::app_route_manifest(),
        sdkwork_routes_canvas_backend_api::backend_route_manifest(),
    ]
    .into_iter()
    .flat_map(|manifest| manifest.routes().to_vec())
    .collect();
    ApiAssemblyContribution::from_manifest(
        "sdkwork-canvas",
        "SDKWork Canvas API",
        router,
        HttpRouteManifest::from_owned_routes(routes),
        Vec::new(),
        Arc::new(AlwaysReady),
    )
    .expect("sdkwork-canvas API assembly contribution must be valid")
}

/// Boots the canvas product service from the process environment and assembles
/// the complete host-neutral contribution (API_ASSEMBLY_SPEC §3/§6.1).
/// Consuming gateways call this entrypoint instead of importing
/// `sdkwork-canvas-pages-service` or `sdkwork-canvas-pages-repository-sqlx`
/// directly.
pub async fn assemble_api_router_from_env() -> Result<ApiAssembly, String> {
    let service = crate::service::database::assemble_canvas_service_from_env().await?;
    Ok(assemble_api_router(service))
}
