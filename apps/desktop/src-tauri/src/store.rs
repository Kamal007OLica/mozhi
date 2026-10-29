//! Settings and history persistence — small JSON files written atomically.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Serialize};

use crate::types::{HistoryItem, Settings};

const HISTORY_CAP: usize = 500;

fn read_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            log::warn!("{} is unreadable ({e}); starting fresh", path.display());
            T::default()
        }),
        Err(_) => T::default(),
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub struct SettingsStore {
    path: PathBuf,
    inner: RwLock<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Self {
        let inner = RwLock::new(read_json(&path));
        Self { path, inner }
    }

    pub fn get(&self) -> Settings {
        self.inner.read().clone()
    }

    pub fn set(&self, next: Settings) -> anyhow::Result<()> {
        write_json(&self.path, &next)?;
        *self.inner.write() = next;
        Ok(())
    }

    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> anyhow::Result<Settings> {
        let mut next = self.get();
        f(&mut next);
        self.set(next.clone())?;
        Ok(next)
    }
}

pub struct History {
    path: PathBuf,
    items: RwLock<Vec<HistoryItem>>,
}

impl History {
    pub fn load(path: PathBuf) -> Self {
        let items = RwLock::new(read_json(&path));
        Self { path, items }
    }

    /// Newest first.
    pub fn list(&self) -> Vec<HistoryItem> {
        self.items.read().clone()
    }

    pub fn add(&self, item: HistoryItem) {
        let snapshot = {
            let mut items = self.items.write();
            items.insert(0, item);
            items.truncate(HISTORY_CAP);
            items.clone()
        };
        if let Err(e) = write_json(&self.path, &snapshot) {
            log::warn!("could not save history: {e}");
        }
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        let snapshot = {
            let mut items = self.items.write();
            items.retain(|i| i.id != id);
            items.clone()
        };
        write_json(&self.path, &snapshot)
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        self.items.write().clear();
        write_json(&self.path, &Vec::<HistoryItem>::new())
    }
}
