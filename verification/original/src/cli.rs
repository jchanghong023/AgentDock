use crate::{model::Launch, paths};
use anyhow::{Context, Result};
use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub project: Option<PathBuf>,
    pub open: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
    pub launch: Option<Launch>,
    pub self_test: bool,
    pub smoke_ui_ms: Option<u64>,
    pub help: bool,
    pub version: bool,
}
impl Options {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let name = arg.to_str().context("选项名称必须为 UTF-8；路径参数可保留操作系统编码")?;
            match name {
                "--help" | "-h" => options.help = true,
                "--version" | "-V" => options.version = true,
                "--self-test" => options.self_test = true,
                "--project" | "--open" | "--state-dir" => {
                    let value = args.next().with_context(|| format!("{name} 缺少路径"))?;
                    let path = paths::absolute(&PathBuf::from(value))?;
                    match name { "--project" => options.project = Some(path), "--open" => options.open = Some(path), _ => options.state_dir = Some(path) }
                }
                "--command" | "--arg" => {
                    let value = args.next().with_context(|| format!("{name} 缺少参数"))?.into_string().map_err(|_| anyhow::anyhow!("命令及参数必须为 UTF-8"))?;
                    let launch = options.launch.get_or_insert_with(Launch::default);
                    if name == "--command" { launch.program = value; } else { launch.args.push(value); }
                }
                "--smoke-ui-ms" => {
                    let value = args.next().context("--smoke-ui-ms 缺少毫秒数")?;
                    let millis = value.to_str().context("毫秒数须为整数")?.parse::<u64>()?;
                    anyhow::ensure!((500..=60_000).contains(&millis), "GUI 冒烟时长须在 500..60000 毫秒之间");
                    options.smoke_ui_ms = Some(millis);
                }
                _ => anyhow::bail!("未知参数 {name}；使用 --help 查看用法"),
            }
        }
        if options.smoke_ui_ms.is_some() {
            anyhow::ensure!(options.state_dir.is_some(), "GUI 冒烟模式必须显式指定独立的 --state-dir");
            anyhow::ensure!(options.launch.is_none(), "GUI 冒烟模式不接受自定义命令");
        }
        if let Some(launch) = &options.launch { anyhow::ensure!(!launch.program.is_empty(), "--arg 必须配合 --command"); }
        Ok(options)
    }
}
pub const HELP: &str = "DevHub 0.1.0 — 原生终端工作台\n\n\
用法: devhub [--project DIRECTORY] [--command PROGRAM [--arg ARG]...]\n\
              [--open FILE] [--state-dir DIRECTORY]\n\n\
  --project DIRECTORY   添加目录并启动一个新的终端会话\n\
  --command PROGRAM     直接启动命令（默认系统交互 Shell）\n\
  --arg ARG             一个独立的命令参数；可重复；不拼接为 Shell 命令\n\
  --open FILE           显式打开一个只读文件预览\n\
  --state-dir DIRECTORY 使用独立的历史和配置目录\n\
  --self-test           无图形界面的真实 PTY 往返自测\n\
  --smoke-ui-ms N       使用独立状态目录启动 GUI 并自动退出，仅供验证\n\
  --version             版本\n\
  --help                帮助\n\n\
鼠标：单击会话显示终端，双击文件只读预览；目录右侧 ★ 置顶、+ 新建会话。\n\
终端：Ctrl+Shift+C 复制，Ctrl+Shift+V / Shift+Insert 粘贴；Shift+滚轮强制历史滚动。\n\
退出 GUI 不保活会话进程。保存的是元数据，不是终端进程或 Agent 的聊天内容。";
#[cfg(test)] mod tests {
    use super::*;
    fn parse(s: &[&str]) -> Result<Options> { Options::parse(s.iter().map(|s| OsString::from(*s))) }
    #[test] fn command_keeps_spaces() { let o = parse(&["--command", "tool", "--arg", "a b"]).unwrap(); assert_eq!(o.launch.unwrap().args, vec!["a b"]); }
    #[test] fn rejects_unknown_flags() { assert!(parse(&["--typo"]).is_err()); }
    #[test] fn rejects_missing_arguments() { assert!(parse(&["--project"]).is_err()); }
    #[test] fn smoke_requires_isolation() { assert!(parse(&["--smoke-ui-ms", "2000"]).is_err()); }
    #[test] fn no_implicit_shell_arguments() { assert!(parse(&["--arg", "hello"]).is_err()); }
}
