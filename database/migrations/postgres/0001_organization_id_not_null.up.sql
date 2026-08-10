-- sdkwork:migration
-- id: 0001_organization_id_not_null
-- engine: postgres
-- module: sdkwork-canvas
-- purpose: Enforce organization_id NOT NULL DEFAULT on all tables in the
--   consolidated baseline. NULL rows (pre-standard data anomalies) are
--   backfilled with the platform sentinel before NOT NULL is set, and
--   NOT NULL columns without an explicit default receive the sentinel
--   default, keeping existing deployments consistent with fresh baseline
--   installs.
-- reversible: false
-- rollback: forward-fix (sentinel backfill is the canonical fix; NULL
--   organization rows are data anomalies)
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

UPDATE canvas_workspace SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE canvas_workspace ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE canvas_workspace ALTER COLUMN organization_id SET NOT NULL;

UPDATE canvas_page SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE canvas_page ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE canvas_page ALTER COLUMN organization_id SET NOT NULL;

UPDATE canvas_page_search_projection SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE canvas_page_search_projection ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE canvas_page_search_projection ALTER COLUMN organization_id SET NOT NULL;

UPDATE canvas_ai_job SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE canvas_ai_job ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE canvas_ai_job ALTER COLUMN organization_id SET NOT NULL;

UPDATE canvas_ai_job_source SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE canvas_ai_job_source ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE canvas_ai_job_source ALTER COLUMN organization_id SET NOT NULL;

UPDATE canvas_ai_suggestion SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE canvas_ai_suggestion ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE canvas_ai_suggestion ALTER COLUMN organization_id SET NOT NULL;

UPDATE canvas_ai_feedback SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE canvas_ai_feedback ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE canvas_ai_feedback ALTER COLUMN organization_id SET NOT NULL;

COMMIT;
