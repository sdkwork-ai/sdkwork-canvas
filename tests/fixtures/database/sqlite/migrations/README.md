# SQLite Migrations

此目录存放 GA 后的增量迁移文件(SQLite 引擎)。

当前阶段:baseline 模式,所有初始化 DDL 位于 `ddl/baseline/sqlite/0001_canvas_baseline.sql`。

迁移文件命名规范: `NNNN_description.sql`(如 `0002_add_user_avatar_column.sql`)。
