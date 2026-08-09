//! Tiny JSON file store with atomic writes (temp file + rename). Corrupt
//! files are moved aside and replaced with defaults instead of crashing.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Store {
        Store { dir }
    }

    pub fn path(&self, file: &str) -> PathBuf {
        self.dir.join(file)
    }

    pub fn load_or_default<T: DeserializeOwned + Default>(&self, file: &str) -> T {
        let path = self.path(file);
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(value) => value,
                Err(e) => {
                    tracing::error!(?path, %e, "corrupt state file; moving aside");
                    let _ = std::fs::rename(&path, backup_path(&path));
                    T::default()
                }
            },
            Err(_) => T::default(),
        }
    }

    pub fn save<T: Serialize>(&self, file: &str, value: &T) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path(file);
        let tmp = path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(value).map_err(anyhow::Error::from)?;
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".corrupt");
    PathBuf::from(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Data {
        items: Vec<String>,
    }

    #[test]
    fn roundtrip_and_corrupt_recovery() {
        let dir = std::env::temp_dir().join(format!("lighter-store-{}", uuid::Uuid::new_v4()));
        let store = Store::new(dir.clone());

        // Missing file → default.
        assert_eq!(store.load_or_default::<Data>("x.json"), Data::default());

        // Roundtrip.
        let data = Data {
            items: vec!["a".into(), "b".into()],
        };
        store.save("x.json", &data).unwrap();
        assert_eq!(store.load_or_default::<Data>("x.json"), data);

        // Corrupt file → moved aside, default returned.
        std::fs::write(store.path("x.json"), "{not json").unwrap();
        assert_eq!(store.load_or_default::<Data>("x.json"), Data::default());
        assert!(store.path("x.json.corrupt").exists() || !store.path("x.json").exists());

        let _ = std::fs::remove_dir_all(dir);
    }
}
