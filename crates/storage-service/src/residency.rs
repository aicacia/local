use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::Path,
};

use serde::{Deserialize, Serialize};

const POLICY_FILE: &str = "storage-residency.json";

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Residency {
    Full,
    #[default]
    Passthrough,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ResidencyPolicy {
    device_default: Residency,
    excluded_applications: BTreeSet<i64>,
    namespaces: BTreeMap<String, NamespacePolicy>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
struct NamespacePolicy {
    rules: BTreeMap<String, Residency>,
}

impl ResidencyPolicy {
    pub fn load(root: &Path) -> io::Result<Self> {
        let path = root.join(POLICY_FILE);
        match fs::read(&path) {
            Ok(content) => serde_json::from_slice(&content)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        let path = root.join(POLICY_FILE);
        let temporary = path.with_extension("tmp");
        let content = serde_json::to_vec_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(&temporary, content)?;
        fs::rename(temporary, path)
    }

    #[must_use]
    pub fn is_application_excluded(&self, application_id: i64) -> bool {
        self.excluded_applications.contains(&application_id)
    }

    #[must_use]
    pub fn device_default(&self) -> Residency {
        self.device_default
    }

    pub fn set_device_default(&mut self, residency: Residency) {
        self.device_default = residency;
    }

    pub fn set_application_excluded(
        &mut self,
        application_id: i64,
        excluded: bool,
    ) -> Result<(), String> {
        validate_application_id(application_id)?;
        if excluded {
            self.excluded_applications.insert(application_id);
        } else {
            self.excluded_applications.remove(&application_id);
        }
        Ok(())
    }

    pub fn set_residency(
        &mut self,
        user_sub: &str,
        application_id: i64,
        path: &str,
        residency: Residency,
    ) -> Result<(), String> {
        let namespace = namespace_id(user_sub, application_id)?;
        validate_path(path)?;
        self.namespaces
            .entry(namespace_key(&namespace))
            .or_default()
            .rules
            .insert(path.to_owned(), residency);
        Ok(())
    }

    pub fn residency(
        &self,
        user_sub: &str,
        application_id: i64,
        path: &str,
    ) -> Result<Residency, String> {
        let namespace = namespace_id(user_sub, application_id)?;
        validate_path(path)?;
        Ok(self
            .namespaces
            .get(&namespace_key(&namespace))
            .and_then(|policy| matching_rule(&policy.rules, path))
            .copied()
            .unwrap_or(self.device_default))
    }
}

fn namespace_id(user_sub: &str, application_id: i64) -> Result<(&str, i64), String> {
    validate_user_sub(user_sub)?;
    validate_application_id(application_id)?;
    Ok((user_sub, application_id))
}

fn namespace_key(namespace: &(&str, i64)) -> String {
    format!("{}:{}", namespace.0, namespace.1)
}

fn matching_rule<'a>(rules: &'a BTreeMap<String, Residency>, path: &str) -> Option<&'a Residency> {
    let mut current = path;
    loop {
        if let Some(residency) = rules.get(current) {
            return Some(residency);
        }
        current = match current.rsplit_once('/') {
            Some((parent, _)) => parent,
            None => return rules.get(""),
        };
    }
}

pub(crate) fn validate_user_sub(user_sub: &str) -> Result<(), String> {
    if !user_sub.is_empty()
        && user_sub
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Ok(())
    } else {
        Err("invalid user subject".to_owned())
    }
}

fn validate_application_id(application_id: i64) -> Result<(), String> {
    if application_id > 0 {
        Ok(())
    } else {
        Err("invalid application id".to_owned())
    }
}

fn validate_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err("invalid residency path".to_owned());
    }
    if path
        .split('/')
        .all(|component| !component.is_empty() && component != "." && component != "..")
    {
        Ok(())
    } else {
        Err("invalid residency path".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::{Residency, ResidencyPolicy};

    #[test]
    fn defaults_to_passthrough() {
        let policy = ResidencyPolicy::default();
        assert_eq!(policy.device_default(), Residency::Passthrough);
        assert_eq!(
            policy.residency("user", 1, "notes/today.txt"),
            Ok(Residency::Passthrough)
        );
    }

    #[test]
    fn uses_the_device_default_when_no_scoped_rule_matches() {
        let mut policy = ResidencyPolicy::default();
        policy.set_device_default(Residency::Full);
        assert_eq!(
            policy.residency("user", 1, "notes/today.txt"),
            Ok(Residency::Full)
        );
    }

    #[test]
    fn resolves_the_most_specific_rule() {
        let mut policy = ResidencyPolicy::default();
        policy
            .set_residency("user", 1, "", Residency::Full)
            .unwrap();
        policy
            .set_residency("user", 1, "notes", Residency::Passthrough)
            .unwrap();
        policy
            .set_residency("user", 1, "notes/today.txt", Residency::Full)
            .unwrap();
        assert_eq!(
            policy.residency("user", 1, "notes/other.txt"),
            Ok(Residency::Passthrough)
        );
        assert_eq!(
            policy.residency("user", 1, "notes/today.txt"),
            Ok(Residency::Full)
        );
    }

    #[test]
    fn exclusions_apply_to_all_users() {
        let mut policy = ResidencyPolicy::default();
        policy.set_application_excluded(1, true).unwrap();
        assert!(policy.is_application_excluded(1));
        assert_eq!(
            policy.residency("another-user", 1, ""),
            Ok(Residency::Passthrough)
        );
    }

    #[test]
    fn persists() {
        let root = env::temp_dir().join(format!("residency-policy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut policy = ResidencyPolicy::default();
        policy.set_device_default(Residency::Full);
        policy
            .set_residency("user", 1, "notes", Residency::Full)
            .unwrap();
        policy.set_application_excluded(2, true).unwrap();
        policy.save(&root).unwrap();
        assert_eq!(ResidencyPolicy::load(&root).unwrap(), policy);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_invalid_identifiers_and_paths() {
        let mut policy = ResidencyPolicy::default();
        assert!(
            policy
                .set_residency("../user", 1, "", Residency::Full)
                .is_err()
        );
        assert!(
            policy
                .set_residency("user", 0, "", Residency::Full)
                .is_err()
        );
        assert!(
            policy
                .set_residency("user", 1, "../notes", Residency::Full)
                .is_err()
        );
    }
}
