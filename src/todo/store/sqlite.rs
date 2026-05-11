use std::cell::RefCell;
use std::path::PathBuf;

use chrono::NaiveDate;
use rusqlite::{params, Connection};

use super::error::Result;
use super::Store;
use crate::todo::{Location, Metadata, Todo, TodoIdentifier};

pub struct SqliteStore {
    conn: RefCell<Connection>,
}

const SCHEMA: &str = "PRAGMA foreign_keys = ON;
     CREATE TABLE IF NOT EXISTS todo (
         id              INTEGER PRIMARY KEY AUTOINCREMENT,
         priority        TEXT,
         completion_date TEXT,
         creation_date   TEXT,
         title           TEXT NOT NULL DEFAULT '',
         description     TEXT,
         file_path       TEXT,
         start_line_num  INTEGER NOT NULL DEFAULT 0,
         end_line_num    INTEGER NOT NULL DEFAULT 0
     );
     CREATE TABLE IF NOT EXISTS tag (
         todo_id INTEGER NOT NULL REFERENCES todo(id) ON DELETE CASCADE,
         tag     TEXT NOT NULL
     );
     CREATE TABLE IF NOT EXISTS metadata (
         todo_id INTEGER NOT NULL REFERENCES todo(id) ON DELETE CASCADE,
         key     TEXT NOT NULL,
         value   TEXT NOT NULL
     );
     CREATE TABLE IF NOT EXISTS reference (
         primary_id     INTEGER NOT NULL REFERENCES todo(id) ON DELETE CASCADE,
         title          TEXT NOT NULL DEFAULT '',
         description    TEXT,
         file_path      TEXT,
         start_line_num INTEGER NOT NULL DEFAULT 0,
         end_line_num   INTEGER NOT NULL DEFAULT 0
     );";

