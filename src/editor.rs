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
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, size, Clear, ClearType},
};

// ============================================================
// 常量与样式
// ============================================================

/// 内置命令候选（抽屉内容）。顺序即显示顺序。
pub const COMMANDS: &[&str] = &[
    "/help",
    "/model",
    "/clear",
    "/truncate",
    "/exit",
    "/quit",
];

const PROMPT: &str = "\x1b[34m> \x1b[0m";
const PROMPT_WIDTH: usize = 2; // "> " 的字符宽度

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
fn layout_starts(buf: &str, term_width: usize) -> Vec<usize> {
    let mut starts: Vec<usize> = Vec::new();
    let mut col = PROMPT_WIDTH;
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
fn cursor_pos(buf: &str, cursor: usize, term_width: usize) -> (usize, usize) {
    let prefix = &buf[..cursor];
    let starts = layout_starts(prefix, term_width);
    let row = starts.len().saturating_sub(1);
    let line_start = *starts.last().unwrap_or(&0);
    let seg = &prefix[line_start..];
    let seg_width = seg.chars().map(char_width).sum::<usize>();
    // 仅「最初那一行的首个显示行」在行首有 prompt 前缀；其余（软换行/后续逻辑行）从列 0 起
    let col = if line_start == 0 {
        PROMPT_WIDTH + seg_width
    } else {
        seg_width
    };
    (row, col)
}

// ============================================================
// 行编辑器状态
// ============================================================

/// 行编辑器内部状态
struct Editor {
    buf: String,
    cursor: usize,    // 字节偏移
    sel: usize,       // 抽屉选中项索引
    prev_filt: String, // 上次的过滤前缀（用于前缀变化时重置选中项）
    dismissed: bool,  // 用户按 Esc 关闭抽屉后的临时隐藏开关
}

impl Editor {
    fn new() -> Self {
        Editor {
            buf: String::new(),
            cursor: 0,
            sel: 0,
            prev_filt: String::new(),
            dismissed: false,
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
        self.buf[1..].split_whitespace().next().unwrap_or("").to_string()
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
        let end = self
            .buf
            .find(char::is_whitespace)
            .unwrap_or(self.buf.len());
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
fn render<W: Write>(
    ed: &Editor,
    anchor_row: usize,
    term_width: usize,
    out: &mut W,
    place_cursor: bool,
) -> io::Result<()> {
    // 跳到锚点行首并清空其下残影
    // 注意 crossterm MoveTo 参数顺序为 (列, 行)
    execute!(
        out,
        MoveTo(0, anchor_row as u16),
        Clear(ClearType::FromCursorDown)
    )?;

    // 打印 prompt + 缓冲区（内部换行由终端处理，超宽自动软换行）
    // 注意：raw 模式下 \n 不会自动回行首，需显式转为 \r\n
    write!(out, "{}", PROMPT)?;
    let rendered_buf = ed.buf.replace('\n', "\r\n");
    write!(out, "{}", rendered_buf)?;

    // 抽屉：在输入区下方空一行后绘制
    if ed.menu_should_open() {
        let m = ed.matches();
        write!(out, "\r\n")?; // 输入区与抽屉之间空一行（raw 模式显式 CRLF）
        for i in 0..m.len() {
            draw_drawer_row(out, &m[i], i == ed.sel.min(m.len() - 1), term_width)?;
        }
    }

    // 把硬件光标移回插入位置（仅在编辑过程中需要）
    if place_cursor {
        let (cur_row, cur_col) = cursor_pos(&ed.buf, ed.cursor, term_width);
        execute!(out, MoveTo(cur_col as u16, (anchor_row + cur_row) as u16))?;
    }
    Ok(())
}

// ============================================================
// 主入口
// ============================================================

/// 在 raw 模式下读取一段多行输入。
/// 返回 `Some(去首尾空白的字符串)`；无法进入 raw 模式则返回 `None`。
pub fn read_input() -> Option<String> {
    // 先记录进入编辑前硬件光标所在行（作为重绘锚点），再进入 raw 模式
    let anchor = crossterm::cursor::position()
        .ok()
        .map(|p| p.1 as usize)
        .unwrap_or(0);

    enable_raw_mode().ok()?;
    execute!(io::stdout(), Hide).ok()?;

    // 确保终端自动换行开启（DECAWM），避免长行不换行、编辑区布局与光标错位
    write!(io::stdout(), "\x1b[?7h").ok();
    let _ = io::stdout().flush();

    let term_width = size()
        .ok()
        .map(|(w, _)| (w as usize).max(20))
        .unwrap_or(80);

    let mut out = io::stdout();
    let mut ed = Editor::new();

    let result = loop {
        ed.sync_selection();
        render(&ed, anchor, term_width, &mut out, true).ok()?;

        let ev = match event::read() {
            Ok(e) => e,
            Err(_) => break ed.buf.trim().to_string(),
        };
        let Event::Key(KeyEvent { code, modifiers, kind, .. }) = ev else { continue };
        if kind != KeyEventKind::Press {
            continue;
        }

        match (code, modifiers) {
            // ---- 提交 ----
            (KeyCode::Enter, _) => {
                if ed.menu_should_open() {
                    let m = ed.matches();
                    let idx = ed.sel.min(m.len() - 1);
                    // 若首 token 已是选中的完整命令 → 直接提交（回车补全后的第二次回车）
                    let token = ed.buf.split_whitespace().next().unwrap_or("");
                    if token == m[idx] {
                        break ed.buf.trim().to_string();
                    } else {
                        ed.apply_selection();
                    }
                } else {
                    break ed.buf.trim().to_string();
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
    render(&ed, anchor, term_width, &mut out, false).ok()?;
    disable_raw_mode().ok()?;
    execute!(io::stdout(), Show).ok()?;

    Some(result)
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
        assert_eq!(layout_starts("", 80), vec![0]);
    }

    #[test]
    fn layout_single_short_line() {
        // 短单行不换行：始终一个显示行
        assert_eq!(layout_starts("abc", 80), vec![0]);
    }

    #[test]
    fn layout_newline_splits() {
        // 换行拆成两个逻辑行：第一行起点 0，第二行起点 4
        assert_eq!(layout_starts("abc\ndef", 80), vec![0, 4]);
    }

    #[test]
    fn layout_soft_wrap() {
        // term_width=5，prompt 占 2。
        // “abc” col=5 满；再加 'd' 溢出 → 软换行，第二行起点字节 3
        let buf = "abcd";
        // 'a'(3) 'b'(4) 'c'(5) 'd'(6>5→换行) → starts=[0,3]
        assert_eq!(layout_starts(buf, 5), vec![0, 3]);
    }

    #[test]
    fn cursor_after_prompt_when_empty() {
        // 空输入：光标应在第一行、prompt 之后（列 2）
        assert_eq!(cursor_pos("", 0, 80), (0, PROMPT_WIDTH));
    }

    #[test]
    fn cursor_after_short_line() {
        // 'ab' 不换行：光标在同一行、prompt 之后
        assert_eq!(cursor_pos("ab", 2, 80), (0, 2 + 2));
    }

    #[test]
    fn cursor_after_newline() {
        // 'abc\nde'：第二行没有 prompt 前缀，光标在第二行列 2（de 两个字符）
        assert_eq!(cursor_pos("abc\nde", 6, 80), (1, 2));
    }

    #[test]
    fn cursor_after_soft_wrap() {
        // term_width=5，'abcd' 第一行 'abc'(col5)，第二行 'd'
        // 光标在第二行：列应为 1（d 占 1，无反白 prompt）
        assert_eq!(cursor_pos("abcd", 4, 5), (1, 1));
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
                    let mut parts: Vec<usize> = args
                        .split(';')
                        .map(|a| a.parse().unwrap_or(1))
                        .collect();
                    if parts.len() == 1 {
                        parts.insert(0, 1);
                    }
                    self.r = (parts[0].saturating_sub(1)) % self.rows;
                    self.c = (parts[1].saturating_sub(1)) % self.cols;
                }
                'J' => {
                    let n: usize = if args.is_empty() { 0 } else { args.parse().ok().unwrap_or(0) };
                    if n == 0 {
                        for x in self.c..self.cols {
                            self.buf[self.r][x] = ' ';
                        }
                        for rr in self.r + 1..self.rows {
                            self.buf[rr] = vec![' '; self.cols];
                        }
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
    fn consume_render(
        scr: &mut Screen,
        ed: &Editor,
        anchor: usize,
        width: usize,
    ) {
        let mut sink = Vec::new();
        render(ed, anchor, width, &mut sink, true).unwrap();
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
            consume_render(&mut scr, &ed, anchor, width);
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
            consume_render(&mut scr, &ed, anchor, width);
        }
        // Ctrl+J 换行
        ed.insert('\n');
        consume_render(&mut scr, &ed, anchor, width);
        // 再输入一行
        for ch in "第二行".chars() {
            ed.insert(ch);
            consume_render(&mut scr, &ed, anchor, width);
        }
        // 退格删掉刚输入的内容
        for _ in 0..3 {
            ed.backspace();
            consume_render(&mut scr, &ed, anchor, width);
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
            consume_render(&mut scr, &ed, anchor, width);
        }
        // 收缩成短内容
        for _ in 0..30 {
            ed.backspace();
            consume_render(&mut scr, &ed, anchor, width);
        }

        let lines = scr.lines();
        let joined: String = lines.join("\n");
        // 收缩后内容应干净，不出现残余 A 或重复 prompt
        let a_count = joined.matches('A').count();
        let prompt_count = joined.matches("> ").count();
        assert!(
            a_count <= 2,
            "收缩后不应残留大量 A（上滚导致锚点错位）：{} a_count={}\n{:?}",
            joined, a_count, lines
        );
        assert_eq!(prompt_count, 1, "提示符出现 {} 次：\n{:?}", prompt_count, lines);
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
            consume_render(&mut scr, &ed, anchor, width);
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
            consume_render(&mut scr, &ed, anchor, width);
        }
        for ch in "PowerShell 你好".chars() {
            ed.insert(ch);
            consume_render(&mut scr, &ed, anchor, width);
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
                consume_render(&mut scr, &ed, anchor, width);
            }
            for _ in 0..2 {
                ed.backspace();
                if width == 8 {
                    eprintln!("--- after backspace (buf={:?}) ---\n{:?}", ed.buf, scr.lines());
                }
                consume_render(&mut scr, &ed, anchor, width);
            }
            for ch in text.chars() {
                ed.insert(ch);
                if width == 8 {
                    eprintln!("--- typed '{}' (buf={:?}) cursor={} ---\n{:?}", ch, ed.buf, ed.cursor, scr.lines());
                }
                consume_render(&mut scr, &ed, anchor, width);
            }
            let joined: String = scr.lines().join("");
            // 统计原文每个字符应出现的次数，与屏幕实际出现次数比对（多出即重叠）。
            let mut expected: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
            for ch in text.chars().filter(|c| *c != ' ') {
                *expected.entry(ch).or_insert(0) += 1;
            }
            for (ch, want) in expected {
                let n = joined.matches(ch).count();
                assert_eq!(
                    n, want,
                    "宽度={} 时字符 '{}' 应出现 {} 次，实际 {} 次（重叠/缺失）\n{:?}",
                    width, ch, want, n, scr.lines(),
                );
            }
        }
    }
}
