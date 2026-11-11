import assert from 'node:assert/strict';
import { readFile, readdir, stat } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';

const ROOT = process.cwd();

async function exists(relativePath) {
  try {
    await stat(path.join(ROOT, relativePath));
    return true;
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return false;
    }
    throw error;
  }
}

async function absoluteExists(absolutePath) {
  try {
    await stat(absolutePath);
    return true;
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return false;
    }
    throw error;
  }
}

async function read(relativePath) {
  return readFile(path.join(ROOT, relativePath), 'utf8');
}

async function readJson(relativePath) {
  return JSON.parse(await read(relativePath));
}

async function walk(relativePath) {
  if (!(await exists(relativePath))) {
    return [];
  }

  const absolutePath = path.join(ROOT, relativePath);
  const metadata = await stat(absolutePath);
  if (metadata.isFile()) {
    return [relativePath];
  }

  const entries = await readdir(absolutePath, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const childPath = path.join(relativePath, entry.name);
    if (entry.isDirectory()) {
      files.push(...await walk(childPath));
    } else if (entry.isFile()) {
      files.push(childPath);
    }
  }
  return files;
}

test('declares root Rust workspace service and app-api route crates for Canvas runtime', async () => {
  assert.equal(await exists('Cargo.toml'), true, 'root Cargo.toml should exist');

  const cargo = await read('Cargo.toml');
  assert.match(cargo, /\[workspace\]/);
  assert.match(cargo, /"crates\/sdkwork-canvas-pages-service"/);
  assert.match(cargo, /"crates\/sdkwork-canvas-pages-repository-sqlx"/);
  assert.match(cargo, /"crates\/sdkwork-routes-canvas-app-api"/);
  assert.match(cargo, /"crates\/sdkwork-routes-canvas-backend-api"/);
  assert.doesNotMatch(cargo, /packages\/native-rust/);
  assert.doesNotMatch(cargo, /sdkwork-routes-notes/);

  const productCargo = await read('crates/sdkwork-canvas-pages-service/Cargo.toml');
  assert.match(productCargo, /name\s*=\s*"sdkwork-canvas-pages-service"/);

  const repositoryCargo = await read('crates/sdkwork-canvas-pages-repository-sqlx/Cargo.toml');
  assert.match(repositoryCargo, /name\s*=\s*"sdkwork-canvas-pages-repository-sqlx"/);

  const appApiCargo = await read('crates/sdkwork-routes-canvas-app-api/Cargo.toml');
  assert.match(appApiCargo, /name\s*=\s*"sdkwork-routes-canvas-app-api"/);
  assert.match(appApiCargo, /name\s*=\s*"sdkwork_routes_canvas_app_api"/);

  const backendApiCargo = await read('crates/sdkwork-routes-canvas-backend-api/Cargo.toml');
  assert.match(backendApiCargo, /name\s*=\s*"sdkwork-routes-canvas-backend-api"/);
  assert.match(backendApiCargo, /name\s*=\s*"sdkwork_routes_canvas_backend_api"/);
});

test('does not create generated SDK transport output inside Canvas SDK families', async () => {
  const generatedTransportRoots = [
    'sdks/sdkwork-canvas-sdk/generated/server-openapi',
    'sdks/sdkwork-canvas-app-sdk/generated/server-openapi',
    'sdks/sdkwork-canvas-backend-sdk/generated/server-openapi'
  ];

  for (const relativePath of generatedTransportRoots) {
    assert.equal(
      await exists(relativePath),
      false,
      `${relativePath} must stay absent until canonical sdkgen output is produced`
    );
  }
});

