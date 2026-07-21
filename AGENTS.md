# Changelog

## [2026.07.21] — v0.7.0
- 版本号从 `YYMM.DD.x` 改为 `MAJOR.MINOR.PATCH`（从此起用）
- AI TTY 模式改造：4 个工具 → 1 个 `bash` 工具（"bash is everything"）
- 删除废代码：`extract_function_call_from_content`、`execute_tool`、`flatten_output`、`tool_display_name`（-130 行）
- 新增 `run_bash()`：pwsh 安全执行 + 危险命令过滤 + 输出截断
- 更新 system prompt：工具说明 → 环境说明（coreutils `.exe` 后缀、`coreutils.exe --list-raw`）
- 简化流式 SSE 处理：去掉多工具累积逻辑
- **变更详情**：[Taolun → 2026-07-21 Bash is Everything](#2026-07-21--bash-is-everything) | [项目进度](#已完成)

## [2026.07.12] — v0.5.0
- 实现 `woman generate <name>` — AI 自动生成结构化中文手册
- 非流式 API 调用 + 自动获取原始资料（缓存优先 → --help 回退）
- 生成结果含 YAML frontmatter（title / source / generated），保存到 docs/
- 生成后自动通过 TUI 展示结果
- **变更详情**：[Taolun → 2026-07-12 Generate](#2026-07-12--generate) | [项目进度](#已完成)

## [2026.07.12] — v0.6.0
- 新增 learn.microsoft.com 在线源支持（Windows 命令文档）
- 搜索顺序：archlinux → MS Learn，Windows 原生命令自动回退
- 修复 archlinux 404 页面被当作有效内容的问题（内容类型检测）
- 添加 HTML 到纯文本的提取工具（标签剥离、实体解码、正文定位）
- **变更详情**：[Taolun → 2026-07-12 MS Learn](#2026-07-12--ms-learn) | [项目进度](#已完成)

## [2026.07.11] — v0.1.0
- 项目初始化，创建 Rust CLI 工具 `woman`
- 实现基础功能：文档查找、在线检索、--help 回退
- 实现 `~/.woman/` 目录管理（docs/ + cache/ + config.json）
- 支持 man.archlinux.org 在线抓取（通过 curl.exe，避免 SSL 依赖问题）
- 支持 `--help` / `/?` 回退（同时捕获 stdout 和 stderr）
- 使用 JSON 而非 TOML 做配置格式，简化依赖
- 前端直接输出纯文本，未使用 termimad（版本冲突，留待 v2）
- YAML frontmatter 解析（手动实现，无需 serde_yaml）
- 创建 AGENTS.md 开发规范文档
- **变更详情**：[Taolun → 2026-07-11 项目启动](#2026-07-11--项目启动) | [项目进度](#已完成)

## [2026.07.12] — v0.2.0 TUI 阅读器
- 用 `ratatui` 替代 `minus`，实现全屏 TUI 文档阅读
- **极简设计**：无顶栏、无边框线、无状态栏，全靠空行分割
- 来源标出现在底部提示中，不干扰正文阅读
- 快捷键：`↑↓`/`jk` 滚动、`/` 搜索（黄底高亮）、`n`/`N` 翻匹配、`?` 帮助弹层、`q` 退出
- 非终端环境（管道/重定向）自动降级为直接打印
- **变更详情**：[Taolun → 2026-07-12 TUI](#2026-07-12--tui) | [项目进度](#已完成)

## [2026.07.12] — v0.3.0 `woman ai` REPL
- 实现 `woman ai` 交互式 AI 对话（REPL 模式）
- 通过 `curl.exe` POST 调用 OpenAI 兼容 API（doubao-seed）
- 4 个工具函数：`run_help`、`search_online`、`read_docs`、`save_docs`
- 支持多 AI 提供者配置（`--ai <name>` 切换）
- 工具调用自动循环：AI 决定调用工具 → 执行 → 结果返回 AI → 直到生成回答
- 工具结果截断显示（超过 8 行只显示前 6 行）
- REPL 命令：`/exit`、`/help`、`/clear`、`/truncate`
- **变更详情**：[Taolun → 2026-07-12 AI](#2026-07-12--ai) | [项目进度](#已完成)

## [2026.07.12] — v0.4.0 SSE 流式 + 打字机 + /model
- API 调用改为 SSE 流式（`curl.exe -N` + `BufReader` 逐行解析），AI 回复实时逐字出现
- 逐字打字机效果（6ms 延迟，ANSI 转义序列整体打出）
- 工具结果和 AI 回答之间自动空行分隔
- `> ` 蓝色提示符，启动信息橙色高亮，/model 命令蓝色
- `AiProvider` 新增 `default: bool`，优先选择默认提供者
- `config.json` 自动迁移：无标记时第一个设为 default
- `/model` 交互式下拉框：↑↓/jk 切换，Enter 确认，Esc 取消，橙底黑字高亮
- 切换模型后自动落盘 `config.json`，下次启动自动选中
- 代码高亮从黄色改为红色
- **变更详情**：[Taolun → 2026-07-12 流式](#2026-07-12--流式) | [Taolun → model](#2026-07-12--model--default) | [项目进度](#已完成)


# Taolun

## 2026-07-11 — 项目启动
### 讨论摘要
- 决定开发 `woman`，作为 Windows 上统一的手册查看工具，替代 Linux man
- 核心思路：AI 作为适配器，根据本机 `--help` 和在线手册生成精准文档
- 目录结构：`docs/`（最终版） + `cache/`（在线缓存） + `config.toml`
- 查找优先级：`docs/` → 命令是否存在 → `cache/` → `--help` → 提示搜索
- `docs/` 内用 YAML frontmatter 区分来源（manual / ai-generated）
- AI 后端使用字节跳动 doubao-seed，config 支持多 AI（用 `name` 区分）
- 先做 v1 基础功能，AI 功能（generate / woman ai）放在 v2
- 在线源：man.archlinux.org（主）+ learn.microsoft.com（后续）
- 项目遵循与 `was` 相同的 AGENTS.md 规范

### 涉及文件
- `Cargo.toml` — 项目配置
- `src/main.rs` — CLI 入口
- `src/config.rs` — 配置管理
- `src/docs.rs` — 文档目录管理
- `src/fetch.rs` — 在线抓取
- `src/display.rs` — Markdown 渲染
- `AGENTS.md` — 开发规范

### 相关变更
- [Changelog → 2026.07.11](#20260711--v010) | [项目进度 → 已完成](#已完成)


# Agents

## 规范
1. **三次重试原则**：同一个问题重复 3 次无法解决，强制停止，向用户详细汇报遇到的问题，等待用户解答。
2. **全中文**：整个对话流程全部使用中文，包括 AI 思考过程输出在终端中的内容。
3. **详细注释**：代码必须有详细的中文注释。
4. **版本格式**：`MAJOR.MINOR.PATCH`（如 `0.7.0`），从 `v0.7.0` 起用。旧版 `YYMM.DD.x` 格式的历史版本号不变。
5. **测试拆分**：测试文件按功能模块拆分成多个文件，禁止在一个文件里写全部测试。
6. **面向对象**：采用 OOP 方式开发，保持功能模块单一，高内聚低耦合。

## 项目进度

### 计划中
- 自动更新检测（文档版本 vs 工具版本）

### 代办
- （无）

### 已完成
- [x] 创建 Rust 项目结构（Cargo.toml, src/） — [Taolun → 项目启动](#2026-07-11--项目启动)
- [x] 实现 `~/.woman/` 目录初始化（docs/ + cache/ + config.json）
- [x] 实现 `docs/` 读取与 YAML frontmatter 解析
- [x] 实现 `cache/` 读写与缓存管理
- [x] 实现 `woman <name>` 完整查找流程（docs → cache → --help）
- [x] 实现 `woman search <name>` 在线抓取（man.archlinux.org，curl.exe）
- [x] 实现 `--help` / `/?` 原始输出回退（支持 stdout + stderr）
- [x] 实现 `--help` / `-?` / `--version` / `-V`
- [x] 实现 TUI 全屏文档阅读器（ratatui，无边框极简设计） — [Taolun → TUI](#2026-07-12--tui)
- [x] 实现 `woman ai` 交互式 AI 对话（REPL + 函数调用 + 4 个工具） — [Taolun → AI](#2026-07-12--ai)
- [x] SSE 流式输出 + 打字机效果 — [Taolun → 流式](#2026-07-12--流式)
- [x] `/model` 交互式下拉框切换提供者 + default 持久化 — [Taolun → /model + default](#2026-07-12--model--default)
- [x] 实现 `woman generate <name>` — AI 自动生成中文手册 — [Taolun → Generate](#2026-07-12--generate) | [Changelog → v0.5.0](#20260712--v050)
- [x] 新增 learn.microsoft.com 在线源 + archlinux 404 修复 — [Taolun → MS Learn](#2026-07-12--ms-learn) | [Changelog → v0.6.0](#20260712--v060)
- [x] 编译发布到 `C:\Program Files\coreutils\bin\` — [Changelog → v0.1.0](#20260711--v010)
- [x] AI TTY 模式"bash is everything"改造 — [Taolun → 2026-07-21 Bash is Everything](#2026-07-21--bash-is-everything) | [Changelog → v0.7.0](#20260721--v070)

## 开发流程
1. **先记录后编码**：每次改动前，先在 `Taolun` 章节保存讨论记录，再开始修改文件。
2. **使用 bash 命令**：Windows 已内置 coreutils，优先使用 `grep` `ls` `sed` `find` 等命令，避免使用 PowerShell cmdlet。
3. **完成后更新**：开发完成后，同步更新「项目进度」和「Changelog」。Changelog 条目与 Taolun 记录、项目进度通过 **外链** 关联，方便溯源。


## 2026-07-12 — TUI
### 讨论摘要
- 用户反馈 `minus` 分页器体验割裂：分隔线/来源标先打，分页器启动，退出后提示才显示
- 决定用 `ratatui` 做全屏 TUI，替代原来 `minus` 的分页方案
- 过程中逐步去除顶部栏、边框线、状态栏，最终定为极简设计
- 来源标从正文上方移到底部提示行
- 快捷键帮助通过 `?` 弹层展示
- `search_online` 结果也用 TUI 显示，保持一致性
- 管道/重定向时不进 TUI，直接打印

### 涉及文件
- `Cargo.toml` — 依赖变更（minus → ratatui + crossterm）
- `src/tui.rs` — 新建 TUI 模块
- `src/main.rs` — lookup_and_show / search_and_cache 改为调 tui::show_document
- `src/display.rs` — 删除 display_paged 和废弃的 render_separator

### 相关变更
- [Changelog → v0.2.0](#20260712--v020-tui-阅读器) | [项目进度 → 已完成](#已完成)

## 2026-07-12 — AI
### 讨论摘要
- 用户选择 REPL 模式（非 TUI）作为 `woman ai` 的交互方式
- 所有 API 调用通过 curl.exe POST 完成，复用 fetch.rs 的 HTTP 方案
- 4 个工具函数对应 4 个已有或新功能：run_help、search_online、read_docs、save_docs
- 工具调用自动循环：AI 决定调用 → 执行并显示结果摘要 → 结果返回 AI → 循环直到 AI 生成回答
- 支持 `--ai <name>` 切换提供者，config.json 存数组
- `/truncate` 命令裁剪消息历史重新开始

### 涉及文件
- `src/ai.rs` — 新建 AI 客户端模块（消息类型、API 调用、工具执行、REPL 循环）
- `src/main.rs` — 添加 `ai` 子命令
- `src/config.rs` — 添加 `get_provider()` 方法
- `~/.woman/config.json` — 创建 AI 配置模板

### 相关变更
- [Changelog → v0.3.0](#20260712--v030-woman-ai-repl) | [项目进度 → 已完成](#已完成)

## 2026-07-12 — 流式
### 讨论摘要
- 用户要求实现打字机效果，将 SSE 逐行输出改为逐字输出
- 逐字输出的关键在于识别 ANSI 转义序列（`\x1b[...m`），作为整体一次打出
- 延迟从最初 15ms 调整为 6ms（用户要求"更快"）
- ANIS 行内 `` `code` `` 从黄色改为红色，用户认为黄色不好看
- `> ` 提示符改为蓝色，doubao/提供者名橙色高亮
- `/model` 命令使用交互式下拉框（crossterm raw mode），而非简单的编号输入
- 下拉框选中项使用橙底黑字（`\x1b[48;5;208m\x1b[30m`），去序号

### 涉及文件
- `src/ai.rs` — 添加 `typewrite()`、`select_provider()`；`chat_completion()` → `chat_completion_stream()`；REPL 排版调整
- `src/display.rs` — YELLOW → RED
- `src/main.rs` — 传 `&mut config.ai` 给 `run_repl`

### 相关变更
- [Changelog → v0.4.0](#20260712--v040-sse-流式--打字机--model) | [项目进度 → 已完成](#已完成)

## 2026-07-12 — /model + default
### 讨论摘要
- 用户提出 `/model` 切换模型后应持久化，下次启动自动选中
- 在 `AiProvider` 中增加 `default: bool` 字段，`get_provider(None)` 优先返回 `default: true` 的
- `Config::load()` 自动迁移：无提供者标记 default 时，第一个自动设为 default 并保存
- 切换模型时通过 `Config::load().set_default()` 落盘，同时更新内存中 `all_providers` 的 default 标志
- `main.rs` 改为传 `&mut Vec<AiProvider>`，使 `ai.rs` 能修改提供者列表
- 借位检查（E0502）通过在 mutable 操作前先提取 `new_name` 为 owned String 解决

### 涉及文件
- `src/config.rs` — `AiProvider.default`、`get_provider()` 优先 default、`set_default()`、`load()` 自动迁移
- `src/ai.rs` — `run_repl` 签名改为 `&mut Vec<AiProvider>`、`/model` 切换时翻转 default + 落盘
- `src/main.rs` — `config` 改为 `mut`，传 `&mut config.ai`
- `~/.woman/config.json` — 补 `default` 字段

### 相关变更
- [Changelog → v0.4.0](#20260712--v040-sse-流式--打字机--model) | [项目进度 → 已完成](#已完成)


# 认知修正

## 2026-07-11 — Rust 依赖管理
- **踩坑**：`ureq` 依赖 `rustls` → `ring`，在 `x86_64-pc-windows-gnu` 工具链下编译失败。`toml` crate 的 `toml_edit` 也有版本冲突
- **纠正**：改用 `curl.exe` 做 HTTP 请求（Windows 内置），配置格式改用 JSON（`serde_json`）替代 TOML
- **教训**：Windows GNU 工具链下，涉及 `ring` 的 crate 容易出问题。对 CLI 工具而言，`curl.exe` 足以满足需求，无需强行引入原生 HTTP 库

## 2026-07-11 — termimad 版本冲突
- **踩坑**：`termimad` v0.29.4 同时依赖 `crossterm` v0.27 和 v0.29，导致 coolor 的 trait 实现冲突
- **纠正**：v1 直接输出纯文本，跳过 Markdown 渲染
- **教训**：终端 Markdown 渲染库的依赖树复杂，v1 先做功能，渲染留到后续版本

## 2026-07-11 — was 与 unwas 必须完全独立
- **踩坑**：最初用单个二进制 + argv[0] 分发实现 was/unwas，被用户纠正
- **纠正**：每个工具只做一件事，was 不含任何删除逻辑，unwas 不含任何设置逻辑
- **教训**：不要为了"代码复用"把职责不同的命令塞进同一个二进制
- 帮助短格式用 `-?` 而非 `-h`

## 2026-07-12 — REPL 模式的输入处理
- **踩坑**：REPL 循环中 `print!("> ")` 需要显式 `flush()`，否则 Windows 上缓冲区不会立即刷新
- **纠正**：`io::stdout().flush()` 放在每次打印提示符之后
- **教训**：Windows 控制台行缓冲行为与 Linux 不同，交互式程序需要主动 flush

## 2026-07-12 — curl.exe 做 AI API 调用可行
- **发现**：`curl.exe` 的 `-d` 参数可以直接传入 JSON body，足够满足 AI API 调用需求
- **局限**：超长消息（超过 CreateProcess 命令行长 32767 字符）时需要改用 `-d @file`
- **教训**：工具调用的结果可能很长（man page 全文），结果返回给 AI 没问题，但用户界面显示时需要截断（可见 6 行 + ...共 N 行）

## 2026-07-12 — doubao-seed 函数调用格式不标准
- **发现**：doubao-seed 虽然号称 OpenAI 兼容，但函数调用有时会以文本 `<|FunctionCallBegin|>...<|FunctionCallEnd|>` 返回在 `content` 字段中，而不是标准的 `function_call` 字段
- **纠正**：添加 `extract_function_call()` 函数，优先检查标准字段，其次解析 content 中的特殊标记
- **额外差异**：部分模型用 `parameters` 对象而非 `arguments` 字符串；`arguments` 可能直接是 JSON 对象而非 JSON 字符串
- **教训**：对接国内 API 时，函数调用需要做两层容错——字段位置和参数格式都要兼容

## 2026-07-12 — SSE 流式方案选择
- **踩坑**：最初考虑用 `ureq` / `reqwest` 做流式 HTTP，但 `rustls` → `ring` 在 Windows GNU 下编译失败
- **纠正**：用 `curl.exe -N` + `Stdio::piped()` + `BufReader::lines()` 逐行解析 SSE 事件流
- **教训**：Windows 上做 SSE 流式，`curl.exe -N`（禁用缓冲）是最可靠的方案，无需原生 HTTP 库

## 2026-07-12 — 打字机效果实现
- **踩坑**：直接逐字打印会导致 ANSI 转义序列（`\x1b[...m`）被拆开，终端显示乱码
- **纠正**：`typewrite()` 函数遇到 `\x1b` 时连续读至 `m` 作为整体一次打出，普通字符逐字输出 + `thread::sleep(delay)`
- **教训**：带有 ANSI 颜色的文本不能简单地 `.chars().for_each(|c| print!("{c}"))`，必须跳过转义序列

## 2026-07-12 — 交互式下拉框方案
- **踩坑**：最初用 `print!("选择 (1-N): ") + read_line()` 做编号选择，用户要求改为真正的下拉框组件
- **纠正**：用 `crossterm` 的 `enable_raw_mode()` + `event::read()` 实现 ↑↓/jk 导航、Enter/Esc 确认取消
- **实现要点**：`MoveUp(N)` 重绘列表实现选中项高亮切换；`Hide`/`Show` 控制光标闪烁；raw mode 结束后需恢复终端状态
- **教训**：`crossterm` 已在依赖中（`ratatui` 的 peer dep），直接复用即可，无需额外引入

## 2026-07-12 — 提供者 default 持久化
- **踩坑**：`run_repl` 原来只接收 `&AiProvider`，无法修改提供者列表；切换模型后无法持久化 default
- **纠正**：签名改为接收 `&mut Vec<AiProvider>`，切换时同时更新内存和磁盘（`Config::load().set_default()`）
- **教训**：`&mut Vec` 和 `&` 引用不能同时存在（E0502），需要先提取 `new_name` 为 owned String 再做 mutable 操作
- `Config::load()` 自动迁移机制避免了用户手动编辑 config.json
- `#[serde(default)]` 确保旧配置兼容，升级后自动补 `default: false`

## 2026-07-12 — Generate
### 讨论摘要
- 用户选择实现 `woman generate <name>` — AI 自动生成结构化中文手册
- 基于现有 AI 基础设施，使用非流式 API 调用（无需打字机效果）
- 自动获取原始资料：优先读缓存（`cache/`），其次执行 `--help`，都不存在则报错引导用户先 `woman search`
- 生成 prompt 要求 AI 输出含 YAML frontmatter（title / source: ai-generated / generated 日期）
- 生成结果保存到 `docs/` 后自动通过 TUI 展示

### 涉及文件
- `src/ai.rs` — 新增 `chat_completion()`（非流式）、`generate_docs()`（公开入口）
- `src/main.rs` — 更新 `generate` 子命令，调用 `generate_docs()`
- `src/docs.rs` — `current_date()` 从 `fn` 改为 `pub(crate) fn`

### 相关变更
- [Changelog → v0.5.0](#20260712--v050) | [项目进度 → 已完成](#已完成)

## 2026-07-12 — MS Learn
### 讨论摘要
- 新增 learn.microsoft.com 在线源支持（Windows 命令文档）
- 搜索顺序：archlinux 优先，MS Learn 作为回退
- 修复 archlinux 404 页面被当作有效内容的问题（`fetch_from_archlinux_section` 增加内容类型检测）
- 实现 HTML 到纯文本提取（标签剥离、实体解码、正文定位到 `<article>` 区域）

### 涉及文件
- `src/fetch.rs` — 新增 `html_to_text()`、`extract_between()`、`extract_mslearn_body()`、`fetch_from_mslearn()`；修复 `fetch_from_archlinux_section()`（404 内容检测）
- `src/main.rs` — `search_and_cache()` 改为先 archlinux 后 MS Learn 的回退流程

### 相关变更
- [Changelog → v0.6.0](#20260712--v060) | [项目进度 → 已完成](#已完成)

## 2026-07-21 — Bash is Everything
### 讨论摘要
- 参考 `claude/src/index.ts` 的智能体模式（single bash tool），改造 `woman ai` TTY 模式
- 核心思路：AI 只有一个 bash 工具，所有操作（`--help`、curl 搜索、文件读写）通过 PowerShell 命令完成
- 删除 4 个旧工具（`run_help`、`search_online`、`read_docs`、`save_docs`），对应删除 `execute_tool` 和 doubao 兼容代码
- AI 直接跑 `pwsh -NoProfile -Command`，需被告知核心工具环境（coreutils `.exe` 后缀、`coreutils.exe --list-raw` 等）

### 涉及文件
- `src/ai.rs` — TOOLS_JSON（4→1）、SYSTEM_PROMPT（工具说明→环境说明）、新增 `run_bash()`、简化流式处理、删除 `execute_tool` 等废代码

### 相关变更
- [Changelog → v0.7.0](#20260721--v070) | [项目进度 → 已完成](#已完成)

## 2026-07-21 — Bash is Everything 认知修正
- **发现**：4 个硬编码工具的 doubao 兼容代码（`<|FunctionCallBegin|>` 标记）和工具派发逻辑（`execute_tool`）全部可以被一个 `bash` 工具替代
- **纠正**：删除 4 个工具 → 用单一 `bash` 工具 + `run_bash()` 函数执行任何 PowerShell 命令
- **教训**：工具越少，AI 越自由。硬编码搜索源（archlinux / MS Learn）不如让 AI 自己用 `curl.exe` 决定怎么查。Rust 代码量减少不等于能力减少，反而更灵活
- **教训**：system prompt 需要准确描述执行环境（coreutils `.exe` 后缀、pwsh 路径倒斜杠），否则 AI 生成的命令会在 Windows 上报错

---> **CoreUtils 使用规范**：`grep` `ls` `sed` `find` 等命令用法详见 `~/.config/opencode/docs/coreutils.md`
