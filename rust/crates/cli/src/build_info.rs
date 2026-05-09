#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub version: String,
    pub commit: String,
    pub date: String,
    pub name: String,
}

impl BuildInfo {
    pub fn from_env() -> Self {
        Self::from_parts(
            option_env!("AGENTFORGE_CLI_VERSION"),
            option_env!("AGENTFORGE_CLI_COMMIT"),
            option_env!("AGENTFORGE_CLI_DATE"),
        )
    }

    fn from_parts(version: Option<&str>, commit: Option<&str>, date: Option<&str>) -> Self {
        Self {
            version: version.filter(|s| !s.is_empty()).unwrap_or("dev").into(),
            commit: commit.filter(|s| !s.is_empty()).unwrap_or("none").into(),
            date: date.filter(|s| !s.is_empty()).unwrap_or("unknown").into(),
            name: "agentforge".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BuildInfo;

    #[test]
    fn uses_dev_fallbacks_when_build_metadata_is_absent() {
        let info = BuildInfo::from_parts(None, None, None);
        assert_eq!(info.version, "dev");
        assert_eq!(info.commit, "none");
        assert_eq!(info.date, "unknown");
        assert_eq!(info.name, "agentforge");
    }

    #[test]
    fn prefers_injected_build_metadata() {
        let info = BuildInfo::from_parts(Some("v1.2.3-4-gabcdef"), Some("abcdef123456"), Some("2026-04-15T12:00:00Z"));
        assert_eq!(info.version, "v1.2.3-4-gabcdef");
        assert_eq!(info.commit, "abcdef123456");
        assert_eq!(info.date, "2026-04-15T12:00:00Z");
    }
}