test('declares component specs for new Rust service crates', async () => {
  for (const relativePath of [
    'crates/sdkwork-canvas-pages-service/specs/component.spec.json',
    'crates/sdkwork-canvas-pages-repository-sqlx/specs/component.spec.json',
    'crates/sdkwork-routes-canvas-app-api/specs/component.spec.json',
    'crates/sdkwork-routes-canvas-backend-api/specs/component.spec.json'
  ]) {
    assert.equal(await exists(relativePath), true, `${relativePath} should exist`);
    const componentSpec = await readJson(relativePath);
    assert.equal(componentSpec.kind, 'sdkwork.component.spec');
    assert.ok(componentSpec.canonicalSpecs.some((spec) => spec.file === 'CODE_STYLE_SPEC.md'));
    assert.ok(componentSpec.canonicalSpecs.some((spec) => spec.file === 'NAMING_SPEC.md'));
    assert.ok(componentSpec.canonicalSpecs.some((spec) => spec.file === 'RUST_CODE_SPEC.md'));
    assert.deepEqual(componentSpec.contracts.dependencyApiExports, []);
    for (const canonicalSpec of componentSpec.canonicalSpecs) {
      const specPath = path.resolve(ROOT, path.dirname(relativePath), canonicalSpec.path);
      assert.equal(
        await absoluteExists(specPath),
        true,
        `${relativePath} canonical spec path should resolve: ${canonicalSpec.path}`
      );
    }
  }
});

test('declares a route manifest artifact aligned with the Canvas App OpenAPI authority', async () => {
  const manifestPath = 'sdks/_route-manifests/app-api/sdkwork-routes-canvas-app-api.route-manifest.json';
  assert.equal(await exists(manifestPath), true, `${manifestPath} should exist`);

  const manifest = await readJson(manifestPath);
  assert.equal(manifest.schemaVersion, 1);
  assert.equal(manifest.kind, 'sdkwork.route.manifest');
  assert.equal(manifest.packageName, 'sdkwork-routes-canvas-app-api');
  assert.equal(manifest.surface, 'app-api');
  assert.equal(manifest.owner, 'sdkwork-canvas');
  assert.equal(manifest.domain, 'canvas');
  assert.equal(manifest.capability, 'canvas');
  assert.equal(manifest.apiAuthority, 'sdkwork-canvas-app-api');
  assert.equal(manifest.sdkFamily, 'sdkwork-canvas-app-sdk');
  assert.equal(manifest.prefix, '/app/v3/api');
  assert.deepEqual(manifest.source, {
    crateRoot: 'crates/sdkwork-routes-canvas-app-api',
    crateImport: 'sdkwork_routes_canvas_app_api'
  });

  const componentSpec = await readJson('crates/sdkwork-routes-canvas-app-api/specs/component.spec.json');
  assert.equal(componentSpec.component.type, 'rust-route-crate');
  assert.equal(componentSpec.contracts.routeManifest, '../../../sdks/_route-manifests/app-api/sdkwork-routes-canvas-app-api.route-manifest.json');

  const openapi = await readJson('apis/app-api/canvas/canvas-app-api.openapi.json');
  const openapiOperations = new Map();
  for (const [apiPath, pathItem] of Object.entries(openapi.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!['get', 'post', 'put', 'patch', 'delete'].includes(method)) {
        continue;
      }
      openapiOperations.set(`${method.toUpperCase()} ${apiPath}`, operation);
    }
  }

  const expectedImplementedOperations = new Set([
    'GET /app/v3/api/canvas/workspaces',
    'POST /app/v3/api/canvas/workspaces',
    'GET /app/v3/api/canvas/workspaces/{workspaceId}/bootstrap',
    'GET /app/v3/api/canvas/workspaces/{workspaceId}/boards',
    'POST /app/v3/api/canvas/workspaces/{workspaceId}/boards',
    'GET /app/v3/api/canvas/boards/{pageId}',
    'PATCH /app/v3/api/canvas/boards/{pageId}',
    'POST /app/v3/api/canvas/boards/{pageId}/remote_apply',
    'GET /app/v3/api/canvas/boards/{pageId}/content',
    'PUT /app/v3/api/canvas/boards/{pageId}/content',
    'GET /app/v3/api/canvas/boards/{pageId}/versions',
    'POST /app/v3/api/canvas/boards/{pageId}/versions/{driveVersionId}/restore',
    'GET /app/v3/api/canvas/boards/{pageId}/ai_suggestions',
    'POST /app/v3/api/canvas/ai_suggestions/{aiSuggestionId}/accept',
    'POST /app/v3/api/canvas/ai_suggestions/{aiSuggestionId}/reject',
    'POST /app/v3/api/canvas/ai_suggestions/{aiSuggestionId}/apply',
    'POST /app/v3/api/canvas/ai_suggestions/{aiSuggestionId}/feedback',
    'GET /app/v3/api/canvas/search',
    'POST /app/v3/api/canvas/ai_jobs'
  ]);

  const manifestOperations = new Map();
  for (const route of manifest.routes ?? []) {
    const key = `${route.method} ${route.path}`;
    manifestOperations.set(key, route);
    assert.ok(expectedImplementedOperations.has(key), `${key} is not an implemented App API operation`);
    assert.ok(route.handler?.module, `${key} should declare handler.module`);
    assert.ok(route.handler?.name, `${key} should declare handler.name`);
    assert.deepEqual(route.ownership, {
      owner: 'sdkwork-canvas',
      apiAuthority: 'sdkwork-canvas-app-api'
    });
    assert.equal(route.auth?.mode, 'dual-token');
    assert.equal(route.requestContext, 'WebRequestContext');
    assert.equal(route.apiSurface, 'app-api');
  }

  assert.deepEqual(new Set(manifestOperations.keys()), expectedImplementedOperations);

  for (const [key, route] of manifestOperations) {
    const operation = openapiOperations.get(key);
    assert.ok(operation, `${key} should exist in apis/app-api/canvas/canvas-app-api.openapi.json`);
    assert.equal(route.operationId, operation.operationId);
    assert.equal(route.ownership.owner, operation['x-sdkwork-owner']);
    assert.equal(operation['x-sdkwork-api-authority'], 'sdkwork-canvas-app-api');
  }
});

