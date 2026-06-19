//! Wykrywanie i zapis adresu brokera (`.parley/broker.json`) + lockfile.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerInfo {
    pub port: u16,
    pub pid: u32,
    pub token: String,
    pub cwd: String,
}

pub fn broker_json_path(state_dir: &Path) -> PathBuf {
    state_dir.join("broker.json")
}

pub fn lock_path(state_dir: &Path) -> PathBuf {
    state_dir.join("broker.lock")
}

pub fn write_atomic(path: &Path, info: &BrokerInfo) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(info)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn read(path: &Path) -> Option<BrokerInfo> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 32 hex znaki z /dev/urandom (unix). Fallback: PID+czas (gdy brak urandom).
pub fn random_token() -> String {
    use std::io::Read;
    let mut buf = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut buf).is_ok() {
            return buf.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
    let n = std::process::id() as u128
        ^ chrono::Local::now().timestamp_nanos_opt().unwrap_or(0) as u128;
    format!("{n:032x}").chars().take(32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = broker_json_path(dir.path());
        let info = BrokerInfo {
            port: 8765,
            pid: 4242,
            token: "abc".into(),
            cwd: "/tmp/x".into(),
        };
        write_atomic(&path, &info).unwrap();
        assert_eq!(read(&path), Some(info));
    }

    #[test]
    fn read_missing_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(&broker_json_path(dir.path())), None);
    }

    #[test]
    fn read_corrupt_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = broker_json_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(read(&path), None);
    }

    #[test]
    fn token_is_hex_32() {
        let t = random_token();
        assert_eq!(t.len(), 32);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(t, random_token());
    }
}
