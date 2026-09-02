// editor.rs — raw 模式多行行编辑器 + 斜杠命令候选抽屉
//
// 替代 run_repl 原先的 `io::stdin().read_line()` 整行读取：
// - 逐键读取（crossterm event::read），实时响应按键
// - 支持普通字符 / CJK / 退格 / 方向键 / Home / End / Del
// - Ctrl+J 换行（多行输入）
// - 以 '/' 开头时，在输入区下方弹出抽屉式内置命令候选（↑↓ 选择，Enter/Tab 补全，Esc 关闭）
//
// 渲染策略：
//   编辑区以「进入编辑时光标所在行」为锚点（anchor_row，绝对行号）。
//   每次按键后重绘：先跳到 anchor_row 并 `Clear(FromCursorDown)` 清空下方残影，
//   再重绘全部输入行，随后在输入区下方绘制抽屉（若有），最后把硬件光标移回插入点。
//   因为 anchor_row 下方就是终端空白区，FromCursorDown 清理是安全的。

use std::io::{self, Write};

use crossterm::{
    cursor::{MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, size, Clear, ClearType},
};

// ============================================================
// 常量与样式
// ============================================================

/// 内置命令候选（抽屉内容）。顺序即显示顺序。
pub const COMMANDS: &[&str] = &["/help", "/model", "/clear", "/truncate", "/exit", "/yolo"];

const PROMPT: &str = "\x1b[34m> \x1b[0m";
/// YOLO 模式提示符：黄色 ! + 空格（与 "> " 等宽，避免 emoji 宽度歧义导致不对齐）
const PROMPT_YOLO: &str = "\x1b[33m!\x1b[0m ";
const PROMPT_WIDTH: usize = 2; // "> " 与 "! " 的字符宽度（两者等宽）

/// 按会话状态返回提示符字符串与显示宽度
fn prompt_of(yolo: bool) -> (&'static str, usize) {
    if yolo {
        (PROMPT_YOLO, PROMPT_WIDTH)
    } else {
        (PROMPT, PROMPT_WIDTH)
    }
}

const DRAWER_BG: &str = "\x1b[48;5;208m\x1b[30m"; // 选中项：橙底黑字
const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

// ============================================================
// 字符宽度与布局
// ============================================================

