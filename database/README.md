# Canvas Database

Database module `canvas` (`database/database.manifest.json`).

## Layout

- `contract/schema.yaml` — portable schema (`canvas_*` tables)
- `ddl/baseline/{engine}/0001_canvas_baseline.sql` — initialization DDL
- `migrations/{engine}/` — post-GA forward migrations
- `seeds/` — locale-aware seeds

Lifecycle: `pnpm db:plan`, `db:init`, `db:migrate`, `db:bootstrap`, `db:drift:check`.

Authority: `../../sdkwork-specs/DATABASE_FRAMEWORK_SPEC.md`

## Initialization state

This module is in **initialization state** for greenfield deployments:

1. **Baseline** — `database/ddl/baseline/{engine}/0001_canvas_baseline.sql` contains the full DDL snapshot.
2. **Migrations** — `database/migrations/{engine}/` is reserved for post-GA incremental schema changes only. It is intentionally empty at initialization.
3. **Drift** — run `pnpm db:drift:check` before release.

## Commands

```bash
pnpm run db:validate
pnpm run db:materialize:contract
pnpm run db:plan
pnpm run db:init
pnpm run db:migrate
pnpm run db:seed
pnpm run db:status
pnpm run db:drift:check
```
