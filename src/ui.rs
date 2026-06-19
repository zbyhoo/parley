use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;
use tui_term::widget::PseudoTerminal;
use unicode_width::UnicodeWidthStr;

use crate::app::{is_bash_input, App, Mode};
use crate::router::AgentId;
use crate::timeline::Kind;

pub struct Areas {
    pub claude: Rect,
    pub codex: Rect,
    pub timeline: Rect,
    pub input: Rect,
    pub status: Rect,
}

/// Minimalna i maksymalna wysokość pola input (z ramką).
const INPUT_MIN_H: u16 = 3;
const INPUT_MAX_H: u16 = 8;

/// Wysokość pola input (z ramką) dla danej treści i szerokości wnętrza.
/// Liczy wizualne wiersze: po '\n' + soft-wrap. Klamruje do [INPUT_MIN_H, INPUT_MAX_H].
pub fn input_height(input: &str, inner_width: u16) -> u16 {
    let w = inner_width.max(1) as usize;
    let mut rows: usize = 0;
    for line in input.split('\n') {
        let width = UnicodeWidthStr::width(line);
        rows += width.div_ceil(w).max(1);
    }
    ((rows + 2) as u16).clamp(INPUT_MIN_H, INPUT_MAX_H)
}

/// Układ B: panele u góry, timeline, input, status.
/// `input_h` (3..=8) rośnie kosztem timeline — wysokość paneli (a więc rozmiar PTY)
/// jest stała i niezależna od inputu, żeby agenci nie przerysowywali się przy pisaniu.
pub fn areas(area: Rect, input_h: u16) -> Areas {
    let input_h = input_h.clamp(INPUT_MIN_H, INPUT_MAX_H);
    let status: u16 = 1;
    // panele = tak jak przy input=3, timeline=8 → H - (1+3+8) = H - 12 (stałe).
    let panes = area.height.saturating_sub(12);
    let used = panes + input_h + status;
    let timeline = area.height.saturating_sub(used);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(panes),
            Constraint::Length(timeline),
            Constraint::Length(input_h),
            Constraint::Length(status),
        ])
        .split(area);
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);
    Areas {
        claude: panels[0],
        codex: panels[1],
        timeline: rows[1],
        input: rows[2],
        status: rows[3],
    }
}

/// Rozmiar PTY wewnątrz panelu z ramką.
pub fn pty_size(panel: Rect) -> (u16, u16) {
    (panel.height.saturating_sub(2), panel.width.saturating_sub(2))
}

pub fn render(f: &mut Frame, app: &App) {
    let inner_w = f.area().width.saturating_sub(4); // ramka (2) + prefiks "> " (2)
    let input_h = input_height(&app.input, inner_w);
    let a = areas(f.area(), input_h);
    render_pane(f, app, AgentId::Claude, a.claude);
    render_pane(f, app, AgentId::Codex, a.codex);
    render_timeline(f, app, a.timeline);
    render_input(f, app, a.input);
    render_status(f, app, a.status);
    // Help ma najwyższy priorytet, potem quit, potem pending, potem autocomplete.
    if app.show_help {
        render_help_popup(f, f.area());
    } else if app.confirm_quit {
        render_quit_popup(f, f.area());
    } else if !app.pending.lock().unwrap().is_empty() {
        render_pending_popup(f, app, f.area());
    } else {
        render_completion_popup(f, app, a.input);
    }
}

fn render_pane(f: &mut Frame, app: &App, id: AgentId, area: Rect) {
    let pane = app.pane(id);
    let focused = app.focus == id;
    let mut title = pane.id.label().to_string();
    if pane.exited.is_some() || pane.proc.is_none() {
        title = format!("{title} — {}", pane.status);
    } else if focused && matches!(app.mode, Mode::Passthrough) {
        title = format!("{title} [PASSTHROUGH]");
    }
    let border_style = if focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default().borders(Borders::ALL).title(title).border_style(border_style);
    match (&pane.proc, pane.exited) {
        (Some(p), None) => p.with_screen(|screen| {
            f.render_widget(PseudoTerminal::new(screen).block(block), area);
        }),
        _ => f.render_widget(
            Paragraph::new(pane.status.clone())
                .alignment(ratatui::layout::Alignment::Center)
                .block(block),
            area,
        ),
    }
}

