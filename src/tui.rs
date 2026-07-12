// tui.rs — 全屏终端文档阅读器
// 极简无边框设计：无顶栏、无状态栏、无边框线，靠空行分割

use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Clear, Paragraph},
    Terminal,
};
use std::io::{stdout, IsTerminal};

use crate::display::render_markdown;

const HELP: &str = "\
──────────────── 快捷键 ────────────────
↑↓ / jk            滚动
PgUp / PgDn        翻半页
g / G              到顶 / 到底
/                  搜索
n / N              下 / 上一个匹配
q / Esc            退出
?                  关闭此帮助
────────────────────────────────────────";

/// 显示文档：终端中进入全屏 TUI，非终端直接打印（ANSI 渲染）
pub fn show_document(body: &str, hints: &[&str]) -> Result<(), String> {
    if !stdout().is_terminal() {
        render_markdown(body);
        for h in hints {
            println!("💡 {h}");
        }
        return Ok(());
    }

    let mut terminal = match init_terminal() {
        Ok(t) => t,
        Err(e) => {
            render_markdown(body);
            for h in hints {
                println!("💡 {h}");
            }
            return Err(e);
        }
    };

    let result = run(&mut terminal, body, hints);
    let _ = restore_terminal(&mut terminal);
    result
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<std::io::Stdout>>, String> {
    enable_raw_mode().map_err(|e| format!("无法进入原始模式: {e}"))?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen).map_err(|e| format!("无法进入备用屏幕: {e}"))?;
    Terminal::new(CrosstermBackend::new(out)).map_err(|e| format!("无法创建终端: {e}"))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<(), String> {
    disable_raw_mode().map_err(|e| format!("无法退出原始模式: {e}"))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|e| format!("无法退出备用屏幕: {e}"))
}

struct App {
    lines: Vec<Line<'static>>,
    scroll: usize,

    search_mode: bool,
    search_buf: String,
    matches: Vec<usize>,
    match_idx: Option<usize>,

    show_help: bool,
}