impl SqliteStore {
    pub fn new() -> Result<Self> {
        let repo = git2::Repository::open_from_env()?;
        let db_path = repo.commondir().join("todoozy/store.db");
        if let Some(dir) = db_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    fn fetch_todo(&self, id: u32) -> Result<Option<Todo>> {
        let conn = self.conn.borrow();
        let result = conn.query_row(
            "SELECT priority, completion_date, creation_date, title, description,
         file_path, start_line_num, end_line_num FROM todo WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        );

        let (priority, completion_date, creation_date, title, description, file_path, start, end) =
            match result {
                Ok(row) => row,
                Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(e) => return Err(Box::new(e)),
            };

        let tags = {
            let mut stmt = conn.prepare("SELECT tag FROM tag WHERE todo_id = ?1")?;
            let result = stmt
                .query_map(params![id], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            result
        };

        let mut metadata = Metadata::new();
        {
            let mut stmt = conn.prepare("SELECT key, value FROM metadata WHERE todo_id = ?1")?;
            let rows: Vec<(String, String)> = stmt
                .query_map(params![id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for (key, value) in rows {
                metadata.set(&key, &value);
            }
        }

        let references = {
            let mut stmt = conn.prepare(
                "SELECT primary_id, title, description, file_path, start_line_num, end_line_num
             FROM reference WHERE primary_id = ?1",
            )?;
            let rows = stmt
                .query_map(params![id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter()
                .map(
                    |(primary_id, ref_title, ref_desc, ref_file, ref_start, ref_end)| Todo {
                        id: Some(TodoIdentifier::Reference(primary_id as u32)),
                        priority: None,
                        completion_date: None,
                        creation_date: None,
                        title: ref_title,
                        description: ref_desc,
                        tags: Vec::new(),
                        metadata: Metadata::new(),
                        location: Location::new(
                            ref_file.map(PathBuf::from),
                            ref_start as usize,
                            ref_end as usize,
                        ),
                        references: Vec::new(),
                    },
                )
                .collect()
        };

        Ok(Some(Todo {
            id: Some(TodoIdentifier::Primary(id)),
            priority: priority.and_then(|s| s.chars().next()),
            completion_date: completion_date
                .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
            creation_date: creation_date
                .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
            title,
            description,
            tags,
            metadata,
            location: Location::new(file_path.map(PathBuf::from), start as usize, end as usize),
            references,
        }))
    }

    fn insert_todo_data(&self, conn: &Connection, id: u32, todo: &Todo) -> Result<()> {
        for tag in &todo.tags {
            conn.execute(
                "INSERT INTO tag (todo_id, tag) VALUES (?1, ?2)",
                params![id, tag],
            )?;
        }
        for (key, value) in todo.metadata.iter() {
            conn.execute(
                "INSERT INTO metadata (todo_id, key, value) VALUES (?1, ?2, ?3)",
                params![id, key, value],
            )?;
        }
        for reference in &todo.references {
            conn.execute(
                "INSERT INTO reference (primary_id, title, description, file_path,
             start_line_num, end_line_num) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    reference.title,
                    reference.description,
                    reference.location.file_path_string(),
                    reference.location.start_line_num as i64,
                    reference.location.end_line_num as i64,
                ],
            )?;
        }
        Ok(())
    }
}

impl Store for SqliteStore {
    fn get_todo(&self, id: u32) -> Option<Todo> {
        self.fetch_todo(id).ok().flatten()
    }

    fn set_todo(&self, todo: &Todo) -> Result<u32> {
        let mut conn = self.conn.borrow_mut();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO todo (priority, completion_date, creation_date, title, description,
             file_path, start_line_num, end_line_num) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                todo.priority.map(|c| c.to_string()),
                todo.completion_date.map(|d| d.to_string()),
                todo.creation_date.map(|d| d.to_string()),
                todo.title,
                todo.description,
                todo.location.file_path_string(),
                todo.location.start_line_num as i64,
                todo.location.end_line_num as i64,
            ],
        )?;
        let id = tx.last_insert_rowid() as u32;

        self.insert_todo_data(&tx, id, todo)?;

        tx.commit()?;
        Ok(id)
    }

    fn import_todo(&self, id: u32, todo: &Todo) -> Result<()> {
        let mut conn = self.conn.borrow_mut();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO todo (id, priority, completion_date, creation_date, title, description,
             file_path, start_line_num, end_line_num) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                todo.priority.map(|c| c.to_string()),
                todo.completion_date.map(|d| d.to_string()),
                todo.creation_date.map(|d| d.to_string()),
                todo.title,
                todo.description,
                todo.location.file_path_string(),
                todo.location.start_line_num as i64,
                todo.location.end_line_num as i64,
            ],
        )?;

        self.insert_todo_data(&tx, id, todo)?;

        tx.commit()?;
        Ok(())
    }

    fn update_todo(&self, id: u32, todo: Todo) -> Result<()> {
        let mut conn = self.conn.borrow_mut();
        let tx = conn.transaction()?;

        tx.execute(
            "UPDATE todo SET priority = ?1, completion_date = ?2, creation_date = ?3,
             title = ?4, description = ?5, file_path = ?6, start_line_num = ?7,
             end_line_num = ?8 WHERE id = ?9",
            params![
                todo.priority.map(|c| c.to_string()),
                todo.completion_date.map(|d| d.to_string()),
                todo.creation_date.map(|d| d.to_string()),
                todo.title,
                todo.description,
                todo.location.file_path_string(),
                todo.location.start_line_num as i64,
                todo.location.end_line_num as i64,
                id,
            ],
        )?;

        tx.execute("DELETE FROM tag WHERE todo_id = ?1", params![id])?;
        tx.execute("DELETE FROM metadata WHERE todo_id = ?1", params![id])?;
        tx.execute("DELETE FROM reference WHERE primary_id = ?1", params![id])?;

        self.insert_todo_data(&tx, id, &todo)?;

        tx.commit()?;
        Ok(())
    }

