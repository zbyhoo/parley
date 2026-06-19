//! Parsowanie argv `parley` na konkretną intencję uruchomienia.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Brak argumentów → istniejące TUI.
    Tui,
    /// `parley __serve` → broker-demon (foreground).
    Serve,
    /// `parley stop` → ubicie demona.
    Stop,
    /// `parley mcp` → zapis/scalanie .mcp.json.
    Mcp,
    /// `parley [--as id] <cmd...>` → wrapper.
    Wrapper { as_id: Option<String>, command: Vec<String> },
}

pub fn parse(args: &[String]) -> Result<Invocation, String> {
    if args.is_empty() {
        return Ok(Invocation::Tui);
    }

    let mut as_id: Option<String> = None;
    let mut i = 0;

    // Flagi parley i słowa zarezerwowane — tylko dopóki nie trafimy na `--` lub binarkę.
    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                i += 1;
                break;
            }
            "--as" => {
                let v = args.get(i + 1).ok_or("--as requires a value")?;
                as_id = Some(v.clone());
                i += 2;
            }
            "__serve" if i == 0 => return Ok(Invocation::Serve),
            "stop" if i == 0 => return Ok(Invocation::Stop),
            "mcp" if i == 0 => return Ok(Invocation::Mcp),
            _ => break, // pierwszy nie-flagowy token = binarka
        }
    }

    let command: Vec<String> = args[i..].to_vec();
    if command.is_empty() {
        return Err("no agent command given".into());
    }
    Ok(Invocation::Wrapper { as_id, command })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Invocation, String> {
        let v: Vec<String> = a.iter().map(|s| s.to_string()).collect();
        parse(&v)
    }

    #[test]
    fn empty_is_tui() {
        assert_eq!(p(&[]), Ok(Invocation::Tui));
    }

    #[test]
    fn reserved_subcommands() {
        assert_eq!(p(&["__serve"]), Ok(Invocation::Serve));
        assert_eq!(p(&["stop"]), Ok(Invocation::Stop));
        assert_eq!(p(&["mcp"]), Ok(Invocation::Mcp));
    }

    #[test]
    fn bare_binary_is_wrapper() {
        assert_eq!(
            p(&["claude"]),
            Ok(Invocation::Wrapper { as_id: None, command: vec!["claude".into()] })
        );
    }

    #[test]
    fn binary_with_args_keeps_them() {
        assert_eq!(
            p(&["codex", "resume", "--last"]),
            Ok(Invocation::Wrapper {
                as_id: None,
                command: vec!["codex".into(), "resume".into(), "--last".into()],
            })
        );
    }

    #[test]
    fn as_override_before_binary() {
        assert_eq!(
            p(&["--as", "reviewer", "claude", "--resume"]),
            Ok(Invocation::Wrapper {
                as_id: Some("reviewer".into()),
                command: vec!["claude".into(), "--resume".into()],
            })
        );
    }

    #[test]
    fn as_without_value_errors() {
        assert!(p(&["--as"]).is_err());
    }

    #[test]
    fn separator_protects_reserved_word() {
        // po `--` słowo `stop` to nazwa binarki agenta, nie subkomenda
        assert_eq!(
            p(&["--", "stop", "--flag"]),
            Ok(Invocation::Wrapper {
                as_id: None,
                command: vec!["stop".into(), "--flag".into()],
            })
        );
    }

    #[test]
    fn separator_after_as() {
        assert_eq!(
            p(&["--as", "x", "--", "claude"]),
            Ok(Invocation::Wrapper { as_id: Some("x".into()), command: vec!["claude".into()] })
        );
    }

    #[test]
    fn no_command_errors() {
        assert!(p(&["--as", "x"]).is_err());
    }
}
