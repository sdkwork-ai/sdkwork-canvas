# Canvas topology profiles

Machine contract: `specs/topology.spec.json` (`schemaVersion: 2`, archetype `application-http-gateway`).

Platform standard: `../../sdkwork-specs/APP_RUNTIME_TOPOLOGY_ADOPTION.md`

## Active profiles

| Profile id | Command |
| --- | --- |
| `standalone.development` | `pnpm dev`, `pnpm dev:browser`, `pnpm dev:desktop` |
| `cloud.development` | `pnpm dev:browser:cloud`, `pnpm dev:desktop:cloud` |
| `standalone.production` | self-hosted production build |
| `cloud.production` | cloud production deploy |

Loader: `scripts/lib/canvas-topology.mjs` → `@sdkwork/app-topology`.
