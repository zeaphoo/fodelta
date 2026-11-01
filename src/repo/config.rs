//! Repository configuration (`config.toml`).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{FodeltaError, Result};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub format_version: u32,
    pub user: UserConfig,
    pub core: CoreConfig,
    pub diff: DiffConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            format_version: CONFIG_VERSION,
            user: UserConfig::default(),
            core: CoreConfig::default(),
            diff: DiffConfig::default(),
        }
    }
}

impl Config {
    pub fn for_new_repo(name: String, email: String) -> Self {
        Self {
            user: UserConfig { name, email },
            ..Self::default()
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&text).map_err(|error| {
            FodeltaError::Config(format!("cannot parse {}: {error}", path.display()))
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let text = toml::to_string_pretty(self)
            .map_err(|error| FodeltaError::Config(format!("cannot serialize config: {error}")))?;
        crate::util::atomic::write_atomic(path, text.as_bytes(), true)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.format_version != CONFIG_VERSION {
            return Err(FodeltaError::UnsupportedFormat {
                found: self.format_version,
                supported: CONFIG_VERSION,
            });
        }
        if !(1..=19).contains(&self.core.compression) {
            return Err(FodeltaError::Config(
                "core.compression must be between 1 and 19".to_string(),
            ));
        }
        if self.core.max_file_size_mb == 0 || self.core.max_file_size_mb > 1_000_000 {
            return Err(FodeltaError::Config(
                "core.max_file_size_mb must be between 1 and 1000000".to_string(),
            ));
        }
        if self.core.prune_age_days == 0 {
            return Err(FodeltaError::Config(
                "core.prune_age_days must be at least 1".to_string(),
            ));
        }
        if self.core.safety_retention_days == 0 {
            return Err(FodeltaError::Config(
                "core.safety_retention_days must be at least 1".to_string(),
            ));
        }
        if !matches!(self.diff.algorithm.as_str(), "myers" | "patience") {
            return Err(FodeltaError::Config(
                "diff.algorithm must be 'myers' or 'patience'".to_string(),
            ));
        }
        if !matches!(self.diff.color.as_str(), "auto" | "always" | "never") {
            return Err(FodeltaError::Config(
                "diff.color must be 'auto', 'always' or 'never'".to_string(),
            ));
        }
        if self.diff.context > 1000 {
            return Err(FodeltaError::Config(
                "diff.context must be at most 1000".to_string(),
            ));
        }
        if self.diff.max_inline_bytes == 0 {
            return Err(FodeltaError::Config(
                "diff.max_inline_bytes must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    /// Decompressed object ceiling. Files may be admitted up to
    /// `max_file_size_mb`, so leave some headroom for the object header and
    /// explicitly allowed large files.
    pub fn max_object_size_bytes(&self) -> u64 {
        self.core
            .max_file_size_mb
            .saturating_mul(1024 * 1024)
            .saturating_mul(2)
    }

    pub const KEYS: &'static [&'static str] = &[
        "format_version",
        "user.name",
        "user.email",
        "core.compression",
        "core.max_file_size_mb",
        "core.allow_large_files",
        "core.prune_age_days",
        "core.safety_retention_days",
        "core.fsync",
        "diff.context",
        "diff.algorithm",
        "diff.max_inline_bytes",
        "diff.color",
    ];

    pub fn get(&self, key: &str) -> Result<String> {
        let value = match key {
            "format_version" => self.format_version.to_string(),
            "user.name" => self.user.name.clone(),
            "user.email" => self.user.email.clone(),
            "core.compression" => self.core.compression.to_string(),
            "core.max_file_size_mb" => self.core.max_file_size_mb.to_string(),
            "core.allow_large_files" => self.core.allow_large_files.to_string(),
            "core.prune_age_days" => self.core.prune_age_days.to_string(),
            "core.safety_retention_days" => self.core.safety_retention_days.to_string(),
            "core.fsync" => self.core.fsync.as_str().to_string(),
            "diff.context" => self.diff.context.to_string(),
            "diff.algorithm" => self.diff.algorithm.clone(),
            "diff.max_inline_bytes" => self.diff.max_inline_bytes.to_string(),
            "diff.color" => self.diff.color.clone(),
            other => return Err(unknown_key(other)),
        };
        Ok(value)
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "format_version" => {
                return Err(FodeltaError::Config(
                    "format_version is read-only".to_string(),
                ));
            }
            "user.name" => self.user.name = value.to_string(),
            "user.email" => self.user.email = value.to_string(),
            "core.compression" => {
                self.core.compression = parse_number(key, value)?;
            }
            "core.max_file_size_mb" => {
                self.core.max_file_size_mb = parse_number(key, value)?;
            }
            "core.allow_large_files" => {
                self.core.allow_large_files = parse_bool(key, value)?;
            }
            "core.prune_age_days" => {
                self.core.prune_age_days = parse_number(key, value)?;
            }
            "core.safety_retention_days" => {
                self.core.safety_retention_days = parse_number(key, value)?;
            }
            "core.fsync" => {
                self.core.fsync = FsyncPolicy::parse(value).ok_or_else(|| {
                    FodeltaError::Config(format!(
                        "core.fsync must be 'refs', 'all' or 'none', got '{value}'"
                    ))
                })?;
            }
            "diff.context" => self.diff.context = parse_number(key, value)?,
            "diff.algorithm" => self.diff.algorithm = value.to_string(),
            "diff.max_inline_bytes" => {
                self.diff.max_inline_bytes = parse_number(key, value)?;
            }
            "diff.color" => self.diff.color = value.to_string(),
            other => return Err(unknown_key(other)),
        }
        self.validate()
    }
}

fn unknown_key(key: &str) -> FodeltaError {
    FodeltaError::Config(format!(
        "unknown configuration key '{key}'; known keys: {}",
        Config::KEYS.join(", ")
    ))
}

fn parse_number<T>(key: &str, value: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| FodeltaError::Config(format!("{key} expects a number, got '{value}'")))
}

fn parse_bool(key: &str, value: &str) -> Result<bool> {
    match value {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(FodeltaError::Config(format!(
            "{key} expects a boolean, got '{other}'"
        ))),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct UserConfig {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct CoreConfig {
    pub compression: i32,
    pub max_file_size_mb: u64,
    pub allow_large_files: bool,
    pub prune_age_days: u64,
    pub safety_retention_days: u64,
    pub fsync: FsyncPolicy,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            compression: 3,
            max_file_size_mb: 512,
            allow_large_files: false,
            prune_age_days: 14,
            safety_retention_days: 30,
            fsync: FsyncPolicy::Refs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct DiffConfig {
    pub context: u32,
    pub algorithm: String,
    pub max_inline_bytes: u64,
    pub color: String,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            context: 3,
            algorithm: "myers".to_string(),
            max_inline_bytes: 8 * 1024 * 1024,
            color: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FsyncPolicy {
    #[default]
    Refs,
    All,
    None,
}

impl FsyncPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Refs => "refs",
            Self::All => "all",
            Self::None => "none",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "refs" => Some(Self::Refs),
            "all" => Some(Self::All),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    pub const fn fsync_objects(self) -> bool {
        matches!(self, Self::All)
    }

    pub const fn fsync_refs(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip_through_toml() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
        assert_eq!(parsed.core.compression, 3);
        assert_eq!(parsed.core.fsync, FsyncPolicy::Refs);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let text = "format_version = 1\n\n[core]\ncompression = 3\nsurprise = true\n";
        assert!(toml::from_str::<Config>(text).is_err());
    }

    #[test]
    fn get_and_set_known_keys() {
        let mut config = Config::default();
        config.set("core.compression", "9").unwrap();
        assert_eq!(config.get("core.compression").unwrap(), "9");
        config.set("core.fsync", "all").unwrap();
        assert_eq!(config.core.fsync, FsyncPolicy::All);
        assert!(config.set("core.nope", "1").is_err());
        assert!(config.get("nope").is_err());
    }

    #[test]
    fn rejects_invalid_values() {
        let mut config = Config::default();
        assert!(config.set("core.compression", "99").is_err());
        assert!(config.set("core.max_file_size_mb", "0").is_err());
        assert!(config.set("diff.algorithm", "magic").is_err());
        assert!(config.set("format_version", "2").is_err());
    }

    #[test]
    fn load_rejects_newer_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let text = "format_version = 2\n";
        fs::write(&path, text).unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, FodeltaError::UnsupportedFormat { .. }));
    }
}
