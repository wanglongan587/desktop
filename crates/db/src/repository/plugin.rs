use ora_application::{PluginRepository, PluginRepositoryError};
use ora_domain::{AuditFields, Plugin, PluginId, PluginKind, PluginLifecycleState};
use rusqlite::{Row, params};

use crate::repository::{RepositoryPool, connection::bool_to_sqlite};

/// Persists installed plugins in SQLite.
#[derive(Clone, Debug)]
pub struct SqlitePluginRepository {
    pool: RepositoryPool,
}

impl SqlitePluginRepository {
    /// Builds a plugin repository from the shared SQLite connection pool.
    pub fn new(pool: RepositoryPool) -> Self {
        Self { pool }
    }
}

impl PluginRepository for SqlitePluginRepository {
    fn create_plugin(&self, plugin: Plugin) -> Result<Plugin, PluginRepositoryError> {
        self.pool.with_connection(|connection| {
            connection.execute(
                "INSERT INTO plugins (id, kind, version, entrypoint, display_name, description, state, source_path, created_at, updated_at, is_deleted) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    plugin.id.to_string(),
                    plugin.kind.database_value(),
                    &plugin.version,
                    &plugin.entrypoint,
                    &plugin.display_name,
                    &plugin.description,
                    plugin.state.database_value(),
                    &plugin.source_path,
                    plugin.audit_fields.created_at,
                    plugin.audit_fields.updated_at,
                    bool_to_sqlite(plugin.audit_fields.is_deleted),
                ],
            )?;
            Ok(plugin)
        }).map_err(plugin_repository_error_from_database)
    }

    fn find_plugin(&self, plugin_id: &PluginId) -> Result<Option<Plugin>, PluginRepositoryError> {
        self.pool.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, kind, version, entrypoint, display_name, description, state, source_path, created_at, updated_at, is_deleted FROM plugins WHERE id = ?1 AND is_deleted = 0",
            )?;
            let mut rows = statement.query(params![plugin_id.to_string()])?;
            rows.next()?.map(map_plugin_row).transpose()
        }).map_err(plugin_repository_error_from_database)
    }

    fn list_plugins(&self) -> Result<Vec<Plugin>, PluginRepositoryError> {
        self.pool.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, kind, version, entrypoint, display_name, description, state, source_path, created_at, updated_at, is_deleted FROM plugins WHERE is_deleted = 0 ORDER BY created_at ASC, id ASC",
            )?;
            let mut rows = statement.query([])?;
            let mut plugins = Vec::new();
            while let Some(row) = rows.next()? {
                plugins.push(map_plugin_row(row)?);
            }
            Ok(plugins)
        }).map_err(plugin_repository_error_from_database)
    }

    fn update_state(
        &self,
        plugin_id: &PluginId,
        state: PluginLifecycleState,
        updated_at: i64,
    ) -> Result<bool, PluginRepositoryError> {
        self.pool
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE plugins SET state = ?2, updated_at = ?3 WHERE id = ?1 AND is_deleted = 0",
                        params![plugin_id.to_string(), state.database_value(), updated_at],
                    )
                    .map(|rows| rows > 0)
                    .map_err(Into::into)
            })
            .map_err(plugin_repository_error_from_database)
    }

    fn soft_delete_plugin(
        &self,
        plugin_id: &PluginId,
        deleted_at: i64,
    ) -> Result<bool, PluginRepositoryError> {
        self.pool
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE plugins SET updated_at = ?2, is_deleted = 1 WHERE id = ?1 AND is_deleted = 0",
                        params![plugin_id.to_string(), deleted_at],
                    )
                    .map(|rows| rows > 0)
                    .map_err(Into::into)
            })
            .map_err(plugin_repository_error_from_database)
    }
}

/// Reconstructs a domain plugin from a selected SQLite row.
fn map_plugin_row(row: &Row<'_>) -> Result<Plugin, crate::DatabaseError> {
    let kind = PluginKind::from_database_value(row.get::<_, i64>("kind")?)?;
    let state = PluginLifecycleState::from_database_value(row.get::<_, i64>("state")?)?;
    Plugin::new(
        PluginId::new(row.get::<_, String>("id")?),
        kind,
        row.get::<_, String>("version")?,
        row.get::<_, String>("entrypoint")?,
        row.get::<_, String>("display_name")?,
        row.get::<_, String>("description")?,
        state,
        row.get::<_, String>("source_path")?,
        AuditFields::new(
            row.get("created_at")?,
            row.get("updated_at")?,
            row.get::<_, i64>("is_deleted")? != 0,
        ),
    )
    .map_err(Into::into)
}

/// Converts database failures into application-port errors.
fn plugin_repository_error_from_database(error: crate::DatabaseError) -> PluginRepositoryError {
    PluginRepositoryError::OperationFailed(error.to_string())
}
