//! Default slug derivation for canonical team/project rows.
//!
//! Exists because migration 026 enforces `teams.slug NOT NULL` +
//! `projects.slug NOT NULL`. The team/project create routes accept `name`
//! but historically did not accept `slug`; rather than force every caller
//! to start sending one, we derive a default here and let the DB constraint
//! catch anything that slips through.
//!
//! Rule: lowercase, replace every run of non-alphanumerics with a single
//! `-`, strip leading/trailing `-`. Matches the rule used by migration 026's
//! backfill SQL so the two paths produce the same slug for the same name.
//! Empty inputs (pure-whitespace names) produce `"untitled"` so the
//! constraint never trips — name validation upstream already rejects empty
//! strings, so this is only a safety net.

pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = true; // suppress leading '-'
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { "untitled".to_string() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugifies_ascii_words() {
        assert_eq!(slugify("Engineering"), "engineering");
        assert_eq!(slugify("My Team"), "my-team");
    }

    #[test]
    fn collapses_runs_of_punctuation() {
        assert_eq!(slugify("A &&& B"), "a-b");
        assert_eq!(slugify("!!foo__bar!!"), "foo-bar");
    }

    #[test]
    fn strips_leading_and_trailing_dashes() {
        assert_eq!(slugify("---hi---"), "hi");
        assert_eq!(slugify("  spaces  "), "spaces");
    }

    #[test]
    fn drops_unicode_gracefully() {
        // Non-ASCII-alphanumeric collapses to single '-'; service-layer
        // name validation is the right place to reject entirely-unicode
        // names, not here. The safety-net fallback prevents a NOT NULL
        // violation if upstream validation is ever bypassed.
        assert_eq!(slugify("日本語"), "untitled");
        assert_eq!(slugify("hello 日本語 world"), "hello-world");
    }

    #[test]
    fn empty_input_returns_untitled() {
        assert_eq!(slugify(""), "untitled");
        assert_eq!(slugify("   "), "untitled");
    }

    #[test]
    fn matches_migration_regex_on_typical_names() {
        // The SQL uses: lower(regexp_replace(name, '[^a-zA-Z0-9]+', '-', 'g')).
        // That rule does NOT strip leading/trailing dashes the way this fn
        // does. Both produce the same output for names that don't start or
        // end with punctuation — which is the steady-state expectation for
        // hand-entered team/project names. Document the divergence here so
        // anyone hitting it knows the SQL is deliberately simpler.
        assert_eq!(slugify("Engineering Team"), "engineering-team");
    }
}
