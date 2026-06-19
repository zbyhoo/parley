# 🗣️ parley

Terminalowy interfejs (TUI) do równoległej pracy z wieloma agentami AI
(`claude`, `codex`) obok siebie, w jednym oknie. 🤝 Agenci mogą komunikować się
między sobą (broker MCP).

## 📋 Wymagania

- macOS na Apple Silicon (arm64)
- CLI agentów dostępne w `PATH`:
  - `claude` — [instalacja](https://docs.claude.com/claude-code)
  - `codex`

## 📦 Instalacja (użytkownik)

```bash
brew install zbyhoo/parley/parley
```

Po instalacji `parley` jest dostępne w terminalu z dowolnego katalogu. 🎉

- ⬆️ Aktualizacja: `brew upgrade parley`
- 🗑️ Odinstalowanie: `brew uninstall parley && brew untap zbyhoo/parley`

## ⌨️ Użycie

Uruchom w katalogu projektu:

```bash
parley
```

Skróty klawiszowe:

| Klawisz             | Akcja                        |
| ------------------- | ---------------------------- |
| `Tab`               | przełącz aktywnego agenta    |
| `Enter`             | wyślij wiadomość do agenta   |
| `@all ...`          | wyślij do wszystkich agentów |
| `Ctrl+]`            | tryb passthrough (wł./wył.)  |
| `?`                 | pomoc                        |
| `Ctrl+R`            | restart aktywnego agenta     |
| `Ctrl+C` / `Ctrl+Q` | wyjście                      |

Stan sesji trafia do `.parley/` w katalogu projektu.

### ⚠️ Gdy agent pyta o interakcję (tryb passthrough)

Czasem agent wymaga bezpośredniej odpowiedzi — potwierdzenia akcji (`y/n`),
wyboru z listy (strzałki), logowania itp. W normalnym trybie to, co piszesz,
trafia do **linii wejścia parley**, a nie do agenta. Żeby odpowiedzieć
**bezpośrednio agentowi**:

1. Naciśnij **`Ctrl+]`** — wejdziesz w tryb **passthrough**.
2. Twoje klawisze (`y`/`n`, strzałki, `Enter`, itp.) lecą teraz wprost do
   aktywnego agenta — zatwierdź / wybierz to, o co pyta.
3. Naciśnij ponownie **`Ctrl+]`**, żeby wrócić do normalnego trybu parley.

> Passthrough działa tylko dla **aktywnego, żyjącego** agenta (`Tab` przełącza
> aktywnego).

## 🤖 Headless mode

Run an agent in a transparent passthrough wrapper connected to a shared broker:

```bash
parley claude                    # wrap claude (auto peer id: "claude")
parley codex resume              # wrap codex with extra args
parley --as reviewer claude      # assign custom peer id "reviewer"
```

The wrapper passes all input/output natively — scroll, mouse, and resize all
work. The agent renders directly to your terminal; parley adds no re-rendering.

### Peer ids

Auto ids are derived from the command name (first word of `<cmd...>` passed to parley): `claude`, `claude-2`, `codex`, …
Use `--as <id>` to assign a custom id. If the id is already in use by another
running wrapper, the new wrapper exits immediately with an error.

### Agent-to-agent communication

All agents in the same directory join one broker. They communicate via two MCP
tools injected by the wrapper:

| Tool | Description |
| ---- | ----------- |
| `list_peers` | List all currently connected peer ids |
| `send_to_peer(to, message)` | Send a message to a specific peer, or `to="all"` for broadcast |

Delivery is automatic — no moderation queue. All traffic is logged to
`.parley/session-*/timeline.jsonl`.

### Broker commands

```bash
parley stop   # stop the broker daemon; removes broker.json
parley mcp    # write/merge a project .mcp.json so a bare `claude`
              # (launched outside parley) can still send messages
              # (best-effort: receiving still requires the wrapper;
              #  re-run after a broker restart)
```

### `parley` with no arguments

Running `parley` with no arguments still launches the existing TUI (see
[Użycie](#️-użycie) above).

## ⚙️ Konfiguracja

Opcjonalny plik konfiguracyjny pozwala nadpisać komendy agentów
(`command`, `resume_command`). Domyślnie: `claude` i `codex`.

## 🔨 Budowanie ze źródła (dev)

```bash
./build.sh            # release binarka -> dist/parley
```

`cargo` nie musi być w `PATH` — skrypt sam dołącza toolchain rustup
(`~/.rustup/toolchains/stable-aarch64-apple-darwin/bin`).

## 🚀 Wydanie nowej wersji

1. Podbij `version` w `Cargo.toml`.
2. Uruchom:

   ```bash
   ./packaging/release.sh
   ```

   Skrypt buduje binarkę, publikuje GitHub Release w publicznym repo tapa
   (`zbyhoo/homebrew-parley`) i aktualizuje `Formula/parley.rb`.

Szczegóły dystrybucji: `packaging/`.
