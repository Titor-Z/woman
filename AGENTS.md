# Changelog

## [2026.09.01] — CI / Release 三平台构建
- 项目已跨平台（v0.11.0），把两个 GitHub Actions workflow 从「仅 Windows」扩展为**三平台矩阵构建**（Windows / macOS / Linux）
- `build.yml`：三平台矩阵 + 新增 `cargo test`（先在 debug 跑测试再 release 构建）+ 上传各平台产物
- `release.yml`：三平台矩阵构建 + 统一命名 `woman-<tag>-<os>` 上传 Release（`softprops/action-gh-release`）
- 产物命名：Windows `x64-windows` / macOS `aarch64-apple`（macos-latest 为 arm64）+ `x64-apple`（macos-13 为 Intel x64）/ Linux `x64-linux`
- 依赖（dirs/serde/ratatui/crossterm）纯 Rust 跨平台，无需额外 target 安装
- **变更详情**：[Taolun → 2026-09-01 CI/Release](#2026-09-01--cirelease-三平台构建) | [项目进度 → 已完成](#已完成)
- **补充**：新增 `macos-13`（Intel x64）矩阵条目，四平台产物互补（arm64 + x64 mac）

## [2026.09.01] — v0.11.0 跨平台：全平台 man 替代（Windows / macOS / Linux）
- 从 Windows 专用改为**全平台 man 替代品**，mac/linux 也能用
- 新增**统一平台抽象层 `src/platform.rs`**：把全部 `cfg!(target_os)` 分叉收敛到一处，其余模块调用它
  - 平台差异映射：curl→curl.exe/curl；shell→`pwsh -NoProfile -Command`/`sh -c`；定位→where.exe/which；清屏→cmd /c cls/ANSI
- **OS 检测**用 `cfg!(target_os)`（编译期硬保证），不引入自定义环境变量
- **AI shell 三平台可用**：Windows pwsh / Unix sh，不降级禁用；`run_bash` 用 `platform::shell_runner()`
- 新增 `is_dangerous()`：危险命令黑名单**分平台**（Win 拦 rm -rf /、rd /s /q、format 等；Unix 拦系统破坏性惯用法）
- **AI prompt 分平台**：`system_prompt()`/`tools_json()` 两个 cfg 实体（Win 提 pwsh/.exe/Get-Help；Unix 提 sh/man/whatis）
- **coreutils 分类**：Windows 探测 `coreutils.exe`；Unix 直接用系统二进制（which 探测），系统命令即 man 目标
- 新增 **man / whatis Unix 源** `fetch_from_man`；`run_help` 平台感知（`/?` 仅 Windows）；Get-Help 编译期 `#[cfg(windows)]` 排除
- skill 模板通用化（`/?`/.exe/MS Learn 标注平台通用措辞），同步磁盘 `~/.woman/skills/manual.md`
- 非 Windows 上 PowerShell/Windows 专属源**编译期 `#[cfg(windows)]` 排除**，无死代码（删 Unix stub，函数加 cfg gate）
- **变更详情**：[Taolun → 2026-09-01 跨平台](#2026-09-01--跨平台全平台-man-替代) | [项目进度 → 已完成](#已完成)

## [2026.09.01] — v0.10.0 woman init：交互式初始化向导 + models.dev 目录（本地缓存离线可用）
- 新增 `woman init`：交互式初始化向导——选厂商 → 选模型 → 输 API Key → 写入 config.json
- 目录来源 = **models.dev**（opencode / pi / deepseek-harness 同源，MIT）：`curl.exe` 拉取 `https://models.dev/api.json`
- 目录**本地缓存离线可用**：缓存 `~/.woman/models/catalog.json`；三级——有缓存直接用（离线）、无缓存拉取、拉取失败且无缓存才报错
- `init --refresh` 强制重拉目录；`init --reset` 恢复默认 config + skill 模板；`--reset --wipe` 另清 docs/cache
- 向导可**重复**进入（追加/替换提供者，兼容现有多提供者 ai[]），新提供者自动设 default
- **OpenAI 兼容过滤**：仅 `npm == "@ai-sdk/openai-compatible"` 且带 `api` 的厂商（真实目录 212 家中约 172 家）直接用 `AiProvider`
- 解析用**精简 serde 结构**（`#[serde(default)]`，忽略未知字段），不引入完整巨型结构
- 新增 `prompt_masked()`（raw 模式 `*` 掩码输入）+ `prompt_line()`；厂商/模型选择复用 mdr 卡片选择器 `tui::pick_entries`
- 新增 `src/catalog.rs`、`src/init.rs`；`Config::models_dir()/catalog_path()/add_provider()`；`init` 子命令 + `--refresh/--reset/--wipe` 解析
- 真实目录解析实测：212 家厂商 / 172 家 OpenAI 兼容 / 7478 个模型，全部通过
- **变更详情**：[Taolun → 2026-09-01 woman init](#2026-09-01--woman-init交互式初始化-向导--modelsdev-目录) | [项目进度 → 已完成](#已完成)

## [2026.09.01] — v0.9.1 手册详细化 + 本地选项校对（AI 校验）
- docs 手册变为**详细完整手册**：skill 模板从「重点讲常用、不罗列冷门」改为**列出全部选项/参数**（完整性优先，方便离线参考）
- `fetch_source` 返回**双资料** `FetchedSource`：本地 `--help`/`/?`（本机选项真值）+ 在线全文（archlinux / MS Learn / Get-Help 详细说明）；coreutils 现改为 `--help` 与 archlinux **同时取**，不再 `--help` 成功即短路
- **在线选项按本地校对（AI 校验）**：`enhance()` 把本地 --help 与在线全文都喂给 AI，由 AI 对比选项；在线有、本机 `--help` 未出现的选项 → 保留并标注 **『本机此版本不支持』**
- 无 AI key 时也抓在线全文合并缓存展示，保证离线 `woman <name>` 有详细手册（本地 `--help` 短输出仅兜底）
- cache 存合并后的完整原始资料（本地 + 在线），无 AI / AI 失败时兜底展示 `combined`
- skill 测试新增「本地选项校对 + 完整性」断言
- **变更详情**：[Taolun → 2026-09-01 手册详细化](#2026-09-01--手册详细化--本地选项校对ai-校验) | [项目进度 → 已完成](#已完成)

## [2026.09.01] — v0.9.0 重构为 man 替代品 + AI 可选智能渲染层
- 单二进制 + 单命令对齐 man：`woman <name>` 即手册查询；新增 `woman -q "<问题>"` 一次性 agent 问答；保留 `woman ai`；**移除** `search`/`generate`/`-s`
- 全自动对齐 man：docs 未命中 → 按类型分类（coreutils/powershell/windows）→ 取源 → AI 整理写 docs/ → 展示；取消手动 search/generate
- AI 作为可选智能渲染层：有 key → 整理成结构化中文手册（存 docs/）；无 key → 直接展示原始内容（纯 man 完全可用）
- 新增源：`Get-Help`（PowerShell）；`ls.exe`/`ls` 均归 coreutils（分类去 `.exe`）
- 新增 `src/skill.rs`：手册风格解耦，`~/.woman/skills/manual.md` 章节模板 + 整理指令，用户可自定义多风格
- 新增 mdr 卡片式选择器 `tui::pick_entries`：多类型命中时让用户选择（coreutils > windows > powershell 为取消兜底）
- 数据模型：docs=`*.md`（人类可读）、cache 改 `.txt` + 兼容读取旧 `.md`；coreutils 清单缓存 `~/.woman/coreutils-list.txt`
- docs frontmatter 增加 `type`；`source: ai-enhanced` 徽标
- **变更详情**：[Taolun → 2026-09-01 重构](#2026-09-01--woman-重构为-man-替代品--ai-可选智能渲染层) | [项目进度 → 已完成](#已完成)

## [2026.07.21] — v0.8.2 上下文压缩省 token
- 用户关心 token 浪费且要求保留准确性
- `run_bash` 工具输出截断 50000 → 12000 字符（AI 只需提炼要点，不损失准确性，单次省大量 token）
- 新增 `trim_context()`：发送请求前自动裁剪上下文，用字符预算（≈120k 字符 ≈ 30k token）兜底
  - system 永远保留；从最新往回收集，优先保留最近的工具调用链与提问
  - 单条超预算的消息丢弃，但最新 user 提问强制保留
- 新增 4 条上下文裁剪单测
- **变更详情**：[Taolun → 2026-07-21 上下文压缩](#2026-07-21--上下文压缩省-token) | [项目进度 → 已完成](#已完成)

## [2026.07.21] — v0.8.1 curl 请求体落盘修复
- 修复 `os error 206`（文件名或扩展名太长：Windows 命令行超 32767 字符上限）
- 根因：`chat_completion_stream` / `chat_completion` 把整个 JSON body 用 `-d <body>` 塞进命令行，对话历史累积过长时 curl.exe 无法启动
- 修复：新增 `write_body_file()` 把 body 写入系统临时目录，改用 `-d @file`；`remove_body_file()` 在 curl 进程结束后清理临时文件
- **变更详情**：[Taolun → 2026-07-21 curl body 落盘](#2026-07-21--curl-body-落盘修复-os-error-206) | [项目进度 → 已完成](#已完成)

## [2026.07.21] — v0.8.0 输入框重塑
- 新建 `src/editor.rs`：raw 模式多行行编辑器，替换 `run_repl` 的 `read_line()`
- 支持普通字符 / CJK / 退格 / 左右上下方向键 / Home / End / Del
- **Ctrl+J / Shift+Enter 换行**（多行输入），多行内容按消息整段发给 AI
- **斜杠命令候选抽屉**：输入 `/` 即弹候选，↑↓ 选择、Enter/Tab 补全、Esc 关闭；首 token 已完整时 Enter 直接提交
- 修复 raw 模式下换行、CJK 宽度软换行、光标定位等细节
- **修复抽屉绘制重叠 bug**：`crossterm::MoveTo` 实参顺序为 `(列, 行)`，原代码误写成 `(行, 列)`，导致提示符/输入画到错误行列、触发 `PowerS> 你好>` 式残影叠加。改用正确顺序后，输入区 + 抽屉重绘在所有终端宽度下均无重叠
- 新增 17 个单元测试（布局、光标、CJK、抽屉过滤、多宽度重绘不重叠）
- **变更详情**：[Taolun → 2026-07-21 输入框重塑](#2026-07-21--输入框重塑斜杠候选抽屉--多行编辑) | [项目进度 → 已完成](#已完成)

## [2026.07.21] — v0.7.1
- 修复 `woman ai` 输入到行尾后不换行、从行首覆盖的问题
- 新增 `ensure_autowrap()`：每轮读取输入前发送 `\x1b[?7h` 重新启用终端自动换行
- 解决 raw mode / 备用屏幕 / 子进程控制序列导致的自动换行被关闭
- 改为问题与 AI 回答之间隔一个空行：发送问题后空一行、AI 回答结束后空一行（工具调用命令前的 `\n` 同步移除，避免双空行）
- **变更详情**：[Taolun → 2026-07-21 输入不换行](#2026-07-21--输入不换行-bug-修复) | [项目进度 → 已完成](#已完成)

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

## 2026-09-01 — CI / Release 三平台构建
### 讨论摘要
- 用户诉求：项目已从 Windows 专用改为**全平台 man 替代品**（v0.11.0），但现有 GitHub Actions 仍只构建 Windows，需要让 CI 跟上跨平台
- 现状：`build.yml` 仅在 `windows-latest` 构建且不跑测试；`release.yml` 同样只在 Windows 构建一个产物
- 改动：两个 workflow 均改为**三平台矩阵构建**（Windows / macOS / Linux）
  - 依赖（dirs/serde/ratatui/crossterm）均为纯 Rust 跨平台，无需额外 target 安装
  - 矩阵含 `os` / `suffix`（产物命名）/ `binary`（产物文件名差异：win 有 `.exe`）
  - `build.yml` 增加 `cargo test`（先在 debug 跑测试再 release 构建），并上传产物
  - `release.yml` 三平台构建 + 统一命名 `woman-<tag>-<os>` 上传 Release（softprops/action-gh-release）
- 产物命名：Windows `x64-windows` / macOS `aarch64-apple`（macos-latest 为 arm64）+ `x64-apple`（macos-13 为 Intel x64）/ Linux `x64-linux`
- 后续补充：用户指出未发布新版本且缺 x64 mac —— 新增 `macos-13`（Intel x64）矩阵条目 `x64-apple`，成四平台产物互补

### 涉及文件
- `.github/workflows/build.yml` — 三平台矩阵 + `cargo test` + 上传产物
- `.github/workflows/release.yml` — 三平台矩阵 + 统一命名 Release 产物
- `AGENTS.md` — 记录

### 相关变更
- [Changelog → CI/Release 三平台构建](#20260901--cirelease-三平台构建) | [项目进度 → 已完成](#已完成)

## 2026-09-01 — 跨平台：全平台 man 替代（Windows / macOS / Linux）
### 讨论摘要
- 用户诉求：把 `woman` 从 Windows 专用改为**全平台 man 替代品**，让 mac/linux 也能用
- 用户提出用「系统变量检测 OS」——经调研建议用 `cfg!(target_os)` / `std::env::consts::OS`（编译期硬保证，比自定义 env var 更稳），coreutils「是否装了」用**二进制探测**（`which`/`where`）运行时判断——即用户「Win 上检测 coreutils」思路的可靠落地
- 四项决策（用户确认）：
  1. **范围**：全平台 man 替代（不满足于最小编译跑通）
  2. **OS 检测**：`cfg!`/`std::env::consts::OS`，不引入自定义环境变量
  3. **AI shell**：三平台都让 AI 用 shell 执行（Windows pwsh / Unix sh），不降级禁用
  4. **coreutils**：Win 探测 `coreutils.exe`，Unix 直接用系统二进制（系统命令即是 man 目标），不再额外探测 GNU coreutils 套件
- **核心方案：单一平台抽象层 `src/platform.rs`**，把所有 `cfg!`/OS 分叉收敛到一处，返回平台相关常量 + 探测函数，其余模块调用它，避免散落 OS 分支
  - 平台差异映射：curl→curl.exe/curl；shell→pwsh -NoProfile -Command /$SHELL·sh -c；定位→where.exe/which；清屏→cmd /c cls/ANSI；coreutils→探测/系统二进制；帮助→--help·/? /--help·man；系统源→Get-Help·MS Learn/man·archlinux·WHAT IS；PowerShell 分类→启用/停用
- 非 Windows 上 PowerShell/Windows 专属源**编译期 `#[cfg(windows)]` 排除**（干净无死代码，虽是运行时禁用也可行但留下死代码）
- 已验证可跨平台的部分：依赖（dirs/serde/ratatui/crossterm）、路径（全用 PathBuf::join）、TUI/编辑器（crossterm）
- 分步实施：1) platform.rs 2) catalog curl 3) fetch 分类/取源 4) ai shell+prompt+黑名单 5) skill 模板 6) main 7) 版本+AGENTS 8) 编译实测

### 涉及文件
- `src/platform.rs`（新建）— 平台枚举 + curl/shell/which/coreutils/帮助/系统源抽象
- `src/fetch.rs` — curl 平台化、coreutils 分类、Get-Help(#[cfg(windows)])、新增 man/WHAT IS 源、run_help 去 /?
- `src/ai.rs` — run_bash shell 分平台、SYSTEM_PROMPT/TOOLS 平台化、clear_screen、危险命令黑名单分平台
- `src/catalog.rs` — curl 平台化
- `src/skill.rs` — 默认模板平台通用化
- `src/main.rs` — 按平台裁剪取源
- `AGENTS.md` — 更新

### 相关变更
- [Changelog → v0.11.0](#20260901--v0110-跨平台全平台-man-替代) | [项目进度 → 已完成](#已完成)

## 2026-09-01 — woman init：交互式初始化向导 + models.dev 目录（本地缓存离线可用）
### 讨论摘要
- 用户诉求：为开源后用户提供 `init` 命令（初始化 + 重置），并希望像 opencode / pi / deepseek-harness 那样用**开源 LLM 厂商/模型目录**做交互式选择厂商+模型
- 确认目录来源 = **models.dev**（MIT，opencode/pi/deepseek-harness 同源）：`curl https://models.dev/api.json`
  - 顶层 `HashMap<providerId, Provider>`（212 家 / 7478 模型）；Provider 含 `id/name/npm/doc/api?/models`；Model 含 `id/name/limit/cost/modality/status/reasoning/tool_call`
  - **OpenAI 兼容过滤**：`npm == "@ai-sdk/openai-compatible"` 且含 `api`（约 170 家）才直接适配 woman 的 `AiProvider`（api_base+api_key+model）；anthropic/google/groq 等原生 SDK 过滤/提示需经 OpenAI 兼容网关
  - 文件 ~4.4MB 单行，`serde_json::from_str` 即可
- 目录**本地缓存离线可用**：缓存 `~/.woman/models/catalog.json`；三级——有缓存直接用（离线）、无缓存拉取、拉取失败且无缓存才报错（提示先联网跑一次）
- 向导可**重复**进入（追加/更换提供者，兼容现有多提供者 ai[]）；无配置自动进向导，`--interactive` 强制重进
- 命令面：`woman init`（向导）、`init --refresh`（重拉目录）、`init --reset`（恢复默认 config+skill）、`init --reset --wipe`（另清 docs/cache）
- 解析用**精简 serde 结构**（`#[serde(default)]`，忽略未知字段），不引入完整巨型结构
- 分步实施：1) config 路径+add_provider 2) catalog 拉取解析 3) init 向导+掩码输入 4) main 解析+help 5) 版本+AGENTS 6) 编译测试实测

