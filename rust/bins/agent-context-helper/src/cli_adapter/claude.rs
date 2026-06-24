use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use agentforge_core::context_envelope::{ContextEnvelope, ContextEnvelopeItemKind};

use super::ContextAdapterReport;

const START_TAG_PREFIX: &str = "<!-- agentforge-context:start";
const END_TAG: &str = "<!-- agentforge-context:end -->";

pub fn apply_claude_adapter(envelope: &ContextEnvelope, home: &Path) -> Result<ContextAdapterReport> {
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).with_context(|| format!("create {}", claude_dir.display()))?;

    let target = claude_dir.join("CLAUDE.md");
    let existing = match fs::read_to_string(&target) {
        Ok(existing) => existing,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err).with_context(|| format!("read {}", target.display())),
    };

    let block = render_context_block(envelope);
    let next = replace_or_append_context_block(&existing, &block);
    fs::write(&target, next).with_context(|| format!("write {}", target.display()))?;

    Ok(ContextAdapterReport {
        adapter: "claude".to_string(),
        applied_items: envelope.applied.len(),
        degradation: envelope.degradation.clone(),
    })
}

fn render_context_block(envelope: &ContextEnvelope) -> String {
    let mut out = String::new();
    out.push_str(&format!("<!-- agentforge-context:start {} -->\n", envelope.envelope_version));
    out.push_str("# AgentForge Runtime Context\n\n");
    out.push_str(&format!("Run: {}\n", envelope.run_id));
    out.push_str(&format!("Task: {}\n", envelope.task_id));
    out.push_str(&format!("Agent: {}\n\n", envelope.agent_id));

    if envelope.applied.is_empty() && envelope.skills_mount.is_empty() {
        out.push_str("No approved AgentForge context was applied to this run.\n");
    } else {
        let memory_items: Vec<_> =
            envelope.applied.iter().filter(|item| item.kind == ContextEnvelopeItemKind::Memory).collect();
        if !memory_items.is_empty() {
            out.push_str("## Applied Memory\n\n");
            for item in memory_items {
                out.push_str(&format!("- {}\n", item.title));
                out.push_str(&format!("  Source: {} ({})\n", item.source.source_type, item.content_ref));
                let content = redacted_content(&item.sensitivity, &item.content);
                out.push_str(&format!("  Content: {content}\n"));
            }
            out.push('\n');
        }

        if !envelope.skills_mount.is_empty() {
            out.push_str("## Mounted Skills\n\n");
            for skill in &envelope.skills_mount {
                out.push_str(&format!("- {} v{}\n", skill.name, skill.version));
                out.push_str(&format!("  Path: {}\n", skill.path));
            }
            out.push('\n');
        }
    }

    if !envelope.degradation.is_empty() {
        out.push_str(&format!("Degradation: {}\n", envelope.degradation.join(", ")));
    }
    out.push_str(END_TAG);
    out.push('\n');
    out
}

fn redacted_content(sensitivity: &str, content: &str) -> String {
    if sensitivity == "secret_detected" { format!("[redacted: {sensitivity}]") } else { content.to_string() }
}

fn replace_or_append_context_block(existing: &str, block: &str) -> String {
    let Some(start) = existing.find(START_TAG_PREFIX) else {
        if existing.trim().is_empty() {
            return block.to_string();
        }
        return format!("{}\n\n{}", existing.trim_end(), block);
    };
    let Some(end_rel) = existing[start..].find(END_TAG) else {
        return format!("{}\n\n{}", existing.trim_end(), block);
    };
    let end = start + end_rel + END_TAG.len();
    let mut next = String::new();
    next.push_str(existing[..start].trim_end());
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(block.trim_end());
    let tail = existing[end..].trim_start();
    if !tail.is_empty() {
        next.push_str("\n\n");
        next.push_str(tail);
    }
    next.push('\n');
    next
}