/// 单个字符的显示宽度估算（CJK 宽字符=2，制表符=4，其余=1，换行=0）
fn char_width(c: char) -> usize {
    if c == '\t' {
        return 4;
    }
    if c == '\n' {
        return 0;
    }
    // East Asian Wide / Fullwidth 范围
    if ('\u{2E80}'..='\u{9FFF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
        || ('\u{FE30}'..='\u{FE4F}').contains(&c)
        || ('\u{FF00}'..='\u{FF60}').contains(&c)
        || ('\u{FFE0}'..='\u{FFE6}').contains(&c)
        || ('\u{20000}'..='\u{2FFFD}').contains(&c)
    {
        return 2;
    }
    1
}

/// 把缓冲区布局为若干「显示行」，返回每行「在缓冲区中的起始字节」。
/// 第 0 行起点多出 prompt 宽度；超宽自动软换行。
fn layout_starts(buf: &str, term_width: usize, pw: usize) -> Vec<usize> {
    let mut starts: Vec<usize> = Vec::new();
    let mut col = pw;
    let mut line_start = 0usize;
    let mut i = 0usize;
    while i < buf.len() {
        let c = buf[i..].chars().next().unwrap();
        if c == '\n' {
            // 一行结束（其内容从 line_start 到 i 前）
            starts.push(line_start);
            i += 1;
            line_start = i;
            col = 0;
            continue;
        }
        let w = char_width(c);
        if col + w > term_width {
            starts.push(line_start);
            line_start = i;
            col = 0;
        }
        col += w;
        i += c.len_utf8();
    }
    starts.push(line_start);
    starts
}

/// 光标处的 (显示行索引, 显示列) —— 由 buf[..cursor] 的布局得出
fn cursor_pos(buf: &str, cursor: usize, term_width: usize, pw: usize) -> (usize, usize) {
    let prefix = &buf[..cursor];
    let starts = layout_starts(prefix, term_width, pw);
    let row = starts.len().saturating_sub(1);
    let line_start = *starts.last().unwrap_or(&0);
    let seg = &prefix[line_start..];
    let seg_width = seg.chars().map(char_width).sum::<usize>();
    // 仅「最初那一行的首个显示行」在行首有 prompt 前缀；其余（软换行/后续逻辑行）从列 0 起
    let col = if line_start == 0 {
        pw + seg_width
    } else {
        seg_width
    };
    (row, col)
}

// ============================================================
// 行编辑器状态
// ============================================================

/// 抽屉一帧的视觉快照：候选列表 + 选中项。
/// 用于判断本次渲染的抽屉是否与上次一致，一致则无需重绘。
#[derive(Clone, PartialEq)]
struct DrawerState {
    matches: Vec<String>,
    sel: usize,
}

/// 行编辑器内部状态
struct Editor {
    buf: String,
    cursor: usize, // 字节偏移
    sel: usize,    // 抽屉选中项索引
    prev_filt: String, // 上次的过滤前缀（用于前缀变化时重置选中项）
    dismissed: bool, // 用户按 Esc 关闭抽屉后的临时隐藏开关
    // 上次帧的快照，用于选择性重绘（避免抽屉被反复重绘闪烁）
    last_drawer: Option<DrawerState>, // 上次抽屉内容（None = 上次抽屉未打开）
    last_in_lines: usize,             // 上次输入区的显示行数
    last_buf: String,                 // 上次缓冲文本（用于跳过完全未变化的帧，如按键 Repeat）
    last_cursor: usize,               // 上次光标字节偏移
}

impl Editor {
    fn new() -> Self {
        Editor {
            buf: String::new(),
            cursor: 0,
            sel: 0,
            prev_filt: String::new(),
            dismissed: false,
            last_drawer: None,
            last_in_lines: 0,
            last_buf: String::new(),
            last_cursor: 0,
        }
    }

    /// 在光标处插入一个字符
    fn insert(&mut self, c: char) {
        if c == '\r' {
            return;
        }
        self.buf.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        if self.buf.starts_with('/') {
            self.dismissed = false;
        }
    }

    /// 退格：删除光标前的一个字符
    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.buf[..self.cursor].chars().next_back().unwrap();
        self.cursor -= prev.len_utf8();
        self.buf.remove(self.cursor);
        if self.buf.starts_with('/') {
            self.dismissed = false;
        }
    }

    /// 删除光标后的一个字符
    fn delete(&mut self) {
        if self.cursor >= self.buf.len() {
            return;
        }
        let c = self.buf[self.cursor..].chars().next().unwrap();
        self.buf.remove(self.cursor);
        let _ = c;
        if self.buf.starts_with('/') {
            self.dismissed = false;
        }
    }

    // ---- 斜杠候选 ----

    /// 当前过滤前缀：'/' 后的第一个「词」
    fn filter(&self) -> String {
        if !self.buf.starts_with('/') {
            return String::new();
        }
        self.buf[1..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    }

    /// 与当前过滤前缀匹配的内置命令列表
    fn matches(&self) -> Vec<String> {
        let f = self.filter();
        COMMANDS
            .iter()
            .filter(|cmd| f.is_empty() || cmd.starts_with(&format!("/{}", f)))
            .map(|s| s.to_string())
            .collect()
    }

    /// 是否应显示抽屉
    fn menu_should_open(&self) -> bool {
        !self.dismissed && self.buf.starts_with('/') && !self.matches().is_empty()
    }

    /// 把当前斜杠 token 替换为选中的完整命令
    fn apply_selection(&mut self) {
        let m = self.matches();
        if m.is_empty() {
            return;
        }
        let idx = self.sel.min(m.len() - 1);
        let cmd = m[idx].clone();
        // 找到第一个空白位置：若存在则保留其后的参数
        let end = self.buf.find(char::is_whitespace).unwrap_or(self.buf.len());
        let rest = self.buf[end..].to_string();
        self.buf = format!("{}{}", cmd, rest);
        self.cursor = cmd.len();
        self.dismissed = false;
        self.prev_filt.clear(); // 关闭后清空过滤记录
    }

    /// 抽屉打开期间同步选中项：过滤前缀变化则重置到 0；不再以 / 开头则解除隐藏
    fn sync_selection(&mut self) {
        let f = self.filter();
        if !self.buf.starts_with('/') {
            // 不再是以 / 开头的命令，解除 Escape 隐藏并复位
            self.dismissed = false;
            self.sel = 0;
            self.prev_filt = f;
            return;
        }
        if f != self.prev_filt {
            self.prev_filt = f;
            self.sel = 0;
        }
        let n = self.matches().len();
        if n > 0 && self.sel >= n {
            self.sel = n - 1;
        }
    }

    /// 本帧是否「完全未变化」（缓冲、光标、抽屉、输入高度全部与上次一致）。
    /// 用于跳过整帧渲染——例如按住键触发的 `Repeat` 事件，输入与抽屉都没变，
    /// 无需向终端写任何字节。
    fn frame_unchanged(&self, in_lines: usize, m: &[String]) -> bool {
        if self.buf != self.last_buf || self.cursor != self.last_cursor {
            return false;
        }
        if in_lines != self.last_in_lines {
            return false;
        }
        let open = self.menu_should_open();
        let cur = if open {
            Some(DrawerState {
                matches: m.to_vec(),
                sel: self.sel.min(m.len().saturating_sub(1)),
            })
        } else {
            None
        };
        cur == self.last_drawer
    }

    /// 抽屉在本帧是否「视觉上变化」了，需要重新整体绘制（输入 + 抽屉）。
    /// 只有当候选列表、选中项、打开状态或输入显示行数（影响抽屉位置）任一变化时才为 true。
    /// 若为 false，抽屉行可以与上次保持原样不重绘，从而避免 `/xxx` 输入时抽屉被反复刷新。
    fn drawer_changed(&self, in_lines: usize, m: &[String]) -> bool {
        let open = self.menu_should_open();
        let cur = if open {
            Some(DrawerState {
                matches: m.to_vec(),
                sel: self.sel.min(m.len().saturating_sub(1)),
            })
        } else {
            None
        };
        // 抽屉位置随输入显示行数下移：输入高度变化则整体重绘
        if in_lines != self.last_in_lines {
            return true;
        }
        cur != self.last_drawer
    }

    /// 作废上一帧快照，强制下一帧完整重绘。
    /// 用于窗口 Resize 等终端状态突变场景：布局参数已失效，
    /// 旧快照的比较结果不再可信，必须整体重画一次。
    fn reset_snapshots(&mut self) {
        self.last_drawer = None;
        self.last_in_lines = usize::MAX; // 强制判定「输入高度已变化」→ 走完整重绘
        self.last_buf.clear();
        self.last_cursor = usize::MAX;
    }

    /// 记录本帧渲染所用的抽屉/输入快照，供下一帧判定变化。
    fn record_frame(&mut self, in_lines: usize, m: &[String]) {
        self.last_in_lines = in_lines;
        self.last_buf = self.buf.clone();
        self.last_cursor = self.cursor;
        if self.menu_should_open() {
            self.last_drawer = Some(DrawerState {
                matches: m.to_vec(),
                sel: self.sel.min(m.len().saturating_sub(1)),
            });
        } else {
            self.last_drawer = None;
        }
    }
}

// ============================================================
// 渲染
// ============================================================

/// 渲染一行候选（选中项铺满背景色）
fn draw_drawer_row<W: Write>(
    out: &mut W,
    text: &str,
    selected: bool,
    term_width: usize,
) -> io::Result<()> {
    let mut line = String::new();
    let tw = text.chars().map(char_width).sum::<usize>();
    if selected {
        line.push_str(DRAWER_BG);
        line.push_str("  ");
        line.push_str(text);
        line.push_str("  ");
        for _ in 0..term_width.saturating_sub(tw + 4) {
            line.push(' ');
        }
        line.push_str(RESET);
    } else {
        line.push_str(DIM);
        line.push('\u{2502}'); // │
        line.push(' ');
        line.push_str(text);
        for _ in 0..term_width.saturating_sub(tw + 2) {
            line.push(' ');
        }
        line.push_str(RESET);
    }
    write!(out, "{}\r\n", line)?;
    Ok(())
}

/// 渲染整个编辑区（输入 + 抽屉），并放置硬件光标。
/// 若 `place_cursor` 为 false，则绘制后不把光标移回插入点（用于提交后的最终留影）。
/// 写入目标泛型化，便于测试时捕获字节流重放进终端模型。
///
/// 采用选择性重绘以减少不必要的终端写入：
/// - 抽屉状态（候选/选中/开关/输入高度）变化 → 完整重绘输入 + 抽屉；
/// - 仅输入文字变化、抽屉未变 → 只重绘输入行（逐行 `Clear(CurrentLine)` 清残影，不触碰下方抽屉）。
/// 这样 `woman ai` 里输入 `/help` 这类命令时，抽屉只在其候选真正变化时才刷新，
/// 不再随每个按键被整块重绘闪烁。
fn render<W: Write>(
    ed: &mut Editor,
    anchor_row: usize,
    term_width: usize,
    yolo: bool,
    out: &mut W,
    place_cursor: bool,
) -> io::Result<()> {
    let (prompt, pw) = prompt_of(yolo);
    let m = ed.matches();
    let in_lines = layout_starts(&ed.buf, term_width, pw).len();

    // 整帧与上次完全一致（如按键 Repeat）：无需向终端写任何字节
    if ed.frame_unchanged(in_lines, &m) {
        return Ok(());
    }

    // 需要完整重绘（抽屉状态或输入高度有变）
    let full = ed.drawer_changed(in_lines, &m);

    if full {
        // 跳到锚点行首并清空其下全部残影（含抽屉）
        // 注意 crossterm MoveTo 参数顺序为 (列, 行)
        execute!(
            out,
            MoveTo(0, anchor_row as u16),
            Clear(ClearType::FromCursorDown)
        )?;

        // 打印 prompt + 缓冲区（内部换行由终端处理，超宽自动软换行）
        // 注意：raw 模式下 \n 不会自动回行首，需显式转为 \r\n
        write!(out, "{}", prompt)?;
        let rendered_buf = ed.buf.replace('\n', "\r\n");
        write!(out, "{}", rendered_buf)?;

        // 抽屉：在输入区下方空一行后绘制
        if ed.menu_should_open() {
            write!(out, "\r\n")?; // 输入区与抽屉之间空一行（raw 模式显式 CRLF）
            for i in 0..m.len() {
                draw_drawer_row(out, &m[i], i == ed.sel.min(m.len() - 1), term_width)?;
            }
        }
    } else {
        // 抽屉未变：只重绘输入行，不触碰下方的抽屉。
        // 不能用 Clear(FromCursorDown)（会把抽屉一起清掉），改为对输入区覆盖的每一行
        // 分别执行 MoveTo + Clear(CurrentLine) 清掉旧残影，再从锚点重写输入。
        // 输入高度稳定（否则走 full 分支），故清行范围与抽屉区域不重叠。
        for k in 0..in_lines {
            if let Err(e) = execute!(
                out,
                MoveTo(0, (anchor_row + k) as u16),
                Clear(ClearType::CurrentLine)
            ) {
                return Err(e);
            }
        }
        if let Err(e) = execute!(out, MoveTo(0, anchor_row as u16)) {
            return Err(e);
        }
        let rendered_buf = ed.buf.replace('\n', "\r\n");
        write!(out, "{}", PROMPT)?;
        write!(out, "{}", rendered_buf)?;
    }

    // 把硬件光标移回插入位置（仅在编辑过程中需要）
    if place_cursor {
        let (cur_row, cur_col) = cursor_pos(&ed.buf, ed.cursor, term_width, pw);
        execute!(out, MoveTo(cur_col as u16, (anchor_row + cur_row) as u16))?;
    }

    // 记录本帧快照，供下一帧判定
    ed.record_frame(in_lines, &m);
    Ok(())
}

// ============================================================
// 预滚动
// ============================================================

/// 估算本帧将占用的屏幕行数（输入行 + 抽屉 + 边距），
/// 若会越过屏幕底部，则在底行主动换行触发终端上滚，并下调锚点。
/// 返回校准后的锚点行号。
///
/// 行数构成（与 render 的写入量对应）：
/// - 输入区 `in_lines` 行；
/// - 抽屉打开时：输入区与抽屉间空行 1 + 候选 `m.len()` 行 + 末候选行尾的 `\r\n` 1；
/// - 始终再留 1 行边距，吸收底行列溢出（DECAWM 自动换行）引发的滚动。
fn ensure_space<W: Write>(
    out: &mut W,
    ed: &Editor,
    anchor: usize,
    term_width: usize,
    yolo: bool,
) -> usize {
    let rows = size()
        .ok()
        .map(|(_, h)| h as usize)
        .unwrap_or(24)
        .max(3);
    let m = ed.matches();
    let in_lines = layout_starts(&ed.buf, term_width, prompt_of(yolo).1).len();
    let drawer_extra = if ed.menu_should_open() { m.len() + 2 } else { 0 };
    let needed = in_lines + drawer_extra + 1;

    if anchor + needed < rows {
        return anchor; // 空间足够，无需滚动
    }

    let deficit = anchor + needed + 1 - rows;
    // 移到底行写入 deficit 个换行：终端上滚 deficit 行，内容整体上移
    let _ = execute!(out, MoveTo(0, (rows - 1) as u16));
    for _ in 0..deficit {
        let _ = write!(out, "\r\n");
    }
    let _ = out.flush();
    anchor.saturating_sub(deficit)
}

// ============================================================
// 主入口
// ============================================================

/// 在 raw 模式下读取一段多行输入。
/// 返回 `Some(去首尾空白的字符串)`；
/// 空缓冲时按 Ctrl+D 返回 `None`（退出会话）；无法进入 raw 模式也返回 `None`。
pub fn read_input(yolo: bool) -> Option<String> {
    // 先记录进入编辑前硬件光标所在行（作为重绘锚点），再进入 raw 模式
    let mut anchor = crossterm::cursor::position()
        .ok()
        .map(|p| p.1 as usize)
        .unwrap_or(0);

    enable_raw_mode().ok()?;
    // 注意：不隐藏硬件光标。render 会把光标放到插入点，
    // 用户依赖它定位输入位置；隐藏后整个编辑过程光标不可见。

    // 确保终端自动换行开启（DECAWM），避免长行不换行、编辑区布局与光标错位
    write!(io::stdout(), "\x1b[?7h").ok();
    let _ = io::stdout().flush();

    let mut term_width = size().ok().map(|(w, _)| (w as usize).max(20)).unwrap_or(80);

    let mut out = io::stdout();
    let mut ed = Editor::new();

    let result: Option<String> = loop {
        ed.sync_selection();

        // 预滚动：若本帧（输入 + 抽屉 + 边距）会超出屏幕底部，
        // 先在底行主动写入换行触发终端上滚，并同步下调锚点。
        // 否则终端自动上滚会把已画内容整体上移，而 anchor 的绝对行号失效，
        // 后续重绘全部错位 → 残影 / 重叠。
        anchor = ensure_space(&mut out, &ed, anchor, term_width, yolo);

        render(&mut ed, anchor, term_width, yolo, &mut out, true).ok()?;

        let ev = match event::read() {
            Ok(e) => e,
            Err(_) => break Some(ed.buf.trim().to_string()),
        };
        let (code, modifiers) = match ev {
            // 窗口尺寸变化：更新宽度、夹回锚点、作废快照后整体重绘
            Event::Resize(w, _) => {
                term_width = (w as usize).max(20);
                let rows = size()
                    .ok()
                    .map(|(_, h)| h as usize)
                    .unwrap_or(24)
                    .max(1);
                anchor = anchor.min(rows - 1);
                ed.reset_snapshots();
                continue;
            }
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) => {
                if kind != KeyEventKind::Press {
                    continue;
                }
                (code, modifiers)
            }
            _ => continue, // 鼠标等其他事件忽略
        };

        match (code, modifiers) {
            // ---- 提交 ----
            (KeyCode::Enter, _) => {
                if ed.menu_should_open() {
                    let m = ed.matches();
                    let idx = ed.sel.min(m.len() - 1);
                    // 若首 token 已是选中的完整命令 → 直接提交（回车补全后的第二次回车）
                    let token = ed.buf.split_whitespace().next().unwrap_or("");
                    if token == m[idx] {
                        break Some(ed.buf.trim().to_string());
                    } else {
                        ed.apply_selection();
                    }
                } else {
                    break Some(ed.buf.trim().to_string());
                }
            }

            // ---- Ctrl+D 退出：缓冲为空时结束会话（返回 None），
            //      非空时按 readline 惯例向前删除一个字符 ----
            (KeyCode::Char('d'), m) if m.contains(KeyModifiers::CONTROL) => {
                if ed.buf.is_empty() {
                    break None;
                } else {
                    ed.delete();
                }
            }

            // ---- Ctrl+J 换行（多行输入）。Windows 下为 Char('j')+CONTROL，兼容 Char('\n') ----
            (KeyCode::Char('j'), m) if m.contains(KeyModifiers::CONTROL) => ed.insert('\n'),
            (KeyCode::Char('\n'), _) => ed.insert('\n'),

            // ---- 退格 / 删除 ----
            (KeyCode::Backspace, _) => ed.backspace(),
            (KeyCode::Delete, _) => ed.delete(),

            // ---- 光标移动（抽屉打开时禁用以避免误触）----
            (KeyCode::Left, _) if !ed.menu_should_open() && ed.cursor > 0 => {
                let prev = ed.buf[..ed.cursor].chars().next_back().unwrap();
                ed.cursor -= prev.len_utf8();
            }
            (KeyCode::Right, _) if !ed.menu_should_open() && ed.cursor < ed.buf.len() => {
                let c = ed.buf[ed.cursor..].chars().next().unwrap();
                ed.cursor += c.len_utf8();
            }
            (KeyCode::Home, _) if !ed.menu_should_open() => ed.cursor = 0,
            (KeyCode::End, _) if !ed.menu_should_open() => ed.cursor = ed.buf.len(),

            // ---- 抽屉导航 ----
            (KeyCode::Up, _) if ed.menu_should_open() && ed.sel > 0 => ed.sel -= 1,
            (KeyCode::Down, _) if ed.menu_should_open() => {
                let n = ed.matches().len();
                if n > 0 && ed.sel + 1 < n {
                    ed.sel += 1;
                }
            }
            (KeyCode::Up, _) => ed.cursor = 0,
            (KeyCode::Down, _) => ed.cursor = ed.buf.len(),

            // ---- Tab 补全 ----
            (KeyCode::Tab, _) if ed.menu_should_open() => ed.apply_selection(),

            // ---- Esc：关闭抽屉 ----
            (KeyCode::Esc, _) if ed.menu_should_open() => ed.dismissed = true,

            // ---- 普通字符 ----
            (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => ed.insert(c),

            _ => {}
        }
    };

    // 清理编辑区残影（不把光标移回插入点，让后续输出从输入末尾另起一行），恢复终端
    // 提交按键可能改变了缓冲（如 Tab 补全），先再做一次预滚动校准锚点
    anchor = ensure_space(&mut out, &ed, anchor, term_width, yolo);
    render(&mut ed, anchor, term_width, yolo, &mut out, false).ok()?;

    // 提交收尾：清空输入区下方（残留的抽屉等），再从输入行末尾换行，
    // 让后续输出（帮助文本/AI 回答）从输入的下一行开始，不与输入拼接
    {
        let pw = prompt_of(yolo).1;
        let (end_row, end_col) = cursor_pos(&ed.buf, ed.buf.len(), term_width, pw);
        let _ = execute!(
            out,
            MoveTo(0, (anchor + end_row + 1) as u16),
            Clear(ClearType::FromCursorDown)
        );
        let _ = execute!(out, MoveTo(end_col as u16, (anchor + end_row) as u16));
        let _ = write!(out, "\r\n");
        let _ = out.flush();
    }

    disable_raw_mode().ok()?;
    execute!(io::stdout(), Show).ok()?;

    result
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_empty() {
        // 空缓冲区 → 只有一行，起点为字节 0
        assert_eq!(layout_starts("", 80, PROMPT_WIDTH), vec![0]);
    }

    #[test]
    fn layout_single_short_line() {
        // 短单行不换行：始终一个显示行
        assert_eq!(layout_starts("abc", 80, PROMPT_WIDTH), vec![0]);
    }

    #[test]
    fn layout_newline_splits() {
        // 换行拆成两个逻辑行：第一行起点 0，第二行起点 4
        assert_eq!(layout_starts("abc\ndef", 80, PROMPT_WIDTH), vec![0, 4]);
    }

    #[test]
    fn layout_soft_wrap() {
        // term_width=5，prompt 占 2。
        // “abc” col=5 满；再加 'd' 溢出 → 软换行，第二行起点字节 3
        let buf = "abcd";
        // 'a'(3) 'b'(4) 'c'(5) 'd'(6>5→换行) → starts=[0,3]
        assert_eq!(layout_starts(buf, 5, PROMPT_WIDTH), vec![0, 3]);
    }

    #[test]
    fn cursor_after_prompt_when_empty() {
        // 空输入：光标应在第一行、prompt 之后（列 2）
        assert_eq!(cursor_pos("", 0, 80, PROMPT_WIDTH), (0, PROMPT_WIDTH));
    }

    #[test]
    fn cursor_after_short_line() {
        // 'ab' 不换行：光标在同一行、prompt 之后
        assert_eq!(cursor_pos("ab", 2, 80, PROMPT_WIDTH), (0, 2 + 2));
    }

    #[test]
    fn cursor_after_newline() {
        // 'abc\nde'：第二行没有 prompt 前缀，光标在第二行列 2（de 两个字符）
        assert_eq!(cursor_pos("abc\nde", 6, 80, PROMPT_WIDTH), (1, 2));
    }

    #[test]
    fn cursor_after_soft_wrap() {
        // term_width=5，'abcd' 第一行 'abc'(col5)，第二行 'd'
        // 光标在第二行：列应为 1（d 占 1，无反白 prompt）
        assert_eq!(cursor_pos("abcd", 4, 5, PROMPT_WIDTH), (1, 1));
    }

    #[test]
    fn cjk_width() {
        // 中文字符宽度为 2
        assert_eq!(char_width('中'), 2);
        assert_eq!(char_width('a'), 1);
    }

    #[test]
    fn drawer_filter_matches() {
        let mut ed = Editor::new();
        // 空输入不弹
        assert!(!ed.menu_should_open());
        // 输入 '/' → 显示所有命令
        ed.insert('/');
        assert!(ed.menu_should_open());
        assert_eq!(ed.matches().len(), COMMANDS.len());
        // 输入 '/m' → 只剩 /model
        ed.insert('m');
        let m = ed.matches();
        assert_eq!(m, vec!["/model"]);
        // 选择补全 → 缓冲区变成完整命令
        ed.apply_selection();
        assert_eq!(ed.buf, "/model");
        // 此时首 token 与选中项一致，Enter 可直接提交（不会陷入无限补全）
        let token = ed.buf.split_whitespace().next().unwrap_or("");
        assert_eq!(token, "/model");
    }

    #[test]
    fn apply_selection_keeps_args() {
        let mut ed = Editor::new();
        ed.insert('/');
        ed.insert('m');
        ed.insert(' ');
        ed.insert('x');
        // 此时 buf="/m x"，选择补全后应保留中间的空格与参数 " x"
        ed.apply_selection();
        assert_eq!(ed.buf, "/model x");
        assert_eq!(ed.cursor, "/model".len());
    }

    #[test]
    fn backspace_handles_cjk() {
        let mut ed = Editor::new();
        ed.insert('中');
        ed.insert('文');
        // 删除 '文'（3 字节），剩下 '中'
        ed.backspace();
        assert_eq!(ed.buf, "中");
        assert_eq!(ed.cursor, '中'.len_utf8());
    }

    // ---- 屏幕渲染模型 + 绘制重叠回归测试 ----

    /// 极简 ANSI 屏幕模拟（仅覆盖编辑器用到的序列：MoveTo/FromCursorDown/颜色/CRLF/字符）
    struct Screen {
        rows: usize,
        cols: usize,
        buf: Vec<Vec<char>>,
        r: usize,
        c: usize,
        esc: bool,
        seq: String,
    }

    impl Screen {
        fn new(rows: usize, cols: usize) -> Self {
            Screen {
                rows,
                cols,
                buf: vec![vec![' '; cols]; rows],
                r: 0,
                c: 0,
                esc: false,
                seq: String::new(),
            }
        }

        fn write(&mut self, s: &str) {
            for ch in s.chars() {
                if self.esc {
                    self.seq.push(ch);
                    if ch.is_ascii_alphabetic() {
                        let seq = std::mem::take(&mut self.seq);
                        self.apply_cmd(&seq);
                        self.esc = false;
                    }
                    continue;
                }
                if ch == '\x1b' {
                    self.esc = true;
                    self.seq.clear();
                    continue;
                }
                match ch {
                    '\r' => self.c = 0,
                    '\n' => {
                        self.r += 1;
                        self.clamp();
                    }
                    '\t' => self.c += 4,
                    _ => self.put(ch),
                }
            }
        }

        fn put(&mut self, ch: char) {
            if self.c >= self.cols {
                self.r += 1;
                self.clamp();
                self.c = 0;
            }
            self.buf[self.r][self.c] = ch;
            self.c += 1;
        }

        fn clamp(&mut self) {
            while self.r >= self.rows {
                self.buf.remove(0);
                self.buf.push(vec![' '; self.cols]);
                self.r -= 1;
            }
        }

        fn apply_cmd(&mut self, body: &str) {
            // body 形如 "[<op>" 或 "[<args><op>"
            let Some(rest) = body.strip_prefix('[') else {
                return;
            };
            if rest.is_empty() {
                return;
            }
            let op = rest.chars().last().unwrap();
            let args = &rest[..rest.len() - 1];
            match op {
                'H' => {
                    let mut parts: Vec<usize> =
                        args.split(';').map(|a| a.parse().unwrap_or(1)).collect();
                    if parts.len() == 1 {
                        parts.insert(0, 1);
                    }
                    self.r = (parts[0].saturating_sub(1)) % self.rows;
                    self.c = (parts[1].saturating_sub(1)) % self.cols;
                }
                'J' => {
                    let n: usize = if args.is_empty() {
                        0
                    } else {
                        args.parse().ok().unwrap_or(0)
                    };
                    if n == 0 {
                        for x in self.c..self.cols {
                            self.buf[self.r][x] = ' ';
                        }
                        for rr in self.r + 1..self.rows {
                            self.buf[rr] = vec![' '; self.cols];
                        }
                    }
                }
                // `Clear(CurrentLine)` → `\x1b[K`：清空当前行从光标到行尾。
                // 选择性重绘用它在「只重绘输入行」时清掉单行残影，必须被模拟。
                'K' => {
                    for x in self.c..self.cols {
                        self.buf[self.r][x] = ' ';
                    }
                }
                _ => {}
            }
        }

        /// 渲染为若干文本行（去行尾空白）
        fn lines(&self) -> Vec<String> {
            self.buf
                .iter()
                .map(|row| String::from_iter(row.iter()).trim_end().to_string())
                .collect()
        }
    }

    /// 把某一步的 render 输出喂进屏幕模型，并断言某行不包含异常拼接片段
    fn consume_render(scr: &mut Screen, ed: &mut Editor, anchor: usize, width: usize) {
        let mut sink = Vec::new();
        render(ed, anchor, width, false, &mut sink, true).unwrap();
        scr.write(&String::from_utf8_lossy(&sink));
    }

    #[test]
    fn simple_render_at_anchor_row() {
        // 基础：不与抽屉交互、不换行时，输入应恰好画在 anchor 行
        let width = 40;
        let anchor = 5;
        let mut scr = Screen::new(12, width);
        let mut ed = Editor::new();
        for ch in "abc".chars() {
            ed.insert(ch);
            consume_render(&mut scr, &mut ed, anchor, width);
        }
        let lines = scr.lines();
        assert_eq!(lines[5], "> abc", "anchor 行应有输入：\n{:?}", lines);
        // 其他行应为空
        assert_eq!(lines[4], "");
        assert_eq!(lines[6], "");
    }

    #[test]
    fn render_no_overlap_on_long_line_and_newline() {
        // 复现「PowerShell 你好」这类长输入 + Ctrl+J 换行 + 退格回退，绘制不应错位/残留
        let width = 20; // 很窄，使输入必然软换行
        let anchor = 3;
        let mut scr = Screen::new(10, width);
        let mut ed = Editor::new();

        // 逐个字符输入
        for ch in "PowerShell 你好".chars() {
            ed.insert(ch);
            consume_render(&mut scr, &mut ed, anchor, width);
        }
        // Ctrl+J 换行
        ed.insert('\n');
        consume_render(&mut scr, &mut ed, anchor, width);
        // 再输入一行
        for ch in "第二行".chars() {
            ed.insert(ch);
            consume_render(&mut scr, &mut ed, anchor, width);
        }
        // 退格删掉刚输入的内容
        for _ in 0..3 {
            ed.backspace();
            consume_render(&mut scr, &mut ed, anchor, width);
        }

        let lines: Vec<String> = scr.lines();
        let mut prompt_count = 0;
        for l in &lines {
            prompt_count += l.matches("> ").count();
        }
        // 屏幕上应恰好出现一次提示符（不因重绘或残留出现两次）
        assert_eq!(
            prompt_count, 1,
            "提示符不应重复/残留，应当恰好 1 次：\n{:?}",
            lines
        );
    }

    #[test]
    fn render_no_overlap_when_scrolling_bottom() {
        // 输入占满视口底部触发终端上滚时，锚点绝对行失效可能造成重叠。
        // 这里用极短视口模拟：先放满内容导致滚动，再改小输入并重绘。
        let width = 20;
        let rows = 4; // 极矮视口
        let anchor = 0; // 从顶部开始，占用全部 4 行
        let mut scr = Screen::new(rows, width);
        let mut ed = Editor::new();

        // 输入占满并超出行高（会触发滚动到视口末尾）
        for ch in "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".chars() {
            ed.insert(ch);
            consume_render(&mut scr, &mut ed, anchor, width);
        }
        // 收缩成短内容
        for _ in 0..30 {
            ed.backspace();
            consume_render(&mut scr, &mut ed, anchor, width);
        }

        let lines = scr.lines();
        let joined: String = lines.join("\n");
        // 收缩后内容应干净，不出现残余 A 或重复 prompt
        let a_count = joined.matches('A').count();
        let prompt_count = joined.matches("> ").count();
        assert!(
            a_count <= 2,
            "收缩后不应残留大量 A（上滚导致锚点错位）：{} a_count={}\n{:?}",
            joined,
            a_count,
            lines
        );
        assert_eq!(
            prompt_count, 1,
            "提示符出现 {} 次：\n{:?}",
            prompt_count, lines
        );
    }

    #[test]
    fn render_no_leftover_after_drawer_toggle() {
        // 触发问题：输入 '/' 弹出抽屉 → 抽屉关闭后，不应残留抽屉字符（│、命令名、橙底空格）
        let width = 40;
        let anchor = 2;
        let mut scr = Screen::new(12, width);
        let mut ed = Editor::new();

        // 输入 '/m'：抽屉打开，显示 /model
        for ch in "/m".chars() {
            ed.insert(ch);
            consume_render(&mut scr, &mut ed, anchor, width);
        }
        let opened = scr.lines();
        assert!(
            opened.iter().any(|l| l.contains('/') || l.contains('│')),
            "抽屉应已绘制候选：\n{:?}",
            opened
        );

        // 退格删掉 '/'，抽屉应关闭；再输入长文本
        for _ in 0..2 {
            ed.backspace();
            consume_render(&mut scr, &mut ed, anchor, width);
        }
        for ch in "PowerShell 你好".chars() {
            ed.insert(ch);
            consume_render(&mut scr, &mut ed, anchor, width);
        }

        // 关抽屉后：屏幕上只剩本次输入，不得残留抽屉的 │ / 命令片段
        let lines = scr.lines();
        let joined: String = lines.join("\n");
        assert_eq!(
            joined.matches('│').count(),
            0,
            "抽屉关闭后残留框线字符：\n{:?}",
            lines
        );
        assert!(!joined.contains("/model"), "残留命令名：\n{:?}", lines);
        // prompt 恰出现一次
        assert_eq!(joined.matches("> ").count(), 1, "提示符残留：\n{:?}", lines);
    }

    #[test]
    fn drawer_render_across_many_widths_no_overlap() {
        // 断言输入文本中的字符绝不因重绘错位而重复出现（重复=两帧叠加）。
        let text = "PowerShell 你好";
        for width in 8..=24usize {
            let rows = 16;
            let anchor = 2;
            let mut scr = Screen::new(rows, width);
            let mut ed = Editor::new();
            for ch in "/m".chars() {
                ed.insert(ch);
                consume_render(&mut scr, &mut ed, anchor, width);
            }
            for _ in 0..2 {
                ed.backspace();
                if width == 8 {
                    eprintln!(
                        "--- after backspace (buf={:?}) ---\n{:?}",
                        ed.buf,
                        scr.lines()
                    );
                }
                consume_render(&mut scr, &mut ed, anchor, width);
            }
            for ch in text.chars() {
                ed.insert(ch);
                if width == 8 {
                    eprintln!(
                        "--- typed '{}' (buf={:?}) cursor={} ---\n{:?}",
                        ch,
                        ed.buf,
                        ed.cursor,
                        scr.lines()
                    );
                }
                consume_render(&mut scr, &mut ed, anchor, width);
            }
            let joined: String = scr.lines().join("");
            // 统计原文每个字符应出现的次数，与屏幕实际出现次数比对（多出即重叠）。
            let mut expected: std::collections::HashMap<char, usize> =
                std::collections::HashMap::new();
            for ch in text.chars().filter(|c| *c != ' ') {
                *expected.entry(ch).or_insert(0) += 1;
            }
            for (ch, want) in expected {
                let n = joined.matches(ch).count();
                assert_eq!(
                    n,
                    want,
                    "宽度={} 时字符 '{}' 应出现 {} 次，实际 {} 次（重叠/缺失）\n{:?}",
                    width,
                    ch,
                    want,
                    n,
                    scr.lines(),
                );
            }
        }
    }

    /// 渲染一帧并返回原始字节串（用于断言某帧是否重写了抽屉）。
    fn render_bytes(ed: &mut Editor, anchor: usize, width: usize) -> String {
        let mut sink = Vec::new();
        render(ed, anchor, width, false, &mut sink, true).unwrap();
        String::from_utf8_lossy(&sink).to_string()
    }

    #[test]
    fn drawer_not_rewritten_when_unchanged() {
        // 核心优化点：输入 `/help` 一路收窄到唯一候选后，候选列表不再变化，
        // 此时继续改写输入（如补全后回车前的微调）不应把整个抽屉重新写入终端，
        // 只重绘输入行。据此断言：抽屉不变的那些帧，其输出字节里不含候选行内容。
        let width = 40;
        let anchor = 2;
        let mut ed = Editor::new();

        // 输入 '/' 打开抽屉（全量重绘一次，包含候选行）
        ed.insert('/');
        let open_frame = render_bytes(&mut ed, anchor, width);
        // 全量帧应包含抽屉候选（│ 分隔符）
        assert!(
            open_frame.contains('\u{2502}'),
            "抽屉打开的首帧应绘制候选：{:?}",
            open_frame
        );

        // 继续输入 'h'：候选从全部收缩为 /help —— 抽屉内容确实变，允许重绘
        ed.insert('h');
        let narrow_frame = render_bytes(&mut ed, anchor, width);
        assert!(
            narrow_frame.contains("/help"),
            "候选收窄帧允许重绘抽屉：{:?}",
            narrow_frame
        );

        // 之后输入 'e','l','p'：候选始终只有 /help，抽屉内容/选中均不变，
        // 这些帧必须「不写抽屉」，只能重写输入行。
        // 注：单候选抽屉的全量重绘会输出选中行的橙底 DRAWER_BG 与 `│` 分隔符；
        // 抽屉未变的部分重绘只写输入行（输入文本本身含 /help，故用 ORANGE/│ 作标记）。
        const ORANGE: &str = "\u{1b}[48;5;208m";
        for ch in ['e', 'l', 'p'] {
            ed.insert(ch);
            let frame = render_bytes(&mut ed, anchor, width);
            assert!(
                !frame.contains(ORANGE),
                "候选未变时不应重绘抽屉（键入 '{}'）: {:?}",
                ch,
                frame
            );
            assert!(
                !frame.contains('\u{2502}'),
                "候选未变时输出不应含候选分隔符（键入 '{}'）: {:?}",
                ch,
                frame
            );
        }
    }

    #[test]
    fn identical_frame_produces_no_output() {
        // 整帧未变化（如按住键产生的 Repeat 事件）时，render 不应向终端输出任何字节。
        // 这是「输入 / 时不会重绘好几次」的补充保障：状态没变就完全不写。
        let width = 40;
        let anchor = 2;
        let mut ed = Editor::new();

        // 第一帧总是要渲染（首帧无快照可比较）
        let first = render_bytes(&mut ed, anchor, width);
        assert!(!first.is_empty(), "首帧应输出内容");
        // 卡在空输入上（Repeat）：缓冲未变 → 第二帧应为空
        let second = render_bytes(&mut ed, anchor, width);
        assert_eq!(second, "", "未变化帧不应输出任何字节：{:?}", second);

        // 打开抽屉后，同样不变化的一帧也应无输出
        ed.insert('/');
        render_bytes(&mut ed, anchor, width); // 打开抽屉的首帧
        let unchanged = render_bytes(&mut ed, anchor, width);
        assert_eq!(unchanged, "", "抽屉打开后的未变化帧不应输出：{:?}", unchanged);
    }

    #[test]
    fn partial_redraw_updates_input_without_drawer() {
        // 抽屉打开且候选稳定时，改写输入（退格/增删字符）只重绘输入行：
        // 屏幕上候选行内容与快照一致、输入行更新、无残影/无重复文本。
        let width = 40;
        let anchor = 2;
        let mut scr = Screen::new(14, width);
        let mut ed = Editor::new();

        // 打开抽屉并输入到唯一候选 /help
        for ch in "/hel".chars() {
            ed.insert(ch);
            consume_render(&mut scr, &mut ed, anchor, width);
        }
        // 继续补齐成 /help（候选仍唯一，抽屉不变）
        ed.insert('p');
        consume_render(&mut scr, &mut ed, anchor, width);

        // 再退格删掉 'p'，又重绘（仍唯一候选）
        ed.backspace();
        consume_render(&mut scr, &mut ed, anchor, width);

        let lines = scr.lines();
        // 输入区（anchor 行）应反映最新 /hel，不残留 'p'
        assert_eq!(
            lines[anchor].trim_end(),
            "> /hel",
            "输入行应更新为 /hel：\n{:?}",
            lines
        );
        // 抽屉行（anchor+1 起，紧接输入区之后）应包含恰好一个 `/help` 候选且无重复/叠加
        let joined: String = lines[anchor + 1..].join("\n");
        assert_eq!(
            joined.matches("/help").count(),
            1,
            "候选行应恰好出现一次 /help，无叠加：\n{:?}",
            lines
        );
        // 不残留任何 'p' 于输入行
        assert!(
            !lines[anchor].contains('p'),
            "输入行不应残留已删除的 'p'：{:?}",
            lines
        );
    }
}
