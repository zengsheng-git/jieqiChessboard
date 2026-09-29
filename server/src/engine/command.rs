use std::process::Child;
use std::process::ChildStdin;
use std::process::Command;
use std::process::Stdio;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

/// 启动引擎子进程。
/// 返回: (子进程, 命令写入口, 克隆出的 stop 写入口——存放在引擎锁之外, 用于中断搜索)
#[cfg(target_os = "windows")]
pub fn new(libs: &std::path::Path) -> std::io::Result<(Child, ChildStdin, Option<ChildStdin>)> {
    use std::os::windows::io::OwnedHandle;
    let mut child = Command::new(libs.join("pikajieqi-windows.exe"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()?;
    let stdin = child.stdin.take().unwrap();
    let owned = OwnedHandle::from(stdin);
    let stop_stdin = owned.try_clone().ok().map(ChildStdin::from);
    let command_stdin = ChildStdin::from(owned);
    Ok((child, command_stdin, stop_stdin))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn new(libs: &std::path::Path) -> std::io::Result<(Child, ChildStdin, Option<ChildStdin>)> {
    use std::os::fd::OwnedFd;
    let exe = if cfg!(target_os = "macos") { "pikajieqi-macos" } else { "pikajieqi-linux" };
    let mut child = Command::new(libs.join(exe))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take().unwrap();
    let owned = OwnedFd::from(stdin);
    let stop_stdin = owned.try_clone().ok().map(ChildStdin::from);
    let command_stdin = ChildStdin::from(owned);
    Ok((child, command_stdin, stop_stdin))
}