test('declares a backend route manifest artifact aligned with the Canvas Backend OpenAPI authority', async () => {
  const manifestPath = 'sdks/_route-manifests/backend-api/sdkwork-routes-canvas-backend-api.route-manifest.json';
  assert.equal(await exists(manifestPath), true, `${manifestPath} should exist`);

  const manifest = await readJson(manifestPath);
  assert.equal(manifest.schemaVersion, 1);
  assert.equal(manifest.kind, 'sdkwork.route.manifest');
  assert.equal(manifest.packageName, 'sdkwork-routes-canvas-backend-api');
  assert.equal(manifest.surface, 'backend-api');
  assert.equal(manifest.owner, 'sdkwork-canvas');
  assert.equal(manifest.domain, 'canvas');
  assert.equal(manifest.capability, 'canvas');
  assert.equal(manifest.apiAuthority, 'sdkwork-canvas-backend-api');
  assert.equal(manifest.sdkFamily, 'sdkwork-canvas-backend-sdk');
  assert.equal(manifest.prefix, '/backend/v3/api');
  assert.deepEqual(manifest.source, {
    crateRoot: 'crates/sdkwork-routes-canvas-backend-api',
    crateImport: 'sdkwork_routes_canvas_backend_api'
  });

  const componentSpec = await readJson('crates/sdkwork-routes-canvas-backend-api/specs/component.spec.json');
  assert.equal(componentSpec.component.type, 'rust-route-crate');
  assert.equal(componentSpec.contracts.routeManifest, '../../../sdks/_route-manifests/backend-api/sdkwork-routes-canvas-backend-api.route-manifest.json');

  const openapi = await readJson('apis/backend-api/canvas/canvas-backend-api.openapi.json');
  const openapiOperations = new Map();
  for (const [apiPath, pathItem] of Object.entries(openapi.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!['get', 'post', 'put', 'patch', 'delete'].includes(method)) {
        continue;
      }
      openapiOperations.set(`${method.toUpperCase()} ${apiPath}`, operation);
    }
  }

  const expectedImplementedOperations = new Set([
    'GET /backend/v3/api/canvas/ai_jobs',
    'GET /backend/v3/api/canvas/ai_jobs/{aiJobId}',
    'POST /backend/v3/api/canvas/ai_jobs/{aiJobId}/cancel',
    'POST /backend/v3/api/canvas/ai_jobs/{aiJobId}/claim',
    'POST /backend/v3/api/canvas/ai_jobs/{aiJobId}/complete',
    'POST /backend/v3/api/canvas/ai_suggestions/{aiSuggestionId}/accept',
    'POST /backend/v3/api/canvas/ai_suggestions/{aiSuggestionId}/reject',
    'POST /backend/v3/api/canvas/ai_suggestions/{aiSuggestionId}/apply',
    'GET /backend/v3/api/canvas/ai_suggestions/{aiSuggestionId}/feedback'
  ]);

  const manifestOperations = new Map();
  for (const route of manifest.routes ?? []) {
    const key = `${route.method} ${route.path}`;
    manifestOperations.set(key, route);
    assert.ok(expectedImplementedOperations.has(key), `${key} is not an implemented Backend API operation`);
    assert.ok(route.handler?.module, `${key} should declare handler.module`);
    assert.ok(route.handler?.name, `${key} should declare handler.name`);
    assert.deepEqual(route.ownership, {
      owner: 'sdkwork-canvas',
      apiAuthority: 'sdkwork-canvas-backend-api'
    });
    assert.equal(route.auth?.mode, 'dual-token');
    assert.equal(route.requestContext, 'WebRequestContext');
    assert.equal(route.apiSurface, 'backend-api');
  }

  assert.deepEqual(new Set(manifestOperations.keys()), expectedImplementedOperations);

  for (const [key, route] of manifestOperations) {
    const operation = openapiOperations.get(key);
    assert.ok(operation, `${key} should exist in apis/backend-api/canvas/canvas-backend-api.openapi.json`);
    assert.equal(route.operationId, operation.operationId);
    assert.equal(route.ownership.owner, operation['x-sdkwork-owner']);
    assert.equal(operation['x-sdkwork-api-authority'], 'sdkwork-canvas-backend-api');
  }
});

