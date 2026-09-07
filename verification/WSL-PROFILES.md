# Windows Shell / WSL 选择入口实测

2026-09-08，当前 Windows 11，Release 构建。

- 标签栏 `+` 旁增加 `▾`，打开终端选择面板；在后台发现本机 Shell 和 WSL。
- 本机识别：Windows PowerShell、命令提示符、PowerShell 7、Ubuntu-24.04、CentOS-7。
- `wsl --list --verbose` 确认两发行版均为 WSL2。
- `/etc/os-release` 确认 Ubuntu 24.04.4 LTS 和 CentOS Linux 7 (Core)。
- 两发行版均通过真实 ConPTY 中文输入输出测试，工作目录为 `/mnt/d/code1111111111/devhub-source-0.1.0/devhub`。
- 从实际 GUI 的选择入口分别启动 Ubuntu 和 CentOS，xterm.js 中中文回显、`uname -s`、`pwd` 均正常。
- 切回 Ubuntu 后保留 Ubuntu 自己的输出，没有混入 CentOS 输出。
- WSL Shell 初次启动可能需要数秒；测试输入在启动期间排队后正常执行。
- 60 项 Rust Release 测试通过，0 失败；覆盖 WSL UTF-16/UTF-8 列表解码、含中文空格和 `&` 的路径保持独立参数。

启动配置写入原有会话元数据；重新打开普通 Shell/WSL 会话继续使用保存的命令，不再误换成 OMP。
WSL 使用发行版自己的默认用户和 Shell；Windows 项目目录通过 `--cd` 交给 WSL 转换。
没有自动安装 WSL、安装 OMP、改变默认发行版或关闭其他 WSL 实例。
这里验证的是 Windows GUI 连接 WSL 内的 Shell，不代表 Linux GUI 在公司 CentOS/X11 环境已通过兼容验收。

复测真实 PTY（自动发现已安装发行版，不执行网络请求或软件安装）：

```powershell
$env:TEMP="$PWD/.tmp"; $env:TMP=$env:TEMP
$env:CARGO_HOME="$PWD/.tmp/cargo-home"; $env:CARGO_TARGET_DIR="$PWD/.tmp/target"
cargo test --release --locked --offline --lib
cargo run --release --locked --offline --example wsl_profiles_check
```

本轮日志和截图位于 `.tmp/wsl-*`，不提交版本管理。
