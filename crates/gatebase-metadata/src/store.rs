use crate::entities;
use crate::mapping::{model_to_access_approval, model_to_active_connection, model_to_session};
use anyhow::{Context, Result};
use gatebase_core::{AccessApproval, ActiveConnection, AuditEvent, Session, SessionId};
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection,
    EntityTrait, QueryFilter, Schema, Set, Statement,
};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct MetadataStore {
    db: DatabaseConnection,
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
        self.create_table(entities::active_connection::Entity)
            .await?;
        self.create_table(entities::access_approval::Entity).await?;
        Ok(())
    }

    pub async fn create_access_approval(&self, approval: &AccessApproval) -> Result<()> {
        entities::access_approval::ActiveModel {
            id: Set(approval.id.clone()),
            repo: Set(approval.repo.clone()),
            pull_request: Set(approval.pull_request),
            target: Set(approval.target.clone()),
            actor: Set(approval.actor.clone()),
            approver: Set(approval.approver.clone()),
            reason: Set(approval.reason.clone()),
            created_at: Set(approval.created_at.to_rfc3339()),
            expires_at: Set(approval.expires_at.map(|time| time.to_rfc3339())),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn find_active_access_approval(
        &self,
        repo: &str,
        pull_request: Option<i64>,
        target: &str,
        actor: &str,
        approvers: &[String],
        now: String,
    ) -> Result<Option<AccessApproval>> {
        let mut query = entities::access_approval::Entity::find()
            .filter(entities::access_approval::Column::Repo.eq(repo))
            .filter(entities::access_approval::Column::Target.eq(target))
            .filter(
                entities::access_approval::Column::Actor
                    .is_null()
                    .or(entities::access_approval::Column::Actor.eq(actor)),
            )
            .filter(
                entities::access_approval::Column::ExpiresAt
                    .is_null()
                    .or(entities::access_approval::Column::ExpiresAt.gt(now)),
            );
        query = match pull_request {
            Some(pull_request) => {
                query.filter(entities::access_approval::Column::PullRequest.eq(pull_request))
            }
            None => query.filter(entities::access_approval::Column::PullRequest.is_null()),
        };
        if !approvers.is_empty() {
            query = query.filter(entities::access_approval::Column::Approver.is_in(approvers));
        }
        query
            .one(&self.db)
            .await?
            .map(model_to_access_approval)
            .transpose()
    }

    pub async fn create_session(&self, session: &Session) -> Result<()> {
        entities::session::ActiveModel {
            id: Set(session.id.to_string()),
            actor: Set(session.actor.clone()),
            github_repo: Set(session.github_repo.clone()),
            pull_request: Set(session.pull_request),
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

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        entities::session::Entity::find()
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

    pub async fn list_active_connections(&self) -> Result<Vec<ActiveConnection>> {
        entities::active_connection::Entity::find()
            .filter(entities::active_connection::Column::DisconnectedAt.is_null())
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
        }
        .insert(&self.db)
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
