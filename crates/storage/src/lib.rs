use deeplocal_core::ModelDescriptor;
use rusqlite::{Connection, params};
use std::path::Path;

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
}
