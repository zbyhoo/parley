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
