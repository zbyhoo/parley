//! `parley mcp` — wpis serwera parley do projektowego .mcp.json (ścieżka best-effort
//! dla gołego CLI; patrz spec „Trwałość sesji").
use anyhow::Result;
use serde_json::{json, Value};

/// Scala wpis serwera `parley` do podanego JSON (zachowując inne serwery).
pub fn merge_parley_server(existing: Option<&str>, port: u16, token: &str, id: &str) -> Result<String> {
    let mut root: Value = match existing {
        Some(s) if !s.trim().is_empty() => serde_json::from_str(s)?,
        _ => json!({}),
    };
    if !root.is_object() {
        root = json!({});
    }
    let servers = root
        .as_object_mut().unwrap()
        .entry("mcpServers").or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    servers.as_object_mut().unwrap().insert(
        "parley".into(),
        json!({
            "type": "http",
            "url": format!("http://127.0.0.1:{port}/mcp"),
            "headers": { "X-Agent-Id": id, "X-Parley-Token": token }
        }),
    );
    Ok(serde_json::to_string_pretty(&root)?)
}

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let state_dir = cwd.join(".parley");
    let self_exe = std::env::current_exe()?;
    let info = crate::headless::discovery::ensure_broker(&state_dir, &cwd, &self_exe)?;
    let path = cwd.join(".mcp.json");
    let existing = std::fs::read_to_string(&path).ok();
    let merged = merge_parley_server(existing.as_deref(), info.port, &info.token, "claude")?;
    std::fs::write(&path, merged)?;
    println!("wrote parley server to {}", path.display());
    println!("note: re-run `parley mcp` if you restart the broker (port/token change)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_when_absent() {
        let out = merge_parley_server(None, 8765, "tok", "claude").unwrap();
        assert!(out.contains("http://127.0.0.1:8765/mcp"));
        assert!(out.contains("X-Parley-Token"));
    }

    #[test]
    fn preserves_other_servers() {
        let existing = r#"{ "mcpServers": { "other": { "type": "http", "url": "x" } } }"#;
        let out = merge_parley_server(Some(existing), 1, "t", "claude").unwrap();
        assert!(out.contains("\"other\""));
        assert!(out.contains("\"parley\""));
    }
}
