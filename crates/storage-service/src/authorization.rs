use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Read,
    ReadWrite,
}

impl Access {
    #[must_use]
    pub const fn allows(self, requested: Self) -> bool {
        matches!(
            (self, requested),
            (Self::ReadWrite, _) | (Self::Read, Self::Read)
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FolderRule {
    pub owner: String,
    #[serde(default)]
    pub grants: BTreeMap<String, Access>,
}

#[derive(Default, Serialize, Deserialize)]
struct Rules {
    #[serde(default)]
    rules: BTreeMap<String, FolderRule>,
}

pub struct AuthorizationStore {
    path: PathBuf,
    rules: Mutex<Rules>,
}

impl AuthorizationStore {
    pub fn open(namespace_root: impl AsRef<Path>) -> io::Result<Self> {
        let path = namespace_root.as_ref().join("authorization.json");
        let rules = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Rules::default(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            path,
            rules: Mutex::new(rules),
        })
    }

    pub async fn set_rule(&self, path: &str, rule: FolderRule) -> io::Result<()> {
        validate_subject(&rule.owner)?;
        for subject in rule.grants.keys() {
            validate_subject(subject)?;
        }
        let path = normalize_path(path)?;
        let mut rules = self.rules.lock().await;
        rules.rules.insert(path, rule);
        persist(&self.path, &rules)
    }

    pub async fn remove_rule(&self, path: &str) -> io::Result<()> {
        let path = normalize_path(path)?;
        let mut rules = self.rules.lock().await;
        rules.rules.remove(&path);
        persist(&self.path, &rules)
    }

    #[must_use]
    pub async fn authorize(&self, subject: &str, path: &str, requested: Access) -> bool {
        let Ok(path) = normalize_path(path) else {
            return false;
        };
        let rules = self.rules.lock().await;
        rules
            .rules
            .iter()
            .rev()
            .find_map(|(folder, rule)| {
                (folder.is_empty()
                    || path == *folder
                    || path
                        .strip_prefix(folder)
                        .is_some_and(|suffix| suffix.starts_with('/')))
                .then_some(rule)
            })
            .is_some_and(|rule| {
                rule.owner == subject
                    || rule
                        .grants
                        .get(subject)
                        .is_some_and(|access| access.allows(requested))
            })
    }
}

fn persist(path: &Path, rules: &Rules) -> io::Result<()> {
    let bytes = serde_json::to_vec(rules).map_err(io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

fn validate_subject(subject: &str) -> io::Result<()> {
    if subject.is_empty() || subject.contains(['/', '\\', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid subject",
        ));
    }
    Ok(())
}

fn normalize_path(path: &str) -> io::Result<String> {
    if path.is_empty() {
        return Ok(String::new());
    }
    if path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".." | ".data"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid folder path",
        ));
    }
    Ok(path.to_owned())
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::{Access, AuthorizationStore, FolderRule};

    #[tokio::test]
    async fn applies_the_longest_matching_rule() {
        let root = env::temp_dir().join(format!("storage-acl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let store = AuthorizationStore::open(&root).unwrap();
        store
            .set_rule(
                "",
                FolderRule {
                    owner: "owner".into(),
                    grants: [("reader".into(), Access::Read)].into(),
                },
            )
            .await
            .unwrap();
        store
            .set_rule(
                "private",
                FolderRule {
                    owner: "other".into(),
                    grants: Default::default(),
                },
            )
            .await
            .unwrap();
        assert!(
            store
                .authorize("owner", "public/file", Access::ReadWrite)
                .await
        );
        assert!(store.authorize("reader", "public/file", Access::Read).await);
        assert!(
            !store
                .authorize("reader", "private/file", Access::Read)
                .await
        );
        assert!(!store.authorize("nobody", "public/file", Access::Read).await);
        let _ = fs::remove_dir_all(root);
    }
}