### 涉及文件
- `src/config.rs` — `models_dir()`/`catalog_path()`；`Config::add_provider()`（去重+设 default+落盘）
- `src/catalog.rs`（新建）— models.dev 目录缓存/拉取/精简 serde 解析
- `src/init.rs`（新建）— `run_init()` 向导 + `prompt_masked()`/`prompt_line()`
- `src/main.rs` — `init` + `--refresh/--reset/--wipe` 解析 + help
- `AGENTS.md` — 更新

### 相关变更
- [Changelog → v0.10.0](#20260901--v0100-woman-init交互式初始化-向导--modelsdev-目录) | [项目进度 → 已完成](#已完成)

## 2026-09-01 — 手册详细化 + 本地选项校对（AI 校验）
### 讨论摘要
- 用户两个诉求：
  1. **更像手册 + 离线可用**：docs 里的手册应是「详细完整手册」，而非当前 AI 强调「重点讲常用、不罗列冷门」的摘要——列入全部选项/参数、更完整示例，作为离线参考
  2. **在线手册按本地命令选项来**：coreutils 本机构建不一定 100% 兼容在线 man page 列出的所有选项，在线手册应校对本地的
- 三项确认（用户选择）：
  - docs 形态 = **详细完整手册**（收录全部选项，最像 man；代价是文件更大、AI 每次生成耗更多 token）
  - 在线有、本机 --help 不支持的选项 = **保留并标注『本机此版本不支持』**（透明告知差异，而非静默省略）
  - 无 AI key 时 = **离线也抓在线全文**（取源时 coreutils 同时抓 archlinux 全文缓存展示，本地短 `--help` 仅兜底），保证离线 `woman <name>` 也有详细手册
  - 本地选项校对实现 = **AI 校对**（把本地 --help 与在线全文都喂给 AI，由 AI 对比选项并标注不支持项，复用现有 enhance/chat_completion；放弃 Rust 启发式解析 man 选项，因其不可靠）
- 核心设计变更：`fetch_source` 返回值从 `(source_label, text)` 扩展为可携带**双资料**（本地 --help = 选项真值 + 在线全文 = 详细说明）；coreutils 此前 `--help` 成功就不抓 archlinux，需改为两者都取
- 分步实施：1) fetch 双资料 FetchedSource 2) skill 详细模板 + 校对指令 3) enhance 接双资料 4) main 适配 5) AGENTS 记录 6) 编译测试实测

