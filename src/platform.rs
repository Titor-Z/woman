// platform.rs — 平台抽象层
// 把散落在各模块的 OS 分支（cfg!(target_os)）集中到这一处，
// 其余模块统一调用这里的常量与探测函数，避免在业务代码里散布 cfg! 分叉。
//
// 平台差异映射（Windows / macOS・Linux）：
// - curl       ：curl.exe                  / curl
// - shell      ：pwsh -NoProfile -Command  / sh -c
// - 命令定位   ：where.exe                 / which
// - 清屏       ：cmd /c cls                / ANSI 转义序列

use std::process::Command;

/// HTTP 客户端二进制（Windows 是 `curl.exe`，Unix 是 `curl`）
pub fn curl_bin() -> &'static str {
    if cfg!(target_os = "windows") {
        "curl.exe"
    } else {
        "curl"
    }
}

/// shell 执行程序与固定前缀参数，供 AI 的 run_bash 工具使用。
/// 返回 `(程序, 前缀参数)`，调用方再追加一条命令字符串。
/// - Windows：PowerShell `pwsh -NoProfile -Command <cmd>`
/// - Unix   ：POSIX sh `sh -c <cmd>`（用 sh 而非 $SHELL，避免不同 shell 语法差异）
pub fn shell_runner() -> (&'static str, &'static [&'static str]) {
    if cfg!(target_os = "windows") {
        ("pwsh", &["-NoProfile", "-Command"])
    } else {
        ("sh", &["-c"])
    }
}

/// 命令定位二进制：Windows `where.exe`，Unix `which`
pub fn which_bin() -> &'static str {
    if cfg!(target_os = "windows") {
        "where.exe"
    } else {
        "which"
    }
}

/// 探测某个可执行文件是否存在于 PATH（运行时判断，跨平台）。
/// Windows 用 `where.exe <name>`，Unix 用 `which <name>`。
/// 当前仅在 Unix 的命令分类（which 探测系统二进制）中使用，故标记允许死代码，
/// 以避免 Windows 编译时因 cfg 裁剪而告警。
#[allow(dead_code)]
pub fn command_exists(bin: &str) -> bool {
    Command::new(which_bin())
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 清屏：返回指示如何清屏。
/// - Windows：返回 ("cmd", ["/c", "cls"])，由调用方作为子进程执行。
/// - Unix   ：返回 ("__ansi__", ["\x1b[2J\x1b[H"])，由调用方直接写入终端。
pub fn clear_screen() -> (&'static str, Vec<String>) {
    if cfg!(target_os = "windows") {
        ("cmd", vec!["/c".to_string(), "cls".to_string()])
    } else {
        ("__ansi__", vec!["\x1b[2J\x1b[H".to_string()])
    }
}