    fn get_todos(&self) -> Vec<Todo> {
        let conn = self.conn.borrow();
        let ids: Vec<u32> = {
            let mut stmt = match conn.prepare("SELECT id FROM todo ORDER BY id") {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let result = match stmt.query_map([], |row| row.get::<_, i64>(0)) {
                Ok(rows) => rows.filter_map(|r| r.ok().map(|id| id as u32)).collect(),
                Err(_) => return Vec::new(),
            };
            result
        };

        ids.into_iter()
            .filter_map(|id| self.fetch_todo(id).ok().flatten())
            .collect()
    }

    fn query_todos(&self, _query: &super::Query) -> Vec<Todo> {
        self.get_todos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::TodoBuilder;

    #[test]
    fn get_todo_returns_none_for_missing_id() {
        let store = SqliteStore::in_memory().unwrap();
        assert!(store.get_todo(99).is_none());
    }

    #[test]
    fn set_todo_returns_incrementing_ids() {
        let store = SqliteStore::in_memory().unwrap();
        let id1 = store
            .set_todo(&TodoBuilder::default().title("First".to_string()).build().unwrap())
            .unwrap();
        let id2 = store
            .set_todo(&TodoBuilder::default().title("Second".to_string()).build().unwrap())
            .unwrap();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn set_and_get_todo_roundtrip() {
        let store = SqliteStore::in_memory().unwrap();
        let t = TodoBuilder::default()
            .title("Fix bug".to_string())
            .priority(Some('A'))
            .description(Some("Fix the thing".to_string()))
            .location(Location::new(Some("src/main.rs"), 10, 15))
            .build()
            .unwrap();
        let id = store.set_todo(&t).unwrap();
        let got = store.get_todo(id).unwrap();
        assert_eq!(got.id, Some(TodoIdentifier::Primary(id)));
        assert_eq!(got.title, "Fix bug");
        assert_eq!(got.priority, Some('A'));
        assert_eq!(got.description.as_deref(), Some("Fix the thing"));
        assert_eq!(got.location.start_line_num, 10);
        assert_eq!(got.location.end_line_num, 15);
        assert_eq!(
            got.location.file_path,
            Some(std::path::PathBuf::from("src/main.rs"))
        );
    }

    #[test]
    fn set_todo_persists_tags() {
        let store = SqliteStore::in_memory().unwrap();
        let t = TodoBuilder::default()
            .title("Tagged".to_string())
            .tags(vec!["feat".to_string(), "urgent".to_string()])
            .build()
            .unwrap();
        let id = store.set_todo(&t).unwrap();
        let got = store.get_todo(id).unwrap();
        assert_eq!(got.tags.len(), 2);
        assert!(got.tags.contains(&"feat".to_string()));
        assert!(got.tags.contains(&"urgent".to_string()));
    }

    #[test]
    fn set_todo_persists_metadata_including_multi_value() {
        let store = SqliteStore::in_memory().unwrap();
        let mut meta = Metadata::new();
        meta.set("owner", "alice");
        meta.set("depends", "42");
        meta.set("depends", "43");
        let t = TodoBuilder::default()
            .title("With metadata".to_string())
            .metadata(meta)
            .build()
            .unwrap();
        let id = store.set_todo(&t).unwrap();
        let got = store.get_todo(id).unwrap();
        assert_eq!(
            got.metadata.get("owner"),
            Some(["alice".to_string()].as_slice())
        );
        assert_eq!(
            got.metadata.get("depends"),
            Some(["42".to_string(), "43".to_string()].as_slice())
        );
    }

    #[test]
    fn set_todo_persists_references() {
        let store = SqliteStore::in_memory().unwrap();
        let reference = TodoBuilder::default()
            .title("Reference note".to_string())
            .id(Some(TodoIdentifier::Reference(0)))
            .description(Some("Extra detail".to_string()))
            .location(Location::new(Some("src/lib.rs"), 5, 8))
            .build()
            .unwrap();
        let t = TodoBuilder::default()
            .title("Primary".to_string())
            .references(vec![reference])
            .build()
            .unwrap();
        let id = store.set_todo(&t).unwrap();
        let got = store.get_todo(id).unwrap();
        assert_eq!(got.references.len(), 1);
        assert_eq!(got.references[0].title, "Reference note");
        assert_eq!(
            got.references[0].description.as_deref(),
            Some("Extra detail")
        );
        assert_eq!(got.references[0].location.start_line_num, 5);
        assert_eq!(got.references[0].location.end_line_num, 8);
    }

    #[test]
    fn import_todo_preserves_explicit_id() {
        let store = SqliteStore::in_memory().unwrap();
        let t = TodoBuilder::default().title("Imported".to_string()).build().unwrap();
        store.import_todo(50, &t).unwrap();
        let got = store.get_todo(50).unwrap();
        assert_eq!(got.id, Some(TodoIdentifier::Primary(50)));
        assert_eq!(got.title, "Imported");
    }

    #[test]
    fn import_todo_fails_on_duplicate() {
        let store = SqliteStore::in_memory().unwrap();
        let t = TodoBuilder::default().title("First".to_string()).build().unwrap();
        store.import_todo(50, &t).unwrap();
        let t2 = TodoBuilder::default().title("Second".to_string()).build().unwrap();
        assert!(store.import_todo(50, &t2).is_err());
    }

    #[test]
    fn set_todo_after_import_continues_from_high_water_mark() {
        let store = SqliteStore::in_memory().unwrap();
        let t = TodoBuilder::default().title("Imported".to_string()).build().unwrap();
        store.import_todo(50, &t).unwrap();
        let next_id = store
            .set_todo(&TodoBuilder::default().title("New".to_string()).build().unwrap())
            .unwrap();
        assert_eq!(next_id, 51);
    }

    #[test]
    fn get_todos_returns_all() {
        let store = SqliteStore::in_memory().unwrap();
        store
            .set_todo(&TodoBuilder::default().title("Alpha".to_string()).build().unwrap())
            .unwrap();
        store
            .set_todo(&TodoBuilder::default().title("Beta".to_string()).build().unwrap())
            .unwrap();
        store
            .set_todo(&TodoBuilder::default().title("Gamma".to_string()).build().unwrap())
            .unwrap();
        let todos = store.get_todos();
        assert_eq!(todos.len(), 3);
        let titles: Vec<&str> = todos.iter().map(|t| t.title.as_str()).collect();
        assert!(titles.contains(&"Alpha"));
        assert!(titles.contains(&"Beta"));
        assert!(titles.contains(&"Gamma"));
    }

    #[test]
    fn update_todo_changes_fields() {
        let store = SqliteStore::in_memory().unwrap();
        let id = store
            .set_todo(&TodoBuilder::default().title("Original".to_string()).build().unwrap())
            .unwrap();
        let updated = TodoBuilder::default()
            .title("Updated".to_string())
            .priority(Some('B'))
            .description(Some("Now with description".to_string()))
            .build()
            .unwrap();
        store.update_todo(id, updated).unwrap();
        let got = store.get_todo(id).unwrap();
        assert_eq!(got.title, "Updated");
        assert_eq!(got.priority, Some('B'));
        assert_eq!(got.description.as_deref(), Some("Now with description"));
    }

    #[test]
    fn update_todo_replaces_tags() {
        let store = SqliteStore::in_memory().unwrap();
        let id = store
            .set_todo(
                &TodoBuilder::default()
                    .title("Tagged".to_string())
                    .tags(vec!["old".to_string()])
                    .build()
                    .unwrap(),
            )
            .unwrap();
        store
            .update_todo(
                id,
                TodoBuilder::default()
                    .title("Tagged".to_string())
                    .tags(vec!["new".to_string()])
                    .build()
                    .unwrap(),
            )
            .unwrap();
        let got = store.get_todo(id).unwrap();
        assert_eq!(got.tags, vec!["new".to_string()]);
    }

    #[test]
    fn update_todo_replaces_references() {
        let store = SqliteStore::in_memory().unwrap();
        let ref1 = TodoBuilder::default()
            .title("Old ref".to_string())
            .id(Some(TodoIdentifier::Reference(0)))
            .build()
            .unwrap();
        let id = store
            .set_todo(
                &TodoBuilder::default()
                    .title("Primary".to_string())
                    .references(vec![ref1])
                    .build()
                    .unwrap(),
            )
            .unwrap();

        let ref2 = TodoBuilder::default()
            .title("New ref".to_string())
            .id(Some(TodoIdentifier::Reference(0)))
            .build()
            .unwrap();
        store
            .update_todo(
                id,
                TodoBuilder::default()
                    .title("Primary".to_string())
                    .references(vec![ref2])
                    .build()
                    .unwrap(),
            )
            .unwrap();

        let got = store.get_todo(id).unwrap();
        assert_eq!(got.references.len(), 1);
        assert_eq!(got.references[0].title, "New ref");
    }
}