impl App {
    fn rebuild_search(&mut self) {
        let query = self.search_buf.to_lowercase();
        self.matches.clear();
        self.match_idx = None;
        if query.is_empty() {
            return;
        }
        for (i, line) in self.lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            if text.to_lowercase().contains(&query) {
                self.matches.push(i);
            }
        }
        if !self.matches.is_empty() {
            self.match_idx = Some(0);
            self.scroll = self.matches[0];
        }
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    body: &str,
    hints: &[&str],
) -> Result<(), String> {
    let mut lines = markdown_to_lines(body);
    lines.push(Line::from(Span::raw(String::new())));
    for h in hints {
        lines.push(Line::from(Span::raw(format!("💡 {h}"))));
    }

    let mut app = App {
        lines,
        scroll: 0,
        search_mode: false,
        search_buf: String::new(),
        matches: Vec::new(),
        match_idx: None,
        show_help: false,
    };

    loop {
        let size = terminal.size().map_err(|e| format!("获取终端大小失败: {e}"))?;
        let content_height = if app.search_mode {
            size.height.saturating_sub(1) as usize
        } else {
            size.height as usize
        };
        let max_scroll = app.lines.len().saturating_sub(content_height);

        terminal
            .draw(|f| ui(f, &app, content_height))
            .map_err(|e| format!("绘制失败: {e}"))?;

        // ---- event handling ----
        let ev = event::read().map_err(|e| format!("事件读取失败: {e}"))?;
        let Event::Key(key) = ev else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        if app.show_help {
            // 帮助弹层下，任意键关闭
            app.show_help = false;
            continue;
        }

        if app.search_mode {
            match key.code {
                KeyCode::Char(c) => {
                    app.search_buf.push(c);
                    app.rebuild_search();
                }
                KeyCode::Backspace => {
                    app.search_buf.pop();
                    app.rebuild_search();
                }
                KeyCode::Enter => {
                    app.search_mode = false;
                    if let Some(idx) = app.match_idx {
                        app.scroll = app.matches[idx];
                    }
                }
                KeyCode::Esc => {
                    app.search_mode = false;
                    app.search_buf.clear();
                    app.matches.clear();
                    app.match_idx = None;
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('?') => app.show_help = true,
                KeyCode::Up | KeyCode::Char('k') if app.scroll > 0 => app.scroll -= 1,
                KeyCode::Down | KeyCode::Char('j') if app.scroll < max_scroll => app.scroll += 1,
                KeyCode::PageUp => app.scroll = app.scroll.saturating_sub(content_height / 2),
                KeyCode::PageDown => {
                    app.scroll = (app.scroll + content_height / 2).min(max_scroll);
                }
                KeyCode::Home | KeyCode::Char('g') => app.scroll = 0,
                KeyCode::End | KeyCode::Char('G') => app.scroll = max_scroll,
                KeyCode::Char('/') => {
                    app.search_mode = true;
                    app.search_buf.clear();
                    app.matches.clear();
                    app.match_idx = None;
                }
                KeyCode::Char('n') if !app.matches.is_empty() => {
                    let len = app.matches.len();
                    let next = app.match_idx.map(|i| (i + 1) % len).unwrap_or(0);
                    app.match_idx = Some(next);
                    app.scroll = app.matches[next];
                }
                KeyCode::Char('N') if !app.matches.is_empty() => {
                    let len = app.matches.len();
                    let prev = app.match_idx.map(|i| (i + len - 1) % len).unwrap_or(len - 1);
                    app.match_idx = Some(prev);
                    app.scroll = app.matches[prev];
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn ui(frame: &mut ratatui::Frame, app: &App, content_height: usize) {
    let area = frame.area();

    if app.show_help {
        let help_area = centered_rect(44, 11, area);
        frame.render_widget(Clear, help_area);
        frame.render_widget(
            Paragraph::new(Text::from(HELP))
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Yellow)),
            help_area,
        );
        return;
    }

    // visible range
    let end = (app.scroll + content_height).min(app.lines.len());
    let query = app.search_buf.to_lowercase();
    let has_query = !query.is_empty();

    let mut display: Vec<Line> = Vec::with_capacity(content_height);
    for i in app.scroll..end {
        let line = &app.lines[i];
        if has_query && app.matches.contains(&i) {
            let styled: Vec<Span> = line
                .spans
                .iter()
                .map(|s| {
                    Span::styled(
                        s.content.clone(),
                        Style::default().bg(Color::Yellow).fg(Color::Black),
                    )
                })
                .collect();
            display.push(Line::from(styled));
        } else {
            display.push(line.clone());
        }
    }

    frame.render_widget(
        Paragraph::new(Text::from(display)),
        Rect {
            height: content_height as u16,
            ..area
        },
    );

    // search input bar (last line)
    if app.search_mode {
        let bar = Rect {
            x: area.x,
            y: area.y + content_height as u16,
            width: area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Text::from(Line::from(Span::raw(format!(
                "/{}",
                app.search_buf
            )))))
            .style(Style::default().fg(Color::Yellow)),
            bar,
        );
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

// ============================================================
// Markdown → 带样式的 Line 转换（与 display::ansi_format 逻辑对称）
// ============================================================

/// 将 Markdown 文本转换为 ratatui 带样式的 Line 向量
fn markdown_to_lines(body: &str) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut in_code_block = false;

    for raw in body.lines() {
        let trimmed = raw.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            out.push(Line::from(Span::styled(
                raw.to_string(),
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        if trimmed.is_empty() {
            out.push(Line::from(Span::raw(String::new())));
            continue;
        }

        if is_hrule(trimmed) && raw.len() >= 3 {
            out.push(Line::from(Span::styled(
                "─".repeat(raw.len().min(80)),
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        if trimmed.starts_with("###") {
            let content = &trimmed[3..].trim();
            let spans = parse_inline(content);
            let styled: Vec<Span> = spans.into_iter().map(|s| {
                s.style(Style::default().add_modifier(Modifier::BOLD).add_modifier(Modifier::ITALIC))
            }).collect();
            out.push(Line::from(styled));
            continue;
        }

        if trimmed.starts_with("##") {
            let content = &trimmed[2..].trim();
            let spans = parse_inline(content);
            let styled: Vec<Span> = spans.into_iter().map(|s| {
                s.style(Style::default().add_modifier(Modifier::BOLD))
            }).collect();
            out.push(Line::from(styled));
            continue;
        }

        let first_char = trimmed.chars().next().unwrap_or(' ');
        if first_char == '#' {
            let content = &trimmed[1..].trim();
            let spans = parse_inline(content);
            let styled: Vec<Span> = spans.into_iter().map(|s| {
                s.style(Style::default().add_modifier(Modifier::BOLD))
            }).collect();
            out.push(Line::from(styled));
            continue;
        }

        if trimmed.starts_with("> ") || trimmed == ">" {
            let content = if trimmed.len() > 2 { &trimmed[2..] } else { "" };
            let mut spans = vec![Span::styled("│ ", Style::default().fg(Color::DarkGray))];
            spans.extend(parse_inline(content));
            out.push(Line::from(spans));
            continue;
        }

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let mut spans = vec![Span::raw("• ")];
            spans.extend(parse_inline(&trimmed[2..]));
            out.push(Line::from(spans));
            continue;
        }

        if is_ordered_list(trimmed) {
            let dot = trimmed.find('.').unwrap_or(1);
            let prefix = &trimmed[..=dot];
            let content = &trimmed[dot + 1..].trim();
            let mut spans = vec![Span::raw(prefix.to_string() + " ")];
            spans.extend(parse_inline(content));
            out.push(Line::from(spans));
            continue;
        }

        out.push(Line::from(parse_inline(raw)));
    }

    out
}

fn is_hrule(s: &str) -> bool {
    let no_spaces: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if no_spaces.len() < 3 {
        return false;
    }
    let first = no_spaces.chars().next().unwrap();
    (first == '-' || first == '*' || first == '_')
        && no_spaces.chars().all(|c| c == first)
}

fn is_ordered_list(s: &str) -> bool {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.is_empty() { return false; }
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
    i > 0 && i < bytes.len() && bytes[i] == b'.'
}

/// 解析行内 Markdown 格式（code、加粗、斜体、删除线、链接）
fn parse_inline(text: &str) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                spans.push(Span::raw(std::mem::take(&mut buf)));
            }
        };
    }

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() && "`*~[".contains(chars[i + 1]) {
            buf.push(chars[i + 1]);
            i += 2;
            continue;
        }

        if chars[i] == '`' {
            if let Some(end) = find_closing(&chars, i + 1, "`") {
                flush!();
                let inner: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(inner, Style::default().fg(Color::Red)));
                i = end + 1;
                continue;
            }
        }

        if chars[i] == '~' && i + 1 < chars.len() && chars[i + 1] == '~' {
            if let Some(end) = find_closing(&chars, i + 2, "~~") {
                flush!();
                let inner: String = chars[i + 2..end].iter().collect();
                for s in parse_inline(&inner) {
                    spans.push(s.style(Style::default().add_modifier(Modifier::CROSSED_OUT)));
                }
                i = end + 2;
                continue;
            }
        }

        if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            if let Some(end) = find_closing(&chars, i + 2, "**") {
                flush!();
                let inner: String = chars[i + 2..end].iter().collect();
                for s in parse_inline(&inner) {
                    spans.push(s.style(Style::default().add_modifier(Modifier::BOLD)));
                }
                i = end + 2;
                continue;
            }
        }

        if chars[i] == '*' && (i == 0 || chars[i - 1] != '*')
            && (i + 1 >= chars.len() || chars[i + 1] != '*')
        {
            if let Some(end) = find_closing(&chars, i + 1, "*") {
                if end + 1 >= chars.len() || chars[end + 1] != '*' {
                    flush!();
                    let inner: String = chars[i + 1..end].iter().collect();
                    for s in parse_inline(&inner) {
                        spans.push(s.style(Style::default().add_modifier(Modifier::ITALIC)));
                    }
                    i = end + 1;
                    continue;
                }
            }
        }

        if chars[i] == '[' {
            let be = find_char(&chars, i + 1, ']');
            let ps = be.and_then(|b| {
                if b + 1 < chars.len() && chars[b + 1] == '(' {
                    Some(b + 1)
                } else {
                    None
                }
            });
            let pe = ps.and_then(|p| find_char(&chars, p + 1, ')'));
            if let (Some(be), Some(_), Some(pe)) = (be, ps, pe) {
                flush!();
                let inner: String = chars[i + 1..be].iter().collect();
                spans.extend(parse_inline(&inner));
                i = pe + 1;
                continue;
            }
        }

        buf.push(chars[i]);
        i += 1;
    }

    flush!();
    spans
}

fn find_closing(chars: &[char], start: usize, pattern: &str) -> Option<usize> {
    let p: Vec<char> = pattern.chars().collect();
    let mut i = start;
    while i + p.len() <= chars.len() {
        if chars[i..i + p.len()] == p[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    chars[start..].iter().position(|&c| c == target).map(|p| start + p)
}
