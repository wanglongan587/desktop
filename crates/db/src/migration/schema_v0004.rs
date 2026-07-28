use super::Migration;

const UP_STATEMENTS: &[&str] = &[r#"
CREATE TABLE IF NOT EXISTS plugins (
    id TEXT PRIMARY KEY,
    kind INTEGER NOT NULL,
    version TEXT NOT NULL,
    entrypoint TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT NOT NULL,
    state INTEGER NOT NULL,
    source_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    is_deleted INTEGER NOT NULL DEFAULT 0
);
"#];

const DOWN_STATEMENTS: &[&str] = &[r#"
DROP TABLE IF EXISTS plugins;
"#];

/// Builds the plugin catalog migration.
pub fn migration() -> Migration {
    Migration::new("0004", UP_STATEMENTS, DOWN_STATEMENTS)
}
