use chrono::{DateTime, Utc};
use deeplocal_core::{ChatMessage, ChatRole, ChatSession, GenerationParameters, ModelDescriptor};
use rusqlite::{Connection, params};
use std::path::Path;
use uuid::Uuid;

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        let storage = Self { conn };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn open_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self { conn };
        storage.migrate()?;
        Ok(storage)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "
            create table if not exists models (
                id text primary key,
                descriptor_json text not null,
                updated_at text not null
            );
            create table if not exists chat_sessions (
                id text primary key,
                title text not null,
                model_id text,
                parameters_json text not null,
                created_at text not null,
                updated_at text not null
            );
            create table if not exists chat_messages (
                id text primary key,
                session_id text not null,
                role text not null,
                content text not null,
                metadata_json text not null,
                created_at text not null
            );
            create table if not exists benchmarks (
                id text primary key,
                model_id text not null,
                backend text not null,
                hardware_fingerprint text not null,
                metrics_json text not null,
                created_at text not null
            );
            ",
        )?;
        Ok(())
    }

    pub fn upsert_model(&self, model: &ModelDescriptor) -> anyhow::Result<()> {
        self.conn.execute(
            "insert into models (id, descriptor_json, updated_at) values (?1, ?2, ?3)
             on conflict(id) do update set descriptor_json = excluded.descriptor_json, updated_at = excluded.updated_at",
            params![model.id, serde_json::to_string(model)?, model.updated_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_models(&self) -> anyhow::Result<Vec<ModelDescriptor>> {
        let mut stmt = self
            .conn
            .prepare("select descriptor_json from models order by updated_at desc")?;
        let rows = stmt.query_map([], |row| {
            let json: String = row.get(0)?;
            Ok(json)
        })?;

        let mut models = Vec::new();
        for row in rows {
            models.push(serde_json::from_str(&row?)?);
        }
        Ok(models)
    }

    pub fn create_chat_session(
        &self,
        title: impl Into<String>,
        model_id: Option<String>,
    ) -> anyhow::Result<ChatSession> {
        let now = Utc::now();
        let session = ChatSession {
            id: Uuid::new_v4(),
            title: title.into(),
            model_id,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            "insert into chat_sessions (id, title, model_id, parameters_json, created_at, updated_at)
             values (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.id.to_string(),
                session.title,
                session.model_id,
                serde_json::to_string(&GenerationParameters::default())?,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
            ],
        )?;
        self.get_chat_session(session.id)
    }

    pub fn list_chat_sessions(&self) -> anyhow::Result<Vec<ChatSession>> {
        let mut stmt = self.conn.prepare(
            "select id, title, model_id, created_at, updated_at from chat_sessions order by updated_at desc",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ChatSession {
                id: parse_uuid(row.get::<_, String>(0)?)?,
                title: row.get(1)?,
                model_id: row.get(2)?,
                messages: Vec::new(),
                created_at: parse_datetime(row.get::<_, String>(3)?)?,
                updated_at: parse_datetime(row.get::<_, String>(4)?)?,
            })
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            let mut session = row?;
            session.messages = self.list_chat_messages(session.id)?;
            sessions.push(session);
        }
        Ok(sessions)
    }

    pub fn get_chat_session(&self, id: Uuid) -> anyhow::Result<ChatSession> {
        let mut stmt = self.conn.prepare(
            "select id, title, model_id, created_at, updated_at from chat_sessions where id = ?1",
        )?;
        let mut session = stmt.query_row(params![id.to_string()], |row| {
            Ok(ChatSession {
                id: parse_uuid(row.get::<_, String>(0)?)?,
                title: row.get(1)?,
                model_id: row.get(2)?,
                messages: Vec::new(),
                created_at: parse_datetime(row.get::<_, String>(3)?)?,
                updated_at: parse_datetime(row.get::<_, String>(4)?)?,
            })
        })?;
        session.messages = self.list_chat_messages(id)?;
        Ok(session)
    }

    pub fn rename_chat_session(&self, id: Uuid, title: impl Into<String>) -> anyhow::Result<()> {
        self.conn.execute(
            "update chat_sessions set title = ?1, updated_at = ?2 where id = ?3",
            params![title.into(), Utc::now().to_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }

    pub fn update_chat_session_model(
        &self,
        id: Uuid,
        model_id: Option<String>,
    ) -> anyhow::Result<()> {
        self.conn.execute(
            "update chat_sessions set model_id = ?1, updated_at = ?2 where id = ?3",
            params![model_id, Utc::now().to_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }

    pub fn delete_chat_session(&self, id: Uuid) -> anyhow::Result<()> {
        self.conn.execute(
            "delete from chat_messages where session_id = ?1",
            params![id.to_string()],
        )?;
        self.conn.execute(
            "delete from chat_sessions where id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    pub fn append_chat_message(
        &self,
        session_id: Uuid,
        role: ChatRole,
        content: impl Into<String>,
    ) -> anyhow::Result<ChatMessage> {
        let message = ChatMessage::new(role, content);
        self.conn.execute(
            "insert into chat_messages (id, session_id, role, content, metadata_json, created_at)
             values (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                message.id.to_string(),
                session_id.to_string(),
                serde_json::to_string(&message.role)?,
                message.content,
                "{}",
                message.created_at.to_rfc3339(),
            ],
        )?;
        self.conn.execute(
            "update chat_sessions set updated_at = ?1 where id = ?2",
            params![Utc::now().to_rfc3339(), session_id.to_string()],
        )?;
        Ok(message)
    }

    fn list_chat_messages(&self, session_id: Uuid) -> anyhow::Result<Vec<ChatMessage>> {
        let mut stmt = self.conn.prepare(
            "select id, role, content, created_at from chat_messages where session_id = ?1 order by created_at asc",
        )?;
        let rows = stmt.query_map(params![session_id.to_string()], |row| {
            let role_json: String = row.get(1)?;
            Ok(ChatMessage {
                id: parse_uuid(row.get::<_, String>(0)?)?,
                role: serde_json::from_str(&role_json).unwrap_or(ChatRole::User),
                content: row.get(2)?,
                created_at: parse_datetime(row.get::<_, String>(3)?)?,
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }
}

fn parse_uuid(value: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn parse_datetime(value: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                value.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
}