### 涉及文件
- `src/fetch.rs` — `FetchedSource` 结构 + `fetch_source` 双资料取源
- `src/skill.rs` — 详细完整手册模板 + 「本地选项校对」指令 + 测试
- `src/ai.rs` — `enhance()` 接受本地/在线双资料 + 拼接校对
- `src/main.rs` — `lookup_and_show` 适配 combined 流程 + 无 AI 展示详细全文
- `AGENTS.md` — 更新

### 相关变更
- [Changelog → v0.9.1](#20260901--v091-手册详细化--本地选项校对) | [项目进度 → 已完成](#已完成)

## 2026-09-01 — woman 重构为 man 替代品 + AI 可选智能渲染层
### 讨论摘要
- 用户目标：把 `woman` 纯粹化为 Windows 上的 man 替代品，专为 GNU coreutils 命令 + PowerShell 指令提供服务（man 在 Windows 上不认 coreutils.exe）
- 单二进制 + 单命令：`woman <name>` 即 man 查询，对齐 man 使用逻辑
- AI 作为可选智能渲染层：有 key 时把原始 man 内容整理成结构化中文手册；无 key 时直接展示原始内容（纯 man，完全可用）
- 全自动对齐 man：docs 未命中 → 分类命令（coreutils/powershell/windows）→ 取源 → AI 整理写 docs → 展示；取消手动 search/generate
- 命令面最终确定：`woman <name>`、`woman -q "<问题>"`（一次性问答，agent 式，docs/cache 作上下文，无 key 报错）、`woman ai`（保留自由对话）；**移除** `search`/`generate`/`-s`
- 手册风格解耦：skill 文件 `~/.woman/skills/manual.md`（章节结构模板 + 给 doubao 的整理指令），用户可自定义，多风格可扩展
- PowerShell 兼容：先 coreutils 后 `Get-Help`（新增源）；`ls.exe`/`ls` 均归 coreutils
- 多类型冲突时用 mdr 卡片式选择器让用户选择（参考 `C:\Users\fools\Projects\mdr` 的文件选择器效果）
- 数据模型：docs=`*.md`（整理后手册，人类可读，无 AI 直出）、cache=`.txt`（源原始副本，AI 输入 + 兜底）；coreutils 命令清单缓存到 `~/.woman/coreutils-list.txt` 供分类
- 评估过引入 nosqlite（本地 NoSQL 数据库）存 docs/cache，后放弃：docs/cache 的价值在于"人类可读、可直接编辑的 markdown/text 文件"，这是 man 的本质，塞进二进制 .nodb 会破坏核心价值，且 woman 的访问模式（按 name 精确读写单条）用不上 nosqlite 的查询能力；nosqlite 更适合辅助层（如 -q 检索索引），当前暂不引入
- 分步实施：1) 分类+Get-Help+coreutils清单 2) skill+enhance() 3) 重写 lookup_and_show+删 search/generate 4) -q 5) mdr 选择器 6) 收尾

