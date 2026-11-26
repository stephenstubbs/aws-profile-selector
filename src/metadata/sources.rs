use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::catalog::identifier;
use super::storage::{self, Paths, Snapshot};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub name: String,
    pub account: String,
    pub vault: String,
    pub document: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Registry {
    pub schema_version: u32,
    pub sources: Vec<Source>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            schema_version: 1,
            sources: Vec::new(),
        }
    }
}

impl Registry {
    pub fn load(paths: &Paths) -> Result<(Self, Snapshot)> {
        let snapshot = storage::snapshot(&paths.sources, true)?;
        let registry = if snapshot.bytes.is_empty() {
            Self::default()
        } else {
            serde_json::from_slice(&snapshot.bytes)
                .map_err(|_| anyhow::anyhow!("source registry is not valid closed-schema JSON"))?
        };
        registry.validate()?;
        Ok((registry, snapshot))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported source registry version"
        );
        ensure!(self.sources.len() <= 64, "too many metadata sources");
        let mut names = BTreeSet::new();
        let mut references = BTreeSet::new();
        for source in &self.sources {
            ensure!(identifier(&source.name), "invalid source name");
            ensure!(
                [&source.account, &source.vault, &source.document]
                    .into_iter()
                    .all(|value| stable_id(value)),
                "account, vault and Document references must be stable 1Password IDs"
            );
            ensure!(names.insert(&source.name), "duplicate metadata source name");
            ensure!(
                references.insert((&source.account, &source.vault, &source.document)),
                "a metadata Document was bound more than once"
            );
        }
        Ok(())
    }

    pub fn save(&self, paths: &Paths, snapshot: &Snapshot) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|_| anyhow::anyhow!("cannot encode source registry"))?;
        storage::atomic_write(&paths.sources, snapshot, &bytes)
    }

    pub fn add(&mut self, source: Source, replace: bool) -> Result<()> {
        if let Some(index) = self
            .sources
            .iter()
            .position(|entry| entry.name == source.name)
        {
            ensure!(
                replace,
                "source is already bound; use --replace after reviewing the new references"
            );
            self.sources[index] = source;
        } else {
            self.sources.push(source);
        }
        self.sources
            .sort_by(|left, right| left.name.cmp(&right.name));
        self.validate()
    }

    pub fn remove(&mut self, name: &str) -> Result<()> {
        let previous = self.sources.len();
        self.sources.retain(|source| source.name != name);
        ensure!(
            self.sources.len() != previous,
            "metadata source is not configured"
        );
        Ok(())
    }
}

fn stable_id(value: &str) -> bool {
    value.len() == 26
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || (b'2'..=b'7').contains(&byte))
}
