use std::collections::BTreeMap;

use anyhow::{Result, bail};

#[derive(Clone, Debug, Default)]
pub struct Section {
    pub name: Option<String>,
    pub raw: String,
    pub values: BTreeMap<String, String>,
    pub complex: bool,
}

pub fn parse(text: &str) -> Result<Vec<Section>> {
    if text.len() > super::MAX_BYTES || text.contains('\0') {
        bail!("AWS configuration is oversized or contains invalid data");
    }
    let mut sections = Vec::new();
    let mut current = Section::default();
    let mut previous_indent = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(header) = trimmed.strip_prefix('[') {
            let (name, tail) = header
                .split_once(']')
                .ok_or_else(|| anyhow::anyhow!("invalid AWS configuration section"))?;
            if name.is_empty()
                || (!tail.trim().is_empty() && !tail.trim().starts_with(['#', ';']))
                || name.chars().any(char::is_control)
            {
                bail!("invalid AWS configuration section");
            }
            sections.push(current);
            current = Section {
                name: Some(name.to_owned()),
                ..Section::default()
            };
            previous_indent = None;
        } else if !trimmed.is_empty() && !trimmed.starts_with(['#', ';']) {
            let indent = line.len() - line.trim_start().len();
            if previous_indent.is_some_and(|previous| indent > previous) {
                current.complex = true;
            } else if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim().to_ascii_lowercase();
                let value = value.trim().to_owned();
                if key.is_empty() || current.values.insert(key, value).is_some() {
                    current.complex = true;
                }
                previous_indent = Some(indent);
            } else {
                current.complex = true;
            }
        }
        current.raw.push_str(line);
        if sections.len() > 1024 {
            bail!("too many AWS configuration sections");
        }
    }
    sections.push(current);
    Ok(sections)
}
