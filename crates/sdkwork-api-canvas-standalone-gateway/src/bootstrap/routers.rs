use std::sync::Arc;

use axum::Router;

use crate::bootstrap::auth::build_protected_router;
use sdkwork_web_bootstrap::{ApiModuleRegistry, CompositeReadinessCheck, service_router, ServiceRouterConfig};

pub async fn build_router() -> Result<Router, Box<dyn std::error::Error + Send + Sync>> {
    // Canvas product service construction and route mounting are owned by the
    // canvas API assembly; the IAM App API surface enters through the IAM API
    // assembly contribution (API_ASSEMBLY_SPEC §3/§6.1).
    let mut module_registry = ApiModuleRegistry::new();
    module_registry.add_module(sdkwork_api_canvas_assembly::assemble_api_router_from_env()
        .await
        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })?);
    let canvas = module_registry.try_compose("SDKWork Canvas API")?;
    let iam = sdkwork_api_iam_assembly::assemble_app_api_contribution()
        .await
        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })?;

    let domain = build_protected_router(canvas.router).await;

    let business = Router::new()
        .merge(iam.router)
        .merge(build_protected_router(domain).await)
        .layer(sdkwork_web_bootstrap::application_cors_layer_from_env(
            &["SDKWORK_CANVAS_ENVIRONMENT"],
            &["SDKWORK_CORS_ALLOWED_ORIGINS"],
        ));

    let readiness = Arc::new(CompositeReadinessCheck::new(vec![
        canvas.readiness_check.clone(),
        iam.readiness_check.clone(),
    ]));

    Ok(service_router(
        business,
        ServiceRouterConfig::default().with_readiness_check(readiness),
    ))
}
