-- SDKWork Canvas bootstrap seed (intentionally empty).
--
-- Canvas owns canvas_page / canvas_workspace / canvas_ai_* tables only; it has
-- no platform-level settings table. Pages and workspaces are created by users
-- at runtime, so the module declares no reference data. This file keeps the
-- standard seed profile non-empty for the lifecycle smoke path without writing
-- to a table the module does not own.
SELECT 1;
