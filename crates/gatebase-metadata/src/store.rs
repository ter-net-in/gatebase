use crate::entities;
use crate::mapping::{
    model_to_access_token, model_to_active_connection, model_to_audit_event,
    model_to_rollback_artifact, model_to_session, model_to_user,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use gatebase_core::{
    AccessToken, ActiveConnection, AuditEvent, RollbackArtifact, Session, SessionId, User,
};
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, Database,
    DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Schema,
    Set, Statement,
};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct MetadataStore {
    db: DatabaseConnection,
}

/// A single row of the unified activity feed (audit + rollback + connection
/// events), produced server-side via a UNION so it can be paginated.
#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub time: String,
    pub category: String,
    pub actor: String,
    pub target: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct AuditEventFilter {
    pub actor: Option<String>,
    pub target: Option<String>,
    pub decision: Option<String>,
    pub search: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct PruneCutoffs {
    pub audit_before: DateTime<Utc>,
    pub rollback_before: DateTime<Utc>,
    pub session_before: DateTime<Utc>,
    pub approval_before: DateTime<Utc>,
    pub active_connection_before: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct PruneResult {
    pub audit_events: u64,
    pub rollback_artifacts: u64,
    pub sessions: u64,
    pub access_tokens: u64,
    pub active_connections: u64,
}

impl MetadataStore {
    pub async fn open_sqlite(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            tokio::fs::create_dir_all(parent).await.with_context(|| {
                format!(
                    "failed to create SQLite metadata directory {}",
                    parent.display()
                )
            })?;
        }
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let db = Database::connect(&url)
            .await
            .with_context(|| format!("failed to open SQLite metadata store at {url}"))?;
        let store = Self { db };
        store.configure_sqlite().await?;
        store.migrate().await?;
        Ok(store)
    }

    async fn configure_sqlite(&self) -> Result<()> {
        let backend = self.db.get_database_backend();
        self.db
            .execute(Statement::from_string(
                backend,
                "PRAGMA journal_mode=WAL;".to_owned(),
            ))
            .await?;
        self.db
            .execute(Statement::from_string(
                backend,
                "PRAGMA busy_timeout = 5000;".to_owned(),
            ))
            .await?;
        Ok(())
    }

    pub async fn migrate(&self) -> Result<()> {
        self.create_table(entities::session::Entity).await?;
        self.create_table(entities::audit_event::Entity).await?;
        self.create_table(entities::rollback_artifact::Entity)
            .await?;
        self.create_table(entities::active_connection::Entity)
            .await?;
        self.create_table(entities::access_token::Entity).await?;
        self.create_table(entities::user::Entity).await?;
        // Added after the table was first shipped; backfill for existing DBs.
        self.ensure_column("audit_events", "rollback_artifact_id", "text")
            .await?;
        Ok(())
    }

    /// Add a column to an existing SQLite table if it is not already present.
    async fn ensure_column(&self, table: &str, column: &str, decl: &str) -> Result<()> {
        let backend = self.db.get_database_backend();
        let rows = self
            .db
            .query_all(Statement::from_string(
                backend,
                format!("PRAGMA table_info({table});"),
            ))
            .await?;
        let exists = rows.iter().any(|row| {
            row.try_get::<String>("", "name")
                .map(|name| name == column)
                .unwrap_or(false)
        });
        if !exists {
            self.db
                .execute(Statement::from_string(
                    backend,
                    format!("ALTER TABLE {table} ADD COLUMN {column} {decl};"),
                ))
                .await?;
        }
        Ok(())
    }

    pub async fn create_user(&self, user: &User) -> Result<()> {
        entities::user::ActiveModel {
            id: Set(user.id.clone()),
            username: Set(user.username.clone()),
            password_hash: Set(user.password_hash.clone()),
            role: Set(user.role.to_string()),
            created_at: Set(user.created_at.to_rfc3339()),
            disabled_at: Set(user.disabled_at.map(|time| time.to_rfc3339())),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn find_user_by_username(&self, username: &str) -> Result<Option<User>> {
        entities::user::Entity::find()
            .filter(entities::user::Column::Username.eq(username))
            .one(&self.db)
            .await?
            .map(model_to_user)
            .transpose()
    }

    pub async fn list_users(&self, limit: Option<u64>, offset: Option<u64>) -> Result<Vec<User>> {
        let mut query =
            entities::user::Entity::find().order_by_asc(entities::user::Column::Username);
        if let Some(limit) = limit {
            query = query.limit(limit);
        }
        if let Some(offset) = offset {
            query = query.offset(offset);
        }
        query
            .all(&self.db)
            .await?
            .into_iter()
            .map(model_to_user)
            .collect()
    }

    pub async fn count_users(&self) -> Result<u64> {
        Ok(entities::user::Entity::find().count(&self.db).await?)
    }

    pub async fn create_access_token(&self, token: &AccessToken) -> Result<()> {
        entities::access_token::ActiveModel {
            id: Set(token.id.clone()),
            token_hash: Set(token.token_hash.clone()),
            actor: Set(token.actor.clone()),
            github_repo: Set(token.github_repo.clone()),
            issue: Set(token.issue),
            target: Set(token.target.clone()),
            created_at: Set(token.created_at.to_rfc3339()),
            expires_at: Set(token.expires_at.to_rfc3339()),
            used_at: Set(token.used_at.map(|time| time.to_rfc3339())),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn find_active_access_token(
        &self,
        repo: &str,
        issue: i64,
        target: &str,
        now: String,
    ) -> Result<Option<AccessToken>> {
        entities::access_token::Entity::find()
            .filter(entities::access_token::Column::GithubRepo.eq(repo))
            .filter(entities::access_token::Column::Issue.eq(issue))
            .filter(entities::access_token::Column::Target.eq(target))
            .filter(entities::access_token::Column::UsedAt.is_null())
            .filter(entities::access_token::Column::ExpiresAt.gt(now))
            .one(&self.db)
            .await?
            .map(model_to_access_token)
            .transpose()
    }

    pub async fn consume_access_token(
        &self,
        token_hash: &str,
        now: String,
    ) -> Result<Option<AccessToken>> {
        let Some(model) = entities::access_token::Entity::find()
            .filter(entities::access_token::Column::TokenHash.eq(token_hash))
            .filter(entities::access_token::Column::UsedAt.is_null())
            .filter(entities::access_token::Column::ExpiresAt.gt(&now))
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };
        entities::access_token::Entity::update_many()
            .col_expr(entities::access_token::Column::UsedAt, Expr::value(now))
            .filter(entities::access_token::Column::Id.eq(model.id.clone()))
            .filter(entities::access_token::Column::UsedAt.is_null())
            .exec(&self.db)
            .await?;
        model_to_access_token(model).map(Some)
    }

    pub async fn create_session(&self, session: &Session) -> Result<()> {
        entities::session::ActiveModel {
            id: Set(session.id.to_string()),
            actor: Set(session.actor.clone()),
            source_type: Set(session.source_type.clone()),
            github_repo: Set(session.github_repo.clone()),
            issue: Set(session.issue),
            target: Set(session.target.clone()),
            scopes: Set(serde_json::to_string(&session.scopes)?),
            created_at: Set(session.created_at.to_rfc3339()),
            expires_at: Set(session.expires_at.to_rfc3339()),
            revoked_at: Set(session.revoked_at.map(|time| time.to_rfc3339())),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn revoke_session(&self, session_id: &SessionId, revoked_at: String) -> Result<()> {
        entities::session::Entity::update_many()
            .col_expr(
                entities::session::Column::RevokedAt,
                Expr::value(revoked_at),
            )
            .filter(entities::session::Column::Id.eq(session_id.to_string()))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn get_session(&self, session_id: &SessionId) -> Result<Option<Session>> {
        let Some(model) = entities::session::Entity::find_by_id(session_id.to_string())
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(model_to_session(model)?))
    }

    pub async fn list_sessions(
        &self,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<Session>> {
        let mut query =
            entities::session::Entity::find().order_by_desc(entities::session::Column::CreatedAt);
        if let Some(limit) = limit {
            query = query.limit(limit);
        }
        if let Some(offset) = offset {
            query = query.offset(offset);
        }
        query
            .all(&self.db)
            .await?
            .into_iter()
            .map(model_to_session)
            .collect()
    }

    pub async fn create_active_connection(&self, connection: &ActiveConnection) -> Result<()> {
        entities::active_connection::ActiveModel {
            id: Set(connection.id.clone()),
            session_id: Set(connection.session_id.to_string()),
            target: Set(connection.target.clone()),
            client_addr: Set(connection.client_addr.clone()),
            connected_at: Set(connection.connected_at.to_rfc3339()),
            disconnected_at: Set(connection.disconnected_at.map(|time| time.to_rfc3339())),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn close_active_connection(&self, id: &str, disconnected_at: String) -> Result<()> {
        entities::active_connection::Entity::update_many()
            .col_expr(
                entities::active_connection::Column::DisconnectedAt,
                Expr::value(disconnected_at),
            )
            .filter(entities::active_connection::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn list_active_connections(
        &self,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<ActiveConnection>> {
        let mut query = entities::active_connection::Entity::find()
            .filter(entities::active_connection::Column::DisconnectedAt.is_null())
            .order_by_desc(entities::active_connection::Column::ConnectedAt);
        if let Some(limit) = limit {
            query = query.limit(limit);
        }
        if let Some(offset) = offset {
            query = query.offset(offset);
        }
        query
            .all(&self.db)
            .await?
            .into_iter()
            .map(model_to_active_connection)
            .collect()
    }

    pub async fn write_audit_event(&self, event: &AuditEvent) -> Result<()> {
        entities::audit_event::ActiveModel {
            id: Set(event.id.to_string()),
            session_id: Set(event.session_id.to_string()),
            actor: Set(event.actor.clone()),
            target: Set(event.target.clone()),
            engine: Set(event.engine.to_string()),
            statement: Set(event.statement.clone()),
            decision: Set(format!("{:?}", event.decision).to_ascii_lowercase()),
            rows_affected: Set(event.rows_affected),
            error: Set(event.error.clone()),
            created_at: Set(event.created_at.to_rfc3339()),
            rollback_artifact_id: Set(event.rollback_artifact_id.clone()),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn find_audit_event(&self, id: &str) -> Result<Option<AuditEvent>> {
        let Some(model) = entities::audit_event::Entity::find_by_id(id)
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(model_to_audit_event(model)?))
    }

    pub async fn list_audit_events(&self, filter: AuditEventFilter) -> Result<Vec<AuditEvent>> {
        let mut query = entities::audit_event::Entity::find()
            .order_by_desc(entities::audit_event::Column::CreatedAt);
        if let Some(actor) = filter.actor {
            query = query.filter(entities::audit_event::Column::Actor.eq(actor));
        }
        if let Some(target) = filter.target {
            query = query.filter(entities::audit_event::Column::Target.eq(target));
        }
        if let Some(decision) = filter.decision {
            query = query.filter(entities::audit_event::Column::Decision.eq(decision));
        }
        if let Some(search) = filter.search.map(|search| search.trim().to_owned()) {
            if !search.is_empty() {
                query = query.filter(
                    Condition::any()
                        .add(entities::audit_event::Column::Actor.contains(&search))
                        .add(entities::audit_event::Column::Target.contains(&search))
                        .add(entities::audit_event::Column::Engine.contains(&search))
                        .add(entities::audit_event::Column::Decision.contains(&search))
                        .add(entities::audit_event::Column::Statement.contains(&search))
                        .add(entities::audit_event::Column::Error.contains(&search)),
                );
            }
        }
        if let Some(limit) = filter.limit {
            query = query.limit(limit);
        }
        if let Some(offset) = filter.offset {
            query = query.offset(offset);
        }
        query
            .all(&self.db)
            .await?
            .into_iter()
            .map(model_to_audit_event)
            .collect()
    }

    pub async fn write_rollback_artifact(&self, artifact: &RollbackArtifact) -> Result<()> {
        entities::rollback_artifact::ActiveModel {
            id: Set(artifact.id.clone()),
            session_id: Set(artifact.session_id.to_string()),
            actor: Set(artifact.actor.clone()),
            target: Set(artifact.target.clone()),
            engine: Set(artifact.engine.to_string()),
            statement: Set(artifact.statement.clone()),
            table_name: Set(artifact.table.clone()),
            primary_key_column: Set(artifact.primary_key_column.clone()),
            before_rows: Set(serde_json::to_string(&artifact.before_rows)?),
            inverse_sql: Set(artifact.inverse_sql.clone()),
            manual_required: Set(artifact.manual_required),
            reason: Set(artifact.reason.clone()),
            created_at: Set(artifact.created_at.to_rfc3339()),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn find_rollback_artifact(&self, id: &str) -> Result<Option<RollbackArtifact>> {
        let Some(model) = entities::rollback_artifact::Entity::find_by_id(id)
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(model_to_rollback_artifact(model)?))
    }

    pub async fn list_rollback_artifacts(
        &self,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<RollbackArtifact>> {
        let mut query = entities::rollback_artifact::Entity::find()
            .order_by_desc(entities::rollback_artifact::Column::CreatedAt);
        if let Some(limit) = limit {
            query = query.limit(limit);
        }
        if let Some(offset) = offset {
            query = query.offset(offset);
        }
        query
            .all(&self.db)
            .await?
            .into_iter()
            .map(model_to_rollback_artifact)
            .collect()
    }

    /// Unified, time-ordered activity feed merged across audit events, rollback
    /// artifacts, and connection events. `limit = None` returns all rows.
    pub async fn list_activity(
        &self,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<ActivityEntry>> {
        let backend = self.db.get_database_backend();
        let sql = "\
            SELECT ts, category, actor, target, detail FROM (\
                SELECT created_at AS ts, \
                    CASE WHEN decision = 'blocked' THEN 'audit_blocked' ELSE 'audit_allowed' END AS category, \
                    actor, target, statement AS detail \
                FROM audit_events \
                UNION ALL \
                SELECT created_at AS ts, 'rollback' AS category, actor, target, \
                    COALESCE(inverse_sql, statement) AS detail \
                FROM rollback_artifacts \
                UNION ALL \
                SELECT COALESCE(disconnected_at, connected_at) AS ts, \
                    CASE WHEN disconnected_at IS NULL THEN 'conn_opened' ELSE 'conn_closed' END AS category, \
                    client_addr AS actor, target, session_id AS detail \
                FROM active_connections\
            ) ORDER BY ts DESC LIMIT ? OFFSET ?";
        // SQLite treats LIMIT -1 as unbounded.
        let limit = limit.map(|value| value as i64).unwrap_or(-1);
        let offset = offset.unwrap_or(0) as i64;
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                backend,
                sql,
                [limit.into(), offset.into()],
            ))
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ActivityEntry {
                    time: row.try_get("", "ts")?,
                    category: row.try_get("", "category")?,
                    actor: row.try_get("", "actor")?,
                    target: row.try_get("", "target")?,
                    detail: row.try_get("", "detail")?,
                })
            })
            .collect()
    }

    pub async fn prune(&self, cutoffs: &PruneCutoffs, dry_run: bool) -> Result<PruneResult> {
        let audit_before = cutoffs.audit_before.to_rfc3339();
        let rollback_before = cutoffs.rollback_before.to_rfc3339();
        let session_before = cutoffs.session_before.to_rfc3339();
        let approval_before = cutoffs.approval_before.to_rfc3339();
        let active_connection_before = cutoffs.active_connection_before.to_rfc3339();

        let result = PruneResult {
            audit_events: prune_audit_events(&self.db, &audit_before, dry_run).await?,
            rollback_artifacts: prune_rollback_artifacts(&self.db, &rollback_before, dry_run)
                .await?,
            sessions: prune_sessions(&self.db, &session_before, dry_run).await?,
            access_tokens: prune_access_tokens(&self.db, &approval_before, dry_run).await?,
            active_connections: prune_active_connections(
                &self.db,
                &active_connection_before,
                dry_run,
            )
            .await?,
        };
        if !dry_run && result.total() > 0 {
            self.compact_sqlite().await?;
        }
        Ok(result)
    }

    async fn compact_sqlite(&self) -> Result<()> {
        let backend = self.db.get_database_backend();
        self.db
            .execute(Statement::from_string(
                backend,
                "PRAGMA wal_checkpoint(TRUNCATE);".to_owned(),
            ))
            .await?;
        self.db
            .execute(Statement::from_string(backend, "VACUUM;".to_owned()))
            .await?;
        Ok(())
    }

    async fn create_table<E>(&self, entity: E) -> Result<()>
    where
        E: EntityTrait,
    {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);
        let mut statement = schema.create_table_from_entity(entity);
        statement.if_not_exists();
        self.db.execute(backend.build(&statement)).await?;
        Ok(())
    }
}

impl PruneResult {
    #[must_use]
    pub fn total(&self) -> u64 {
        self.audit_events
            + self.rollback_artifacts
            + self.sessions
            + self.access_tokens
            + self.active_connections
    }
}

async fn prune_audit_events(db: &DatabaseConnection, before: &str, dry_run: bool) -> Result<u64> {
    let filter = entities::audit_event::Column::CreatedAt.lt(before);
    if dry_run {
        return Ok(entities::audit_event::Entity::find()
            .filter(filter)
            .count(db)
            .await?);
    }
    Ok(entities::audit_event::Entity::delete_many()
        .filter(filter)
        .exec(db)
        .await?
        .rows_affected)
}

async fn prune_rollback_artifacts(
    db: &DatabaseConnection,
    before: &str,
    dry_run: bool,
) -> Result<u64> {
    let filter = entities::rollback_artifact::Column::CreatedAt.lt(before);
    if dry_run {
        return Ok(entities::rollback_artifact::Entity::find()
            .filter(filter)
            .count(db)
            .await?);
    }
    Ok(entities::rollback_artifact::Entity::delete_many()
        .filter(filter)
        .exec(db)
        .await?
        .rows_affected)
}

async fn prune_sessions(db: &DatabaseConnection, before: &str, dry_run: bool) -> Result<u64> {
    let filter = entities::session::Column::ExpiresAt.lt(before);
    if dry_run {
        return Ok(entities::session::Entity::find()
            .filter(filter)
            .count(db)
            .await?);
    }
    Ok(entities::session::Entity::delete_many()
        .filter(filter)
        .exec(db)
        .await?
        .rows_affected)
}

async fn prune_access_tokens(db: &DatabaseConnection, before: &str, dry_run: bool) -> Result<u64> {
    let filter = entities::access_token::Column::ExpiresAt.lt(before);
    if dry_run {
        return Ok(entities::access_token::Entity::find()
            .filter(filter)
            .count(db)
            .await?);
    }
    Ok(entities::access_token::Entity::delete_many()
        .filter(filter)
        .exec(db)
        .await?
        .rows_affected)
}

async fn prune_active_connections(
    db: &DatabaseConnection,
    before: &str,
    dry_run: bool,
) -> Result<u64> {
    let filter = entities::active_connection::Column::DisconnectedAt.lt(before);
    if dry_run {
        return Ok(entities::active_connection::Entity::find()
            .filter(filter)
            .count(db)
            .await?);
    }
    Ok(entities::active_connection::Entity::delete_many()
        .filter(filter)
        .exec(db)
        .await?
        .rows_affected)
}
