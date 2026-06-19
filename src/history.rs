use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

/// Maksymalna liczba wpisów trzymanych w pamięci (ostatnie N z pliku).
/// Plik na dysku nie jest przycinany — limit dotyczy tylko RAM przy starcie.
const MAX_ENTRIES: usize = 500;

/// Trwała historia wysłanych promptów (globalna dla projektu).
/// Format pliku: JSONL — każda linia to `serde_json` string, więc prompty
/// wieloliniowe (zawierające '\n') round-trippują poprawnie.
pub struct History {
    file: File,
    pub entries: Vec<String>,
}

impl History {
    /// Otwiera (tworząc katalogi) plik JSONL w trybie append; wczytuje wpisy.
    /// Na unixach wymusza 0600 — historia zawiera treść promptów użytkownika.
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut opts = OpenOptions::new();
        opts.create(true).append(true).read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600); // skuteczne tylko przy tworzeniu pliku
        }
        let file = opts.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        let mut entries = Vec::new();
        // Osobny uchwyt do odczytu: `file` jest w trybie append (kursor na końcu).
        for line in BufReader::new(File::open(path)?).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<String>(&line) {
                Ok(s) => entries.push(s),
                Err(_) => continue, // uszkodzona linia nie blokuje sesji
            }
        }
        // Trzymaj w pamięci tylko ostatnie MAX_ENTRIES.
        if entries.len() > MAX_ENTRIES {
            entries.drain(0..entries.len() - MAX_ENTRIES);
        }
        Ok(History { file, entries })
    }

    /// Dopisuje prompt do historii. Pomija puste (po trim) oraz duplikat
    /// identyczny z ostatnim wpisem (dedupe kolejnych powtórzeń).
    pub fn push(&mut self, line: String) -> io::Result<()> {
        if line.trim().is_empty() {
            return Ok(());
        }
        if self.entries.last() == Some(&line) {
            return Ok(());
        }
        let encoded = serde_json::to_string(&line)?;
        writeln!(self.file, "{encoded}")?;
        self.file.flush()?;
        self.entries.push(line);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/history.jsonl");
        {
            let mut h = History::open(&path).unwrap();
            h.push("hello".into()).unwrap();
            h.push("world".into()).unwrap();
            assert_eq!(h.entries.len(), 2);
        }
        let h = History::open(&path).unwrap();
        assert_eq!(h.entries, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn multiline_entry_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        {
            let mut h = History::open(&path).unwrap();
            h.push("line one\nline two".into()).unwrap();
        }
        let h = History::open(&path).unwrap();
        assert_eq!(h.entries, vec!["line one\nline two".to_string()]);
    }

    #[test]
    fn skips_consecutive_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let mut h = History::open(&path).unwrap();
        h.push("same".into()).unwrap();
        h.push("same".into()).unwrap();
        h.push("other".into()).unwrap();
        h.push("same".into()).unwrap(); // nie-kolejny duplikat → zapisany
        assert_eq!(h.entries, vec!["same".to_string(), "other".to_string(), "same".to_string()]);
    }

    #[test]
    fn skips_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let mut h = History::open(&path).unwrap();
        h.push("   ".into()).unwrap();
        h.push("".into()).unwrap();
        assert!(h.entries.is_empty());
    }

    #[test]
    fn corrupted_line_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        {
            let mut h = History::open(&path).unwrap();
            h.push("ok".into()).unwrap();
        }
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "\"niepełny json").unwrap();
        let h = History::open(&path).unwrap();
        assert_eq!(h.entries, vec!["ok".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn file_mode_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let _h = History::open(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
