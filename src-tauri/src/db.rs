use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub source: String,
    pub content: String,
    pub subscription_url: Option<String>,
    pub auto_update: bool,
    pub updated_at: i64,
    pub created_at: i64,
    pub last_error: Option<String>,
    pub size: usize,
}

pub struct Db {
    conn: Mutex<Connection>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn row_to_profile(row: &Row<'_>) -> rusqlite::Result<Profile> {
    let content: String = row.get("content")?;
    Ok(Profile {
        id: row.get("id")?,
        name: row.get("name")?,
        kind: row.get("kind")?,
        source: row.get("source")?,
        size: content.len(),
        content,
        subscription_url: row.get("subscription_url")?,
        auto_update: row.get::<_, i64>("auto_update")? != 0,
        updated_at: row.get("updated_at")?,
        created_at: row.get("created_at")?,
        last_error: row.get("last_error")?,
    })
}

const PROFILE_COLS: &str = "id, name, kind, source, content, subscription_url, auto_update, updated_at, created_at, last_error";

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create data dir: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("open database: {e}"))?;
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| e.to_string())?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        {
            use std::os::unix::fs::PermissionsExt;
            for suffix in ["-wal", "-shm"] {
                let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
                if sidecar.exists() {
                    let _ = std::fs::set_permissions(&sidecar, std::fs::Permissions::from_mode(0o600));
                }
            }
        }
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS profiles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'local',
                source TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                subscription_url TEXT,
                auto_update INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                last_error TEXT
            );
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS selections (
                profile_id INTEGER NOT NULL,
                group_tag TEXT NOT NULL,
                outbound_tag TEXT NOT NULL,
                PRIMARY KEY (profile_id, group_tag)
            );
            CREATE INDEX IF NOT EXISTS idx_profiles_kind ON profiles(kind);
            "#,
        )
        .map_err(|e| format!("migrate database: {e}"))?;
        Ok(())
    }

    pub fn list_profiles(&self) -> Result<Vec<Profile>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {PROFILE_COLS} FROM profiles ORDER BY created_at DESC, id DESC"
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], row_to_profile)
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())
    }

    pub fn get_profile(&self, id: i64) -> Result<Option<Profile>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(&format!("SELECT {PROFILE_COLS} FROM profiles WHERE id = ?1"))
            .map_err(|e| e.to_string())?;
        stmt.query_row(params![id], row_to_profile)
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn create_profile(
        &self,
        name: &str,
        kind: &str,
        content: &str,
        source: &str,
        subscription_url: Option<&str>,
        auto_update: bool,
    ) -> Result<Profile, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("profile name must not be empty".into());
        }
        let ts = now();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO profiles (name, kind, source, content, subscription_url, auto_update, updated_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![name, kind, source, content, subscription_url, auto_update as i64, ts],
        )
        .map_err(|e| format!("create profile: {e}"))?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.get_profile(id)?
            .ok_or_else(|| "profile vanished after insert".to_string())
    }

    pub fn update_profile(
        &self,
        id: i64,
        name: &str,
        content: Option<&str>,
    ) -> Result<Profile, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("profile name must not be empty".into());
        }
        let conn = self.conn.lock().unwrap();
        let changed = match content {
            Some(content) => conn.execute(
                "UPDATE profiles SET name = ?1, content = ?2, updated_at = ?3, last_error = NULL WHERE id = ?4",
                params![name, content, now(), id],
            ),
            None => conn.execute(
                "UPDATE profiles SET name = ?1 WHERE id = ?2",
                params![name, id],
            ),
        }
        .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err(format!("profile {id} not found"));
        }
        drop(conn);
        self.get_profile(id)?
            .ok_or_else(|| "profile not found".to_string())
    }

    pub fn set_profile_auto_update(&self, id: i64, enabled: bool) -> Result<Profile, String> {
        let conn = self.conn.lock().unwrap();
        let changed = conn
            .execute(
                "UPDATE profiles SET auto_update = ?1 WHERE id = ?2",
                params![enabled as i64, id],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err(format!("profile {id} not found"));
        }
        drop(conn);
        self.get_profile(id)?
            .ok_or_else(|| "profile not found".to_string())
    }

    pub fn delete_profile(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM profiles WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM selections WHERE profile_id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn duplicate_profile(&self, id: i64, name: &str) -> Result<Profile, String> {
        let source = self
            .get_profile(id)?
            .ok_or_else(|| format!("profile {id} not found"))?;
        self.create_profile(
            name,
            &source.kind,
            &source.content,
            &source.source,
            source.subscription_url.as_deref(),
            false,
        )
    }

    pub fn set_subscription_result(
        &self,
        id: i64,
        content: Option<&str>,
        last_error: Option<&str>,
    ) -> Result<Profile, String> {
        let conn = self.conn.lock().unwrap();
        let ts = now();
        let changed = match content {
            Some(content) => conn.execute(
                "UPDATE profiles SET content = ?1, updated_at = ?2, last_error = NULL WHERE id = ?3",
                params![content, ts, id],
            ),
            None => conn.execute(
                "UPDATE profiles SET last_error = ?1 WHERE id = ?2",
                params![last_error, id],
            ),
        }
        .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err(format!("profile {id} not found"));
        }
        drop(conn);
        self.get_profile(id)?
            .ok_or_else(|| "profile not found".to_string())
    }

    pub fn due_subscriptions(&self, interval_hours: u32) -> Result<Vec<Profile>, String> {
        if interval_hours == 0 {
            return Ok(Vec::new());
        }
        let cutoff = now() - (interval_hours as i64) * 3600;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {PROFILE_COLS} FROM profiles WHERE kind = 'subscription' AND auto_update = 1 AND subscription_url IS NOT NULL AND updated_at <= ?1 ORDER BY updated_at ASC"
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![cutoff], row_to_profile)
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn last_profile_id(&self) -> Option<i64> {
        self.get_meta("last_profile_id")
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
    }

    pub fn set_last_profile_id(&self, id: i64) {
        let _ = self.set_meta("last_profile_id", &id.to_string());
    }

    pub fn selections(&self, profile_id: i64) -> Result<Vec<(String, String)>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT group_tag, outbound_tag FROM selections WHERE profile_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())
    }

    pub fn set_selection(
        &self,
        profile_id: i64,
        group_tag: &str,
        outbound_tag: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO selections (profile_id, group_tag, outbound_tag) VALUES (?1, ?2, ?3)
             ON CONFLICT(profile_id, group_tag) DO UPDATE SET outbound_tag = excluded.outbound_tag",
            params![profile_id, group_tag, outbound_tag],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn export_backup(&self) -> Result<Value, String> {
        let profiles = self.list_profiles()?;
        Ok(json!({
            "format": "veil-backup",
            "version": 1,
            "exportedAt": now(),
            "profiles": profiles
        }))
    }

    pub fn import_backup(&self, raw: &str) -> Result<usize, String> {
        let value: Value =
            serde_json::from_str(raw).map_err(|e| format!("invalid backup JSON: {e}"))?;
        let profiles = value
            .get("profiles")
            .and_then(Value::as_array)
            .or_else(|| value.as_array())
            .ok_or_else(|| "backup must contain a profiles array".to_string())?;
        let mut count = 0;
        let ts = now();
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        for item in profiles {
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if name.is_empty() {
                return Err("backup contains a profile without a name".into());
            }
            let content = item
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("profile '{name}' has no content"))?;
            let parsed: Value = serde_json::from_str(content)
                .map_err(|e| format!("profile '{name}' content is not JSON: {e}"))?;
            if !parsed.is_object() {
                return Err(format!("profile '{name}' content must be a JSON object"));
            }
            let kind = item.get("kind").and_then(Value::as_str).unwrap_or("local");
            let source = item.get("source").and_then(Value::as_str).unwrap_or("");
            let sub = item.get("subscriptionUrl").and_then(Value::as_str);
            let auto = item
                .get("autoUpdate")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let created = item
                .get("createdAt")
                .and_then(Value::as_i64)
                .unwrap_or(ts);
            let updated = item
                .get("updatedAt")
                .and_then(Value::as_i64)
                .unwrap_or(ts);
            tx.execute(
                "INSERT INTO profiles (name, kind, source, content, subscription_url, auto_update, updated_at, created_at, last_error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                params![name, kind, source, content, sub, auto as i64, updated, created],
            )
            .map_err(|e| format!("import profile '{name}': {e}"))?;
            count += 1;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"outbounds":[{"type":"direct","tag":"direct"}]}"#;

    #[test]
    fn crud_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.list_profiles().unwrap().is_empty());
        let a = db
            .create_profile("Alpha", "local", SAMPLE, "test", None, false)
            .unwrap();
        assert_eq!(a.kind, "local");
        assert_eq!(a.size, SAMPLE.len());
        assert!(a.created_at > 0);
        let list = db.list_profiles().unwrap();
        assert_eq!(list.len(), 1);

        let updated = db.update_profile(a.id, "Alpha 2", Some(SAMPLE)).unwrap();
        assert_eq!(updated.name, "Alpha 2");

        let dup = db.duplicate_profile(a.id, "Copy").unwrap();
        assert_ne!(dup.id, a.id);
        assert_eq!(dup.content, SAMPLE);
        assert!(!dup.auto_update);

        db.set_selection(a.id, "Proxy", "node-a").unwrap();
        db.set_selection(a.id, "Proxy", "node-b").unwrap();
        assert_eq!(
            db.selections(a.id).unwrap(),
            vec![("Proxy".to_string(), "node-b".to_string())]
        );

        db.delete_profile(a.id).unwrap();
        assert_eq!(db.list_profiles().unwrap().len(), 1);
        assert!(db.selections(a.id).unwrap().is_empty());
    }

    #[test]
    fn auto_update_toggle_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let p = db
            .create_profile(
                "Sub",
                "subscription",
                SAMPLE,
                "https://example.com/sub",
                Some("https://example.com/sub"),
                true,
            )
            .unwrap();
        assert!(p.auto_update);

        let off = db.set_profile_auto_update(p.id, false).unwrap();
        assert!(!off.auto_update);
        assert!(!db.get_profile(p.id).unwrap().unwrap().auto_update);

        let on = db.set_profile_auto_update(p.id, true).unwrap();
        assert!(on.auto_update);
        assert!(db.get_profile(p.id).unwrap().unwrap().auto_update);

        assert!(db.set_profile_auto_update(999_999, true).is_err());
    }

    #[test]
    fn subscription_due_and_errors() {
        let db = Db::open_in_memory().unwrap();
        let p = db
            .create_profile(
                "Sub",
                "subscription",
                SAMPLE,
                "https://example.com/sub",
                Some("https://example.com/sub"),
                true,
            )
            .unwrap();
        assert_eq!(db.due_subscriptions(24).unwrap().len(), 0);
        db.set_subscription_result(p.id, None, Some("network down"))
            .unwrap();
        let refreshed = db.get_profile(p.id).unwrap().unwrap();
        assert_eq!(refreshed.last_error.as_deref(), Some("network down"));
        db.set_subscription_result(p.id, Some(SAMPLE), None).unwrap();
        assert!(db.get_profile(p.id).unwrap().unwrap().last_error.is_none());
        // Force due by moving updated_at into the past.
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "UPDATE profiles SET updated_at = ?1 WHERE id = ?2",
                params![now() - 100_000, p.id],
            )
            .unwrap();
        }
        assert_eq!(db.due_subscriptions(24).unwrap().len(), 1);
        assert_eq!(db.due_subscriptions(0).unwrap().len(), 0);
    }

    #[test]
    fn backup_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        db.create_profile("One", "local", SAMPLE, "src", None, false)
            .unwrap();
        let backup = db.export_backup().unwrap().to_string();
        let db2 = Db::open_in_memory().unwrap();
        assert_eq!(db2.import_backup(&backup).unwrap(), 1);
        assert_eq!(db2.list_profiles().unwrap()[0].name, "One");
        assert!(db2.import_backup("{\"profiles\":[{\"name\":\"x\",\"content\":\"nope\"}]}").is_err());
    }

    #[test]
    fn meta_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.last_profile_id().is_none());
        db.set_last_profile_id(42);
        assert_eq!(db.last_profile_id(), Some(42));
    }
}
