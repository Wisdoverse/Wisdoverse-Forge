use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Returns the default credentials path, respecting `XDG_CONFIG_HOME`.
/// Matches `cli/internal/auth/store.go:DefaultCredentialsPath`.
pub fn default_credentials_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("agentforge").join("credentials");
    }
    match dirs_home() {
        Some(home) => home.join(".agentforge").join("credentials"),
        None => PathBuf::from(".agentforge").join("credentials"),
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Stores `token` at `path` with 0600 mode, creating parent dirs with 0700.
pub fn store(path: &Path, token: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
    }
    let mut opts = fs::OpenOptions::new();
    opts.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    file.write_all(token.as_bytes())?;
    Ok(())
}

/// Reads and trims the token stored at `path`. Returns `Ok(None)` if the
/// file does not exist.
pub fn load_from_file(path: &Path) -> anyhow::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Source label for [`resolve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    Flag,
    Env,
    File,
    None,
}

impl TokenSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Env => "env",
            Self::File => "file",
            Self::None => "",
        }
    }
}

/// Resolves the effective token: flag > `AGENTFORGE_TOKEN` env > file.
/// Matches `cli/internal/auth/store.go:Resolve`.
pub fn resolve(flag_value: Option<&str>, file_path: &Path) -> (Option<String>, TokenSource) {
    if let Some(v) = flag_value
        && !v.is_empty()
    {
        return (Some(v.to_string()), TokenSource::Flag);
    }
    if let Ok(v) = std::env::var("AGENTFORGE_TOKEN")
        && !v.is_empty()
    {
        return (Some(v), TokenSource::Env);
    }
    if let Ok(Some(v)) = load_from_file(file_path)
        && !v.is_empty()
    {
        return (Some(v), TokenSource::File);
    }
    (None, TokenSource::None)
}

/// Deletes the credentials file. Ok if it doesn't exist.
pub fn delete(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn store_load_round_trip() {
        let d = tempdir().unwrap();
        let path = d.path().join(".agentforge/credentials");
        store(&path, "abc123\n").unwrap();
        let got = load_from_file(&path).unwrap().unwrap();
        assert_eq!(got, "abc123");
    }

    #[test]
    fn load_missing_returns_none() {
        let d = tempdir().unwrap();
        let got = load_from_file(&d.path().join("nope")).unwrap();
        assert_eq!(got, None);
    }

    #[test]
    fn delete_missing_is_ok() {
        let d = tempdir().unwrap();
        delete(&d.path().join("nope")).unwrap();
    }

    #[test]
    fn resolve_priority_flag_wins() {
        let d = tempdir().unwrap();
        let path = d.path().join("creds");
        store(&path, "from-file").unwrap();
        // Isolate env via temp_env so a parallel test that sets AGENTFORGE_TOKEN
        // cannot leak into this one.
        temp_env::with_var("AGENTFORGE_TOKEN", None::<&str>, || {
            let (t, src) = resolve(Some("from-flag"), &path);
            assert_eq!(t.as_deref(), Some("from-flag"));
            assert_eq!(src, TokenSource::Flag);
        });
    }

    #[test]
    fn xdg_overrides_home() {
        let d = tempdir().unwrap();
        // temp_env wraps the scoped env mutation so Rust 2024's unsafe
        // `set_var` / `remove_var` aren't needed and parallel tests stay safe.
        let xdg = d.path().to_owned();
        temp_env::with_var("XDG_CONFIG_HOME", Some(xdg.as_os_str()), || {
            let p = default_credentials_path();
            assert!(p.starts_with(d.path()));
        });
    }
}
