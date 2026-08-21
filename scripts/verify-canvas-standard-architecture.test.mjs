import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';

const ROOT = process.cwd();
const PC_ROOT = 'apps/sdkwork-canvas-pc';

function read(relativePath) {
  return readFileSync(path.join(ROOT, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath).replace(/^\uFEFF/u, ''));
}

function exists(relativePath) {
  return existsSync(path.join(ROOT, relativePath));
}

test('declares SDKWork standard root directories', () => {
  for (const dir of ['apis', 'apps', 'crates', 'database', 'sdks', 'deployments', 'docs', 'scripts', 'tests', 'specs']) {
    assert.equal(exists(dir), true, `${dir}/ must exist`);
  }
});

test('declares workspace agent entrypoints and packaging workflow', () => {
  for (const file of ['AGENTS.md', 'README.md', 'Cargo.toml', 'sdkwork.workflow.json', '.github/workflows/package.yml']) {
    assert.equal(exists(file), true, `${file} must exist`);
  }
  const workflow = readJson('sdkwork.workflow.json');
  assert.equal(workflow.app.id, 'sdkwork-canvas');
  assert.equal(workflow.app.configPath, 'sdkwork.app.config.json');
});

test('declares Canvas OpenAPI authorities', () => {
  for (const file of [
    'apis/app-api/canvas/canvas-app-api.openapi.json',
    'apis/backend-api/canvas/canvas-backend-api.openapi.json',
  ]) {
    assert.equal(exists(file), true, file);
    assert.equal(readJson(file).openapi, '3.1.2');
  }
});

test('integrates sdkwork-web-framework and sdkwork-utils in Rust workspace', () => {
  const cargo = read('Cargo.toml');
  assert.match(cargo, /sdkwork-web-core/);
  assert.match(cargo, /sdkwork-utils-rust/);
  assert.match(cargo, /sdkwork-database-config/);
  assert.match(cargo, /sdkwork_drive_app_sdk_generated_rust/);
});

test('declares route manifest aligned with app-api OpenAPI', () => {
  const manifest = readJson('sdks/_route-manifests/app-api/sdkwork-routes-canvas-app-api.route-manifest.json');
  assert.equal(manifest.owner, 'sdkwork-canvas');
  assert.ok(manifest.routes.length >= 10);
});

test('Rust workspace members include canvas pages and gateway crates', () => {
  const cargo = read('Cargo.toml');
  for (const member of [
    'crates/sdkwork-canvas-pages-service',
    'crates/sdkwork-routes-canvas-app-api',
    'crates/sdkwork-api-canvas-standalone-gateway',
  ]) {
    assert.match(cargo, new RegExp(member.replaceAll('/', '\\/')));
  }
});

test('does not declare sdkwork-discovery without RPC services', () => {
  const cargo = read('Cargo.toml');
  assert.doesNotMatch(cargo, /path = "\.\.\/sdkwork-discovery/);
  assert.equal(exists('apis/rpc'), false);
});

test('PC application root under apps/sdkwork-canvas-pc', () => {
  assert.equal(exists(`${PC_ROOT}/sdkwork.app.config.json`), true);
  assert.equal(exists(`${PC_ROOT}/AGENTS.md`), true);
  const manifest = readJson(`${PC_ROOT}/sdkwork.app.config.json`);
  assert.equal(manifest.kind, 'sdkwork.app.surface');
});

test('declares SDKWORK_DEPLOY_SPEC deployments/deploy.yaml', () => {
  const deploy = read('deployments/deploy.yaml');
  assert.match(deploy, /^version:\s*2/m);
  assert.match(deploy, /defaultProfile:\s*cloud\.production/);
  assert.match(deploy, /domain:\s*canvas\.sdkwork\.com/);
});

test('wires database validation into pnpm check', () => {
  const pkg = readJson('package.json');
  assert.match(pkg.scripts.check, /db:validate/);
  assert.match(pkg.scripts.check, /check-api-response-envelope/);
});

test('database module id is canvas', () => {
  const manifest = readJson('database/database.manifest.json');
  assert.equal(manifest.moduleId, 'canvas');
  assert.equal(manifest.serviceCode, 'CANVAS');
});

test('integrates sdkwork-drive in standalone gateway bootstrap', () => {
  const facade = read('crates/sdkwork-api-canvas-standalone-gateway/src/bootstrap/drive_app_sdk_facade.rs');
  assert.match(facade, /sdkwork_drive_app_sdk_generated_rust/);
});