test('keeps Canvas service source free of Drive-owned storage lifecycle terms', async () => {
  const files = [
    ...await walk('services'),
    ...await walk('crates')
  ]
    .filter((file) => /[\\/]src[\\/].*\.(rs|sql)$/.test(file));
  assert.ok(files.length > 0, 'service source files should be discoverable');

  const forbiddenPatterns = [
    { label: ['storage', 'object'].join('_'), regex: new RegExp(['storage', 'object'].join('_'), 'i') },
    { label: ['upload', 'session'].join('_'), regex: new RegExp(['upload', 'session'].join('_'), 'i') },
    { label: 'buck' + 'et', regex: new RegExp('buck' + 'et', 'i') },
    { label: ['object', 'key'].join('_'), regex: new RegExp(['object', 'key'].join('_'), 'i') },
    { label: ['canvas', 'note'].join('_'), regex: new RegExp(['canvas', 'note'].join('_'), 'i') },
    { label: ['canvas', 'revision'].join('_'), regex: new RegExp(['canvas', 'revision'].join('_'), 'i') },
    { label: '/canvas/' + 'canvas', regex: new RegExp('/canvas/' + 'canvas', 'i') }
  ];

  const driveAdapterAllowlist = new Set([
    'crates/sdkwork-api-canvas-assembly/src/service/drive_http_adapter.rs',
    'crates/sdkwork-api-canvas-assembly/src/service/drive_in_process.rs',
  ]);

  const findings = [];
  for (const file of files) {
    const normalized = file.replaceAll('\\', '/');
    if (driveAdapterAllowlist.has(normalized)) {
      continue;
    }
    const text = await read(file);
    for (const pattern of forbiddenPatterns) {
      if (pattern.regex.test(text)) {
        findings.push(`${file}: ${pattern.label}`);
      }
    }
  }

  assert.deepEqual(findings, []);
});
