# SDKWork Canvas PC Specs

This directory is the PC application-local spec index for `apps/sdkwork-canvas-pc`.

## Files

- `README.md`: human index for this application root.
- `../packages/sdkwork-canvas-pc-core/specs/component.spec.json`: machine-readable PC core composition contract, including SDK dependencies and permission inheritance.

## Authority

The parent repository manifest at `../../agent.app.config.json` owns Canvas application identity. Global standards remain in `../../../sdkwork-specs/`; this directory links to those standards and does not copy their rule bodies.

Canvas PC follows `APP_PC_ARCHITECTURE_SPEC.md`, `FRONTEND_SPEC.md`, `APP_COMPOSITION_SPEC.md`, `APP_SDK_INTEGRATION_SPEC.md`, `APP_PERMISSION_COMPOSITION_SPEC.md`, `TYPESCRIPT_CODE_SPEC.md`, `FRONTEND_CODE_SPEC.md`, `CODE_STYLE_SPEC.md`, and `NAMING_SPEC.md` on demand.

## Runtime

- Browser runtime config example: `config/browser/runtime-env.development.example.json`
- Application surface manifest: `agent.app.config.json`
- Standard development command: run `pnpm dev` from the repository root.

Feature packages must consume SDK and session capability through PC core public exports. They must not import generated SDK transport names, dependency-private source paths, or manual auth/header utilities.