### 涉及文件
- `src/fetch.rs` — `fetch_from_gethelp`、`classify_command`、coreutils 清单缓存、源选择
- `src/docs.rs` — frontmatter 加 `type`/`tool_version`；`ai-enhanced` 徽标；cache 改 `.txt` + 旧 `.md` 兼容读取
- `src/skill.rs`（新建） — `manual.md` 读取/默认模板
- `src/ai.rs` — 删 `generate_docs`；新增 `enhance()`/`ask_once()`；system prompt 调整
- `src/config.rs` — `skills_dir()`/`coreutils_list_path()`
- `src/tui.rs` — mdr 卡片选择器
- `src/main.rs` — 删 `search/generate/-s`；加 `-q`；重写 `lookup_and_show`
- `AGENTS.md` — 更新

### 相关变更
- [Changelog → v0.9.0](#20260901--v090-重构为-man-替代品--ai-可选智能渲染层) | [项目进度 → 已完成](#已完成)

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

## 2026-07-21 — 上下文压缩省 token
### 讨论摘要
- 用户关心工具浪费 token，但明确要求「保留对话准确性」
- 分析结论：最大浪费来自「整段历史每轮全量重发」+「工具输出最大 5 万字符完整保留」
- 方案三层，保准确优先：
  1. 工具输出瘦身：`run_bash` 的 `MAX_OUTPUT` 50000 → 12000。AI 只需提炼要点，省 token 几乎不损失准确性，单次省最多 3.8 万字符
  2. 新增 `trim_context()`：字符预算（120k ≈ 30k token）自动裁剪。system 永保；从最新往回收集，近工具链优先，旧纯对话先丢；最新提问强制保留
  3. 每轮请求前调用 `trim_context(&messages)`
- 准确性保障：裁剪只发生在超预算时；预算内原样返回；最近回合（含工具调用链）优先保留

### 涉及文件
- `src/ai.rs` — `run_bash` 截断 50000→12000；新增 `trim_context()`/`msg_chars()`；`RequestMessage` 加 `Clone`/`PartialEq`；`run_repl` 请求前调用裁剪；新增 context_tests 4 例

### 相关变更
- [Changelog → v0.8.2](#20260721--v082-上下文压缩省-token) | [项目进度 → 已完成](#已完成)

## 2026-07-21 — curl body 落盘修复 os error 206
### 讨论摘要
- 用户反馈：执行过程中报「无法启动 curl.exe：文件名或扩展名太长 (os error 206)」
- 根因：Windows `CreateProcess` 命令行长度上限 32767 字符。`chat_completion_stream` / `chat_completion` 直接把序列化后的 JSON body 用 `-d <body>` 作为命令行参数传给 curl.exe；当 REPL 对话历史不断累积（尤其工具调用结果很长）时，命令行超出上限导致 curl 无法启动
- 方案：新增 `write_body_file()` 把 body 写入 `std::env::temp_dir()` 下的 `woman_body_<pid>.json`，curl 改用 `-d @file` 从文件读取；新增 `remove_body_file()` 在 curl 进程结束后删除临时文件
- 清理时机：流式版本在 `child.wait()` 后清理（`-d @file` 是 curl 启动时读取，过早删除有竞态）；非流式 `.output()` 阻塞至完成，随后清理；spawn 失败时也清理
- 进程内多次调用共用 `<pid>` 名，顺序执行相互覆盖安全

### 涉及文件
- `src/ai.rs` — 新增 `write_body_file()`、`remove_body_file()`；`chat_completion_stream` / `chat_completion` 的 `-d` 改为落盘方式

### 相关变更
- [Changelog → v0.8.1](#20260721--v081-curl-请求体落盘修复) | [项目进度 → 已完成](#已完成)

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
- 旧 `.md` cache 迁移到 `.txt`（当前仅兼容读取，未做重命名迁移）

### 代办
- （无）

### 已完成
- [x] CI / Release 三平台矩阵构建：`build.yml`（三平台 + `cargo test` + 上传产物）、`release.yml`（三平台 + 统一命名 `woman-<tag>-<os>` 上传 Release）；补充 `macos-13`（Intel x64）成四平台（Windows x64 / macOS arm64+x64 / Linux x64）— [Taolun → 2026-09-01 CI/Release](#2026-09-01--cirelease-三平台构建) | [Changelog → CI/Release 三平台构建](#20260901--cirelease-三平台构建)
- [x] 跨平台全平台 man 替代（Windows/macOS/Linux）：新增 `src/platform.rs` 统一平台抽象层收敛全部 cfg! 分叉；AI shell 三平台可用（pwsh/sh）+ `is_dangerous` 分平台黑名单；`system_prompt`/`tools_json` 分平台实体；coreutils 分类（Win 探 coreutils.exe / Unix 用系统二进制）；man/whatis Unix 源；Get-Help 编译期 cfg 排除；skill 模板通用化 — [Taolun → 2026-09-01 跨平台](#2026-09-01--跨平台全平台-man-替代) | [Changelog → v0.11.0](#20260901--v0110-跨平台全平台-man-替代)
- [x] `woman init` 交互式初始化向导：models.dev 目录缓存离线可用（212 厂商/172 OpenAI 兼容/7478 模型解析通过）、选厂商/模型/掩码输 Key、可重复进向导增改提供者、`--refresh/--reset/--wipe` — [Taolun → 2026-09-01 woman init](#2026-09-01--woman-init交互式初始化-向导--modelsdev-目录) | [Changelog → v0.10.0](#20260901--v0100-woman-init交互式初始化-向导--modelsdev-目录)
- [x] 手册详细化 + 本地选项校对：skill 模板改详细完整手册（列全部选项）；`fetch_source` 双资料（本地 --help 真值 + 在线全文）；`enhance` AI 校对本机不支持选项并标注『本机此版本不支持』；无 AI 也抓在线全文缓存展示 — [Taolun → 2026-09-01 手册详细化](#2026-09-01--手册详细化--本地选项校对ai-校验) | [Changelog → v0.9.1](#20260901--v091-手册详细化--本地选项校对ai-校验)
- [x] 重构为 man 替代品 + AI 可选智能渲染层：单命令 `woman <name>` 全自动取源；`-q` 问答；删 search/generate/-s；分类（coreutils/powershell/windows）+ Get-Help 源；skill 手册风格；mdr 卡片选择器；cache 改 .txt；docs type/ai-enhanced — [Taolun → 2026-09-01 重构](#2026-09-01--woman-重构为-man-替代品--ai-可选智能渲染层) | [Changelog → v0.9.0](#20260901--v090-重构为-man-替代品--ai-可选智能渲染层)
- [x] 上下文压缩省 token：`run_bash` 截断降为 12000 + 新增 `trim_context()` 自动裁剪 — [Taolun → 2026-07-21 上下文压缩](#2026-07-21--上下文压缩省-token)
- [x] 修复 `woman ai` 超长对话导致 curl.exe 无法启动（os error 206，请求体改落盘 `-d @file`） — [Taolun → 2026-07-21 curl body 落盘](#2026-07-21--curl-body-落盘修复-os-error-206)
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
- [x] 修复`woman ai`输入到行尾不换行的 bug — [Taolun → 2026-07-21 输入不换行](#2026-07-21--输入不换行-bug-修复) | [Changelog → v0.7.1](#20260721--v071)
- [x] 输入框重塑：slash 候选抽屉 + 多行编辑（Ctrl+J 换行） — [Taolun → 2026-07-21 输入框重塑](#2026-07-21--输入框重塑斜杠候选抽屉--多行编辑) | [Changelog → v0.8.0](#20260721--v080-输入框重塑)

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

## 2026-07-21 — token 浪费的主因与取舍
- **发现**：上下文最大浪费不是单次对话，而是「整段历史每轮全量重发」+「工具结果 5 万字符完整保留且永久留档」
- **取舍**：粗暴丢历史会伤准确性；但工具输出瘦身 + 预算内优先保留工具链 + 最新提问兜底，可以在几乎不损失准确性的前提下大幅省 token
- **教训**：工具输出的返回长度 == 后续每轮请求的复读成本（读 N 次）。输出用于提炼信息，AI 不需要 5 万字符原文，12000 足够。若未来确需大输出，应改为「摘要压缩」而非原文

## 2026-07-21 — curl -d 与 Windows 命令行长度限制
- **踩坑**：直接把 AI 请求 body 用 `-d <body>` 塞进命令行，对话历史累积后触发 os error 206（文件名或扩展名太长），curl.exe 无法启动
- **纠正**：长 body 一律用 `-d @file`，把 JSON 写入系统临时目录由 curl 自行读取，命令行参数只保留短路径
- **教训**：AGENTS.md v0.3.0 阶段就记录过「超长消息需改用 `-d @file`」，但后续实现一直没落地，直到生产环境真正撑爆。涉及 Windows `CreateProcess` 32767 字符限制时，写入文件是唯一可靠出路

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

## 2026-07-21 — 输入不换行 Bug 修复
### 讨论摘要
- 用户反馈：`woman ai` 中输入一行时，输到最后一个字符后，后续字符会从这行行首覆盖替换，即终端不自动换行
- 根因：DECAWM（自动换行，`\x1b[?7h`）在部分场景被意外关闭——如 `/model` 下拉框的 raw mode、TUI 备用屏幕、工具循环中子进程 `pwsh` 输出控制序列等都会改变终端状态
- 方案：新增 `ensure_autowrap()`（幂等写入 `\x1b[?7h`），在 REPL 每轮读取用户输入前调用，保证自动换行始终开启
- 放在每轮循环开头而非只在固定位置，是因为任何一次子进程输出都可能带来意外状态，循环开头调用最稳妥

### 涉及文件
- `src/ai.rs` — 新增 `ensure_autowrap()`；`run_repl` 输入循环开头调用

### 相关变更
- [Changelog → v0.7.1](#20260721--v071) | [项目进度 → 已完成](#已完成)

## 2026-07-21 — 问题与回答空行
### 讨论摘要
- 用户反馈：默认回答时，AI 回答与输入的问题上下行贴合太紧，希望中间增加一个空行
- 方案：发送问题给 AI 前 `println!()` 空一行；`Complete` 分支回答结束后 `println!()` 空一行，与下一问题分隔
- 响应：工具调用命令块原本以 `\n` 开头，现在外部已统一空一行，去掉该 `\n` 避免出现双空行

### 涉及文件
- `src/ai.rs` — `run_repl`：问题后、回答结束各加一个空行；ToolCall 分支去掉命令块前缀 `\n`

### 相关变更
- [Changelog → v0.7.1](#20260721--v071) | [项目进度 → 已完成](#已完成)

## 2026-07-21 — 输入框重塑：斜杠候选抽屉 + 多行编辑
### 聊天记录
- 用户反馈当前输入框：无法像 pi 那样输入 `/` 就弹出智能候选；且不支持多行换行输入
- 用户明确要：1) 像 pi 一样输入 `/` 弹候选；2) 候选内容=内置命令；3) 抽屉式（覆盖式，↑↓ 选择）；4) 输入框支持 Ctrl+J 换行多行输入

### 讨论摘要
- 现状：`run_repl` 用 `io::stdin().read_line()` 整行读取，无法实时捕获按键、无法弹候选、无法多行
- 方案：新建 `src/editor.rs` 模块，实现「raw 模式多行行编辑器」
  - 逐键读取（crossterm `event::read`），维护缓冲区 + 光标字节偏移
  - 支持：普通字符/CJK 插入、退格、左右/上下方向键（光标移动）、Home/End、Del、Ctrl+J 换行（多行）
  - Enter：若斜杠抽屉可见则选择补全到输入；否则提交整段输入
  - 以 `/` 开头时计算内置命令匹配项，在输入区下方渲染抽屉候选（橙底选中），↑↓ 选择、Enter/Tab 补全、Esc 关闭
- 渲染策略：编辑区以「当前光标所在初始行」为锚点，每次重绘先 `Goto` 锚点行 + `Clear(FromCursorDown)` 清空下方，再重绘输入区 + 抽屉，最后把硬件光标放到插入点
- 字符宽度按 CJK 宽字符=2、其他=1 估算并计算软换行，兼容中文输入
- 抽屉在输入区下方、未进备用屏，保持普通滚动区
- Ctrl+J 在 Windows crossterm 中映射为 `KeyCode::Char('j')` + `CONTROL` 修饰键（源码 parse.rs 已确认），编辑器需同时兼容 `Char('\n')`
- 命令分派仍留在 `run_repl`：编辑器只返回「最终字符串」，是否 / 命令由 run_repl 按是否单行且以 / 开头判断

### 涉及文件
- `src/editor.rs` — 新建多行行编辑器 + 斜杠候选抽屉
- `src/ai.rs` — `run_repl` 输入部分改为调用编辑器

### 相关变更
- [Changelog → v0.8.0](#20260721--v080-输入框重塑) | [项目进度 → 已完成](#已完成)


## 2026-07-21 — crossterm MoveTo 参数顺序
- **踩坑**：实现 `/` 抽屉 + 多行编辑器的重绘时，用 `MoveTo(anchor_row, 0)` 定位，结果提示符和输入全被画到第 0 行 + 右偏 anchor 列，触发 `PowerS> 你好>` 式的残影叠加
- **根因**：crossterm `cursor::MoveTo(col, row)`，第一个参数是**列**、第二个是**行**（源码 `write_ansi` 输出 `\x1b[{row+1};{col+1}H`）。原代码把行当成了第一参数
- **纠正**：改为 `MoveTo(0, anchor_row)`（列 0，行 anchor）；光标归位同理 `MoveTo(cur_col, anchor_row+cur_row)`
- **教训**：crossterm 的 `MoveTo`/`MoveToColumn` 参数顺序不是直觉的 `(行,列)`，而是 `(列,行)`。凡用到坐标命令一律先查源码签名，避免隐性绘制错乱
- **验证**：新增「多终端宽度重绘不重叠」测试（8~24 列遍历 + 屏幕模拟器重建字节流），确保任意宽度下输入字符不重复、提示符只出现一次


## 2026-07-21 — Bash is Everything 认知修正
- **发现**：4 个硬编码工具的 doubao 兼容代码（`<|FunctionCallBegin|>` 标记）和工具派发逻辑（`execute_tool`）全部可以被一个 `bash` 工具替代
- **纠正**：删除 4 个工具 → 用单一 `bash` 工具 + `run_bash()` 函数执行任何 PowerShell 命令
- **教训**：工具越少，AI 越自由。硬编码搜索源（archlinux / MS Learn）不如让 AI 自己用 `curl.exe` 决定怎么查。Rust 代码量减少不等于能力减少，反而更灵活
- **教训**：system prompt 需要准确描述执行环境（coreutils `.exe` 后缀、pwsh 路径倒斜杠），否则 AI 生成的命令会在 Windows 上报错

---> **CoreUtils 使用规范**：`grep` `ls` `sed` `find` 等命令用法详见 `~/.config/opencode/docs/coreutils.md`