fn render_timeline(f: &mut Frame, app: &App, area: Rect) {
    let visible = area.height.saturating_sub(2) as usize;
    let items: Vec<ListItem> = app
        .timeline
        .entries
        .iter()
        .rev()
        .take(visible)
        .rev()
        .map(|e| {
            let time = e.ts.get(11..19).unwrap_or("--:--:--");
            let line = match e.kind {
                Kind::Message => format!("[{time}] {} → {}: {}", e.from, e.to, e.text),
                Kind::Event => format!("[{time}] ({} → {}) {}", e.from, e.to, e.text),
            };
            ListItem::new(Line::from(line))
        })
        .collect();
    f.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("timeline")),
        area,
    );
}

fn render_input(f: &mut Frame, app: &App, area: Rect) {
    // Ghost text: suffix wybranego kandydata autocomplete (to, co dopisze Tab).
    let ghost = app.active_completion().and_then(|c| {
        let item = c.candidates[app.completion_index % c.candidates.len()];
        item.value.strip_prefix(app.input.as_str()).filter(|s| !s.is_empty()).map(str::to_string)
    });

    // Tryb bash ('!'): inny kolor tekstu i ramki, jak w Claude Code / Codex.
    let bash = is_bash_input(&app.input);
    let content_style = if bash {
        Style::default().fg(Color::LightMagenta)
    } else {
        Style::default()
    };

    let logical: Vec<&str> = app.input.split('\n').collect();
    let last = logical.len() - 1;
    let lines: Vec<Line> = logical
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let mut spans = Vec::new();
            if i == 0 {
                spans.push(Span::raw("> "));
            }
            spans.push(Span::styled((*l).to_string(), content_style));
            if i == last {
                if let Some(g) = &ghost {
                    spans.push(Span::styled(g.clone(), Style::default().fg(Color::DarkGray)));
                }
            }
            Line::from(spans)
        })
        .collect();

    let title = if bash { "input — ! runs in shell" } else { "input (@claude/@codex/@all)" };
    let mut block = Block::default().borders(Borders::ALL).title(title);
    if bash {
        block = block.border_style(Style::default().fg(Color::LightMagenta));
    }
    f.render_widget(
        Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }).block(block),
        area,
    );

    if matches!(app.mode, Mode::Input) && !app.confirm_quit {
        // Pozycja kursora (koniec tekstu) z uwzględnieniem zawijania i '\n'.
        let inner_w = area.width.saturating_sub(2).max(1); // wnętrze; prefiks wlicza się w 1. linię
        let mut row: u16 = 0;
        let mut col: u16 = 0;
        for (i, l) in logical.iter().enumerate() {
            let mut w = UnicodeWidthStr::width(*l) as u16;
            if i == 0 {
                w += 2; // prefiks "> "
            }
            if i == last {
                row += w / inner_w;
                col = w % inner_w;
            } else {
                row += (w / inner_w) + 1;
            }
        }
        let x = (area.x + 1 + col).min(area.right().saturating_sub(2));
        let y = (area.y + 1 + row).min(area.bottom().saturating_sub(2));
        f.set_cursor_position((x, y));
    }
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let mode = match app.mode {
        Mode::Input => "INPUT",
        Mode::Passthrough => "PASSTHROUGH",
    };
    let auto = match app.auto {
        Some(n) => format!(" | AUTO ({n} left)"),
        None => String::new(),
    };
    let text = format!(
        " {mode}{auto} | focus: {} | Tab=focus Ctrl+]=mode Ctrl+R=restart Ctrl+C=quit ?=help",
        app.focus.label()
    );
    f.render_widget(Paragraph::new(text).style(Style::default().bg(Color::DarkGray)), area);
}

/// Wycentrowany prostokąt o zadanych wymiarach (przycięty do obszaru).
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// Popup oczekującej wiadomości agent→agent (głowa kolejki) ze skrótami moderacji.
fn render_pending_popup(f: &mut Frame, app: &App, area: Rect) {
    let q = app.pending.lock().unwrap();
    let total = q.len();
    let Some(head) = q.front() else { return };
    let more = if total > 1 { format!("    (+{} more)", total - 1) } else { String::new() };
    let body = format!(
        "{} → {}:\n\n{}\n\ny / Enter = approve    n / Esc = reject{}",
        head.from.label(),
        head.to.label(),
        head.text,
        more,
    );
    let rect = centered_rect(64, 9, area);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" peer message ")
        .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(
        Paragraph::new(body).alignment(ratatui::layout::Alignment::Left).block(block),
        rect,
    );
}

