use std::path::PathBuf;

use sdkwork_iam_embedded_application_bootstrap::{
    ensure_tenant_application_from_app_root_with_env_and_fallback, resolve_bootstrap_environment,
};

pub async fn ensure_canvas_tenant_application_bootstrap() -> Result<(), String> {
    let app_root = resolve_canvas_app_root();
    sdkwork_iam_database_host::unified_postgres_env::apply_unified_cloud_postgres_env(&app_root);
    ensure_tenant_application_from_app_root_with_env_and_fallback(
        resolve_bootstrap_environment().as_str(),
        app_root,
        None,
        &[],
    )
    .await
}

fn resolve_canvas_app_root() -> PathBuf {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let pc_react_root = repo_root.join("sdkwork-canvas-pc-react");
    if pc_react_root.join("sdkwork.app.config.json").is_file() {
        pc_react_root
            .canonicalize()
            .unwrap_or(pc_react_root)
    } else {
        repo_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_app_root_resolves_to_pc_react_manifest_root() {
        let root = resolve_canvas_app_root();
        assert!(root.join("sdkwork.app.config.json").is_file());
    }
}
