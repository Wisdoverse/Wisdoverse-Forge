use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub output: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub org: String,
    #[serde(default)]
    pub defaults: Defaults,
}

/// Returns the default config path, respecting `XDG_CONFIG_HOME`.
/// Matches `cli/internal/config/config.go:DefaultPath`.
pub fn default_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("agentforge").join("config.yaml");
    }
    match std::env::var_os("HOME").map(PathBuf::from) {
        Some(h) => h.join(".agentforge").join("config.yaml"),
        None => PathBuf::from(".agentforge").join("config.yaml"),
    }
}

/// Loads config from `path`. Returns a Config with `defaults.output = "table"`
/// when the file does not exist. Propagates parse errors.
pub fn load(path: &Path) -> anyhow::Result<Config> {
    let mut cfg = Config::default();
    cfg.defaults.output = "table".into();
    match fs::read_to_string(path) {
        Ok(data) => {
            let parsed: Config =
                serde_yaml::from_str(&data).map_err(|e| anyhow::anyhow!("parse config {}: {e}", path.display()))?;
            cfg = parsed;
            if cfg.defaults.output.is_empty() {
                cfg.defaults.output = "table".into();
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow::anyhow!("read config {}: {e}", path.display())),
    }
    Ok(cfg)
}

pub fn save(path: &Path, cfg: &Config) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
    }
    let data = serde_yaml::to_string(cfg)?;
    fs::write(path, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

impl Config {
    pub fn resolve_server(&self) -> String {
        std::env::var("AGENTFORGE_SERVER").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| self.server.clone())
    }

    pub fn resolve_org(&self) -> String {
        std::env::var("AGENTFORGE_ORG").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| self.org.clone())
    }

    pub fn get(&self, key: &str) -> anyhow::Result<String> {
        Ok(match key.to_lowercase().as_str() {
            "server" => self.server.clone(),
            "org" => self.org.clone(),
            "defaults.output" => self.defaults.output.clone(),
            "defaults.project" => self.defaults.project.clone(),
            "defaults.tool" => self.defaults.tool.clone(),
            _ => return Err(anyhow::anyhow!("unknown config key: {key}")),
        })
    }

    pub fn set(&mut self, key: &str, value: &str) -> anyhow::Result<()> {
        match key.to_lowercase().as_str() {
            "server" => self.server = value.into(),
            "org" => self.org = value.into(),
            "defaults.output" => self.defaults.output = value.into(),
            "defaults.project" => self.defaults.project = value.into(),
            "defaults.tool" => self.defaults.tool = value.into(),
            _ => return Err(anyhow::anyhow!("unknown config key: {key}")),
        }
        Ok(())
    }

    pub fn list(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        if !self.server.is_empty() {
            m.insert("server".into(), self.server.clone());
        }
        if !self.org.is_empty() {
            m.insert("org".into(), self.org.clone());
        }
        if !self.defaults.output.is_empty() {
            m.insert("defaults.output".into(), self.defaults.output.clone());
        }
        if !self.defaults.project.is_empty() {
            m.insert("defaults.project".into(), self.defaults.project.clone());
        }
        if !self.defaults.tool.is_empty() {
            m.insert("defaults.tool".into(), self.defaults.tool.clone());
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_missing_returns_defaults() {
        let d = tempdir().unwrap();
        let cfg = load(&d.path().join("nope.yaml")).unwrap();
        assert_eq!(cfg.defaults.output, "table");
    }

    #[test]
    fn save_and_load_round_trip() {
        let d = tempdir().unwrap();
        let p = d.path().join("c.yaml");
        let mut cfg = Config::default();
        cfg.set("server", "https://a.example").unwrap();
        cfg.set("defaults.tool", "claude").unwrap();
        save(&p, &cfg).unwrap();
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.server, "https://a.example");
        assert_eq!(loaded.defaults.tool, "claude");
    }

    #[test]
    fn get_set_list_keys() {
        // Load from a nonexistent file to get the "table" default output.
        let d = tempdir().unwrap();
        let mut cfg = load(&d.path().join("nope.yaml")).unwrap();
        cfg.set("org", "o1").unwrap();
        cfg.set("server", "s").unwrap();
        assert_eq!(cfg.get("org").unwrap(), "o1");
        let keys: Vec<_> = cfg.list().into_keys().collect();
        assert_eq!(keys, vec!["defaults.output", "org", "server"]);
    }

    #[test]
    fn unknown_key_rejected() {
        let mut cfg = Config::default();
        assert!(cfg.set("bogus", "x").is_err());
    }
}