/// Popup z listą podpowiedzi autocomplete (gdy ≥2 kandydatów) — nad polem input.
fn render_completion_popup(f: &mut Frame, app: &App, input_area: Rect) {
    let Some(comp) = app.active_completion() else { return };
    if comp.candidates.len() < 2 {
        return; // pojedyncze dopasowanie pokazuje tylko ghost text
    }
    let n = comp.candidates.len();
    let selected = app.completion_index % n;
    let items: Vec<ListItem> = comp
        .candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:<10}", c.value), style),
                Span::styled(c.desc, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let height = (n as u16) + 2;
    let width = 44u16.min(input_area.width);
    let x = input_area.x;
    let y = input_area.y.saturating_sub(height);
    let rect = Rect::new(x, y, width, height);
    f.render_widget(Clear, rect);
    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" suggestions (Tab=accept ↑↓=select Esc=close) ")
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        rect,
    );
}

/// Overlay pomocy: skróty klawiszowe + komendy specjalne parley.
fn render_help_popup(f: &mut Frame, area: Rect) {
    let key_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let hdr_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);

    let header = |s: &str| Line::from(Span::styled(format!("  {s}"), hdr_style));
    let row = |k: &str, d: &str| {
        Line::from(vec![
            Span::styled(format!("  {k:<16}"), key_style),
            Span::raw(d.to_string()),
        ])
    };

    let lines = vec![
        header("Keybindings"),
        row("Tab", "switch focus (claude / codex)"),
        row("Ctrl+]", "passthrough mode (keys go straight to agent)"),
        row("Ctrl+R", "restart focused agent"),
        row("Enter", "send input"),
        row("Shift+Enter / Ctrl+J", "insert a new line"),
        row("Up / Down", "browse prompt history"),
        row("Esc", "abort auto mode / clear input"),
        row("Backspace", "delete last character"),
        row("Ctrl+C / Ctrl+Q", "quit (with confirmation)"),
        row("?", "show this help (when input is empty)"),
        Line::raw(""),
        header("Autocomplete (@… or /…)"),
        row("Tab", "accept highlighted suggestion"),
        row("Up / Down", "select suggestion"),
        row("Esc", "close suggestions"),
        Line::raw(""),
        header("Passthrough mode"),
        row("Ctrl+]", "back to input mode"),
        Line::raw(""),
        header("Peer message popup"),
        row("y / Enter", "approve"),
        row("n / Esc", "reject"),
        Line::raw(""),
        header("Parley commands (type in input)"),
        row("@claude <msg>", "send to claude"),
        row("@codex <msg>", "send to codex"),
        row("@all <msg>", "send to both agents"),
        row("/auto N", "auto-approve next N peer messages"),
        row("/auto off", "disable auto mode"),
        row("/discuss [N] <topic>", "start a peer discussion from focused agent"),
        row("/help  /?", "show this help"),
        row("!<command>", "run a shell command (output to timeline)"),
        Line::raw(""),
        Line::from(Span::styled("  press any key to close", dim)),
    ];

    let rect = centered_rect(72, lines.len() as u16 + 2, area);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" help ")
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

fn render_quit_popup(f: &mut Frame, area: Rect) {
    let rect = centered_rect(44, 5, area);
    f.render_widget(Clear, rect);
    let text = "Quit parley?\n\nCtrl+C / y = quit    any other key = stay";
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" quit ")
        .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(
        Paragraph::new(text).alignment(ratatui::layout::Alignment::Center).block(block),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_height_single_line_is_min() {
        assert_eq!(input_height("", 80), 3);
        assert_eq!(input_height("hello", 80), 3);
    }

    #[test]
    fn input_height_wraps_long_line() {
        // 25 znaków przy szerokości 10 → 3 wiersze treści → 5 z ramką
        assert_eq!(input_height(&"x".repeat(25), 10), 5);
    }

    #[test]
    fn input_height_counts_explicit_newlines() {
        // 3 logiczne linie → 3 wiersze → 5 z ramką
        assert_eq!(input_height("a\nb\nc", 80), 5);
    }

    #[test]
    fn input_height_clamps_to_max() {
        assert_eq!(input_height(&"y\n".repeat(20), 80), INPUT_MAX_H);
    }

    #[test]
    fn input_height_zero_width_does_not_panic() {
        // szerokość 0 traktowana jak 1 — bez paniki, wynik w dozwolonym zakresie
        let h = input_height("abc", 0);
        assert!((INPUT_MIN_H..=INPUT_MAX_H).contains(&h));
    }

    #[test]
    fn areas_keep_pane_height_independent_of_input() {
        let area = Rect::new(0, 0, 100, 40);
        let a3 = areas(area, 3);
        let a8 = areas(area, 8);
        // panele (a więc rozmiar PTY) niezależne od wysokości inputu
        assert_eq!(a3.claude.height, a8.claude.height);
        // input rośnie kosztem timeline
        assert_eq!(a8.input.height, 8);
        assert_eq!(a3.timeline.height, a8.timeline.height + 5);
    }
}
