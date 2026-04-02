// Cache layer for VCS TODO history
//
// This module provides a file-based caching wrapper for VCS backends.
// Cache is stored in `.tdz/cache/` directory, which is gitignored local state.

// use super::{
//     error::{Error, Result},
//     EventType, TodoEvent, TodoLifecycle, VcsBackend,
// };
// use chrono::DateTime;
// use serde::{Deserialize, Serialize};
// use std::collections::HashMap;
// use std::fs;
// use std::path::{Path, PathBuf};
//
// const CACHE_DIR: &str = ".tdz/cache";
// const EVENTS_FILE: &str = "todo_events.json";
// const LAST_COMMIT_FILE: &str = "last_commit.sha";
//
// /// Serializable form of TodoContent for JSON storage.
// #[derive(Debug, Clone, Serialize, Deserialize)]
// struct SerializedContent {
//     raw_text: String,
//     title: String,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     description: Option<String>,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     priority: Option<char>,
//     #[serde(skip_serializing_if = "Vec::is_empty", default)]
//     tags: Vec<String>,
//     #[serde(skip_serializing_if = "HashMap::is_empty", default)]
//     metadata: HashMap<String, Vec<String>>,
// }
//
// /// Serializable form of TodoEvent for JSON storage.
// #[derive(Debug, Clone, Serialize, Deserialize)]
// struct SerializedEvent {
//     id: u32,
//     event_type: String,
//     commit_sha: String,
//     timestamp: i64,
//     author_name: String,
//     author_email: String,
//     file_path: String,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     content: Option<SerializedContent>,
// }
//
// impl From<&TodoContent> for SerializedContent {
//     fn from(content: &TodoContent) -> Self {
//         SerializedContent {
//             raw_text: content.raw_text.clone(),
//             title: content.title.clone(),
//             description: content.description.clone(),
//             priority: content.priority,
//             tags: content.tags.clone(),
//             metadata: content
//                 .metadata
//                 .iter()
//                 .fold(HashMap::new(), |mut acc, (k, v)| {
//                     acc.entry(k.clone()).or_default().push(v.clone());
//                     acc
//                 }),
//         }
//     }
// }
//
// impl From<&TodoEvent> for SerializedEvent {
//     fn from(event: &TodoEvent) -> Self {
//         SerializedEvent {
//             id: event.id,
//             event_type: event.event_type.to_string(),
//             commit_sha: event.commit_sha.clone(),
//             timestamp: event.timestamp.timestamp(),
//             author_name: event.author_name.clone(),
//             author_email: event.author_email.clone(),
//             file_path: event.file_path.clone(),
//             content: event.todo.as_ref().map(SerializedContent::from),
//         }
//     }
// }
//
// impl From<SerializedContent> for TodoContent {
//     fn from(s: SerializedContent) -> Self {
//         TodoContent {
//             raw_text: s.raw_text,
//             title: s.title,
//             description: s.description,
//             priority: s.priority,
//             tags: s.tags,
//             metadata: s
//                 .metadata
//                 .into_iter()
//                 .flat_map(|(k, vs)| vs.into_iter().map(move |v| (k.clone(), v)))
//                 .collect(),
//         }
//     }
// }
//
// impl TryFrom<SerializedEvent> for TodoEvent {
//     type Error = Error;
//
//     fn try_from(s: SerializedEvent) -> Result<Self> {
//         let event_type = match s.event_type.as_str() {
//             "created" => EventType::Created,
//             "removed" => EventType::Removed,
//             other => return Err(Error::ParseError(format!("unknown event type: {}", other))),
//         };
//
//         let timestamp = DateTime::from_timestamp(s.timestamp, 0)
//             .ok_or_else(|| Error::ParseError("invalid timestamp".to_string()))?;
//
//         Ok(TodoEvent {
//             id: s.id,
//             event_type,
//             commit_sha: s.commit_sha,
//             timestamp,
//             author_name: s.author_name,
//             author_email: s.author_email,
//             file_path: s.file_path,
//             todo: s.content.map(TodoContent::from),
//         })
//     }
// }
//
// /// Cache data structure stored in JSON.
// #[derive(Debug, Serialize, Deserialize)]
// struct CacheData {
//     events: Vec<SerializedEvent>,
// }
//
// /// A caching wrapper around a VCS backend.
// ///
// /// This wrapper stores TODO events in `.tdz/cache/todo_events.json` to avoid
// /// rescanning the entire git history on every lookup.
// pub struct CachedVcsBackend<B: VcsBackend> {
//     inner: B,
//     cache_dir: PathBuf,
//     cached_events: Option<Vec<TodoEvent>>,
// }
//
// impl<B: VcsBackend> CachedVcsBackend<B> {
//     /// Create a new cached backend wrapper.
//     ///
//     /// The `repo_root` should be the root directory of the repository,
//     /// where `.tdz/cache/` will be created.
//     pub fn new(inner: B, repo_root: &Path) -> Self {
//         let cache_dir = repo_root.join(CACHE_DIR);
//         CachedVcsBackend {
//             inner,
//             cache_dir,
//             cached_events: None,
//         }
//     }
//
//     /// Load events from cache file.
//     fn load_cache(&self) -> Result<Option<Vec<TodoEvent>>> {
//         let events_path = self.cache_dir.join(EVENTS_FILE);
//
//         if !events_path.exists() {
//             return Ok(None);
//         }
//
//         let content = fs::read_to_string(&events_path)
//             .map_err(|e| Error::CacheError(format!("failed to read cache: {}", e)))?;
//
//         let cache_data: CacheData = serde_json::from_str(&content)
//             .map_err(|e| Error::CacheError(format!("failed to parse cache: {}", e)))?;
//
//         let events: Result<Vec<TodoEvent>> = cache_data
//             .events
//             .into_iter()
//             .map(TodoEvent::try_from)
//             .collect();
//
//         Ok(Some(events?))
//     }
//
//     /// Save events to cache file.
//     fn save_cache(&self, events: &[TodoEvent]) -> Result<()> {
//         // Ensure cache directory exists
//         fs::create_dir_all(&self.cache_dir)
//             .map_err(|e| Error::CacheError(format!("failed to create cache dir: {}", e)))?;
//
//         let cache_data = CacheData {
//             events: events.iter().map(SerializedEvent::from).collect(),
//         };
//
//         let content = serde_json::to_string_pretty(&cache_data)
//             .map_err(|e| Error::CacheError(format!("failed to serialize cache: {}", e)))?;
//
//         let events_path = self.cache_dir.join(EVENTS_FILE);
//         fs::write(&events_path, content)
//             .map_err(|e| Error::CacheError(format!("failed to write cache: {}", e)))?;
//
//         Ok(())
//     }
//
//     /// Get the last cached commit SHA if available.
//     pub fn get_last_cached_commit(&self) -> Option<String> {
//         let sha_path = self.cache_dir.join(LAST_COMMIT_FILE);
//         fs::read_to_string(&sha_path)
//             .ok()
//             .map(|s| s.trim().to_string())
//     }
//
//     /// Save the last cached commit SHA.
//     pub fn set_last_cached_commit(&self, sha: &str) -> Result<()> {
//         fs::create_dir_all(&self.cache_dir)
//             .map_err(|e| Error::CacheError(format!("failed to create cache dir: {}", e)))?;
//
//         let sha_path = self.cache_dir.join(LAST_COMMIT_FILE);
//         fs::write(&sha_path, sha)
//             .map_err(|e| Error::CacheError(format!("failed to write last commit: {}", e)))?;
//
//         Ok(())
//     }
//
//     /// Build or rebuild the cache from the VCS backend.
//     ///
//     /// This scans all TODO events from the VCS and saves them to the cache file.
//     /// Used by `tdz cache build` command (TODO #67).
//     pub fn build_cache(&mut self) -> Result<()> {
//         let events = self.inner.scan_all_todo_events()?;
//         self.save_cache(&events)?;
//         self.cached_events = Some(events);
//         Ok(())
//     }
//
//     /// Clear the cache files.
//     pub fn clear_cache(&self) -> Result<()> {
//         let events_path = self.cache_dir.join(EVENTS_FILE);
//         let sha_path = self.cache_dir.join(LAST_COMMIT_FILE);
//
//         if events_path.exists() {
//             fs::remove_file(&events_path)
//                 .map_err(|e| Error::CacheError(format!("failed to remove cache: {}", e)))?;
//         }
//
//         if sha_path.exists() {
//             fs::remove_file(&sha_path)
//                 .map_err(|e| Error::CacheError(format!("failed to remove commit file: {}", e)))?;
//         }
//
//         Ok(())
//     }
//
//     /// Build lifecycles from a list of events.
//     fn build_lifecycles(events: &[TodoEvent]) -> HashMap<u32, TodoLifecycle> {
//         let mut lifecycles: HashMap<u32, TodoLifecycle> = HashMap::new();
//
//         for event in events {
//             let lifecycle = lifecycles.entry(event.id).or_insert_with(|| TodoLifecycle {
//                 id: event.id,
//                 created: None,
//                 removed: None,
//             });
//
//             match event.event_type {
//                 EventType::Created => {
//                     // Keep the earliest creation event
//                     if lifecycle.created.is_none()
//                         || event.timestamp < lifecycle.created.as_ref().unwrap().timestamp
//                     {
//                         lifecycle.created = Some(event.clone());
//                     }
//                 }
//                 EventType::Removed => {
//                     // Keep the latest removal event
//                     if lifecycle.removed.is_none()
//                         || event.timestamp > lifecycle.removed.as_ref().unwrap().timestamp
//                     {
//                         lifecycle.removed = Some(event.clone());
//                     }
//                 }
//             }
//         }
//
//         lifecycles
//     }
// }
//
// impl<B: VcsBackend> VcsBackend for CachedVcsBackend<B> {
//     fn get_todo_lifecycle(&self, id: u32) -> Result<Option<TodoLifecycle>> {
//         // For lookups, we can't use self.get_events() since it takes &mut self
//         // Try cache first, fall back to inner backend
//         if let Some(events) = self.load_cache()? {
//             let lifecycles = Self::build_lifecycles(&events);
//             Ok(lifecycles.get(&id).cloned())
//         } else {
//             self.inner.get_todo_lifecycle(id)
//         }
//     }
//
//     fn scan_all_todo_events(&self) -> Result<Vec<TodoEvent>> {
//         // If we have cache, return it; otherwise delegate to inner
//         if let Some(events) = self.load_cache()? {
//             Ok(events)
//         } else {
//             self.inner.scan_all_todo_events()
//         }
//     }
//
//     fn get_all_historical_ids(&self) -> Result<Vec<u32>> {
//         if let Some(events) = self.load_cache()? {
//             let lifecycles = Self::build_lifecycles(&events);
//             let mut ids: Vec<u32> = lifecycles.keys().copied().collect();
//             ids.sort();
//             Ok(ids)
//         } else {
//             self.inner.get_all_historical_ids()
//         }
//     }
//
//     fn get_max_historical_id(&self) -> Result<u32> {
//         let ids = self.get_all_historical_ids()?;
//         Ok(ids.into_iter().max().unwrap_or(0))
//     }
// }
//
// /// Create a cached VCS backend for the given repository path.
// ///
// /// This wraps the standard VCS backend with a caching layer that stores
// /// `TODO` events in `.tdz/cache/`.
// pub fn create_cached_backend(repo_path: &Path) -> Result<CachedVcsBackend<impl VcsBackend>> {
//     let inner = super::git::GitBackend::new(repo_path)?;
//     Ok(CachedVcsBackend::new(inner, repo_path))
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::todo::Metadata;
//     use chrono::Utc;
//     use std::fs;
//     use std::process::Command;
//     use tempfile::TempDir;
//
//     fn create_test_repo() -> TempDir {
//         let dir = TempDir::new().expect("failed to create temp dir");
//
//         Command::new("git")
//             .args(["init"])
//             .current_dir(dir.path())
//             .output()
//             .expect("failed to init repo");
//
//         Command::new("git")
//             .args(["config", "user.email", "test@example.com"])
//             .current_dir(dir.path())
//             .output()
//             .expect("failed to set email");
//
//         Command::new("git")
//             .args(["config", "user.name", "Test User"])
//             .current_dir(dir.path())
//             .output()
//             .expect("failed to set name");
//
//         Command::new("git")
//             .args(["config", "commit.gpgsign", "false"])
//             .current_dir(dir.path())
//             .output()
//             .expect("failed to disable gpg signing");
//
//         dir
//     }
//
//     fn commit_file(dir: &Path, filename: &str, content: &str, message: &str) {
//         let file_path = dir.join(filename);
//         fs::write(&file_path, content).expect("failed to write file");
//
//         Command::new("git")
//             .args(["add", filename])
//             .current_dir(dir)
//             .output()
//             .expect("failed to add file");
//
//         Command::new("git")
//             .args(["commit", "-m", message])
//             .current_dir(dir)
//             .output()
//             .expect("failed to commit");
//     }
//
//     #[test]
//     fn test_serialized_event_roundtrip() {
//         let event = TodoEvent {
//             id: 42,
//             event_type: EventType::Created,
//             commit_sha: "abc123".to_string(),
//             timestamp: Utc::now(),
//             author_name: "Test".to_string(),
//             author_email: "test@test.com".to_string(),
//             file_path: "test.rs".to_string(),
//             todo: None,
//         };
//
//         let serialized = SerializedEvent::from(&event);
//         let deserialized = TodoEvent::try_from(serialized).expect("failed to deserialize");
//
//         assert_eq!(event.id, deserialized.id);
//         assert_eq!(event.event_type, deserialized.event_type);
//         assert_eq!(event.commit_sha, deserialized.commit_sha);
//         assert_eq!(event.author_name, deserialized.author_name);
//         assert_eq!(event.author_email, deserialized.author_email);
//         assert_eq!(event.file_path, deserialized.file_path);
//         assert!(deserialized.todo.is_none());
//     }
//
//     #[test]
//     fn test_serialized_event_with_content_roundtrip() {
//         let mut metadata = Metadata::new();
//         metadata.set("owner", "alice");
//
//         let content = TodoContent {
//             raw_text: "// TODO #42 Fix bug".to_string(),
//             title: "Fix bug".to_string(),
//             description: Some("Detailed description".to_string()),
//             priority: Some('A'),
//             tags: vec!["urgent".to_string()],
//             metadata,
//         };
//
//         let event = TodoEvent {
//             id: 42,
//             event_type: EventType::Created,
//             commit_sha: "abc123".to_string(),
//             timestamp: Utc::now(),
//             author_name: "Test".to_string(),
//             author_email: "test@test.com".to_string(),
//             file_path: "test.rs".to_string(),
//             todo: Some(content),
//         };
//
//         let serialized = SerializedEvent::from(&event);
//         let deserialized = TodoEvent::try_from(serialized).expect("failed to deserialize");
//
//         let content = deserialized.todo.expect("content should exist");
//         assert_eq!(content.title, "Fix bug");
//         assert_eq!(
//             content.description,
//             Some("Detailed description".to_string())
//         );
//         assert_eq!(content.priority, Some('A'));
//         assert!(content.tags.contains(&"urgent".to_string()));
//         assert_eq!(
//             content.metadata.get("owner"),
//             Some(&["alice".to_string()][..])
//         );
//     }
//
//     #[test]
//     fn test_cache_build_and_load() {
//         let dir = create_test_repo();
//
//         commit_file(
//             dir.path(),
//             "main.rs",
//             "// TODO #100 Test todo\nfn main() {}",
//             "Add TODO",
//         );
//
//         let mut backend = create_cached_backend(dir.path()).expect("failed to create backend");
//
//         // Build cache
//         backend.build_cache().expect("failed to build cache");
//
//         // Verify cache file exists
//         let cache_path = dir.path().join(CACHE_DIR).join(EVENTS_FILE);
//         assert!(cache_path.exists());
//
//         // Load from cache
//         let events = backend.scan_all_todo_events().expect("failed to scan");
//         assert_eq!(events.len(), 1);
//         assert_eq!(events[0].id, 100);
//     }
//
//     #[test]
//     fn test_cache_clear() {
//         let dir = create_test_repo();
//
//         commit_file(
//             dir.path(),
//             "main.rs",
//             "// TODO #200 Test\nfn main() {}",
//             "Add TODO",
//         );
//
//         let mut backend = create_cached_backend(dir.path()).expect("failed to create backend");
//         backend.build_cache().expect("failed to build cache");
//
//         let cache_path = dir.path().join(CACHE_DIR).join(EVENTS_FILE);
//         assert!(cache_path.exists());
//
//         backend.clear_cache().expect("failed to clear cache");
//         assert!(!cache_path.exists());
//     }
//
//     #[test]
//     fn test_last_commit_tracking() {
//         let dir = create_test_repo();
//
//         commit_file(dir.path(), "main.rs", "fn main() {}", "Initial");
//
//         let backend = create_cached_backend(dir.path()).expect("failed to create backend");
//
//         // No last commit initially
//         assert!(backend.get_last_cached_commit().is_none());
//
//         // Set and retrieve
//         backend
//             .set_last_cached_commit("abc123")
//             .expect("failed to set");
//         assert_eq!(backend.get_last_cached_commit(), Some("abc123".to_string()));
//     }
// }
