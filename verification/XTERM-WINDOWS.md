# Windows 内嵌 xterm.js 实测 — 2026-09-08

Windows 默认终端改为 xterm.js 6.0.0 + wry 0.56.1 / WebView2。
现有 Iced 侧栏、标签、文件预览和 Rust ConPTY 保留。终端字节直接交给 xterm，原 WezTerm 引擎不再解析该会话输出。
JS、CSS、Fit 和 Unicode11 插件随可执行文件打包，无运行时 CDN/npm 依赖。
未启用 WebGL，并通过 `--disable-gpu` 禁用 WebView GPU。

## 当前机器实测

Windows 11 10.0.26200，Ryzen 9 9950X，Rust 1.98.0 MSVC Release；
WebView2 152.0.4191.66，OMP v18.1.10+fork.172，初始窗口 1240×820。
全部在独立 `.tmp/` 状态/会话目录测试，未提交聊天提示词、选择模型或修改模型配置。

| 检查 | 结果 |
| --- | --- |
| 真实 OMP `/model` 重复打开 10 次 | 回车 DOM 事件至完整面板 onRender：最小 142.5 ms，中位数 215.5 ms，最大 523.3 ms |
| 模型面板打开期间 WebView 帧间隔 | 最大 16.8 ms |
| 真实 ConPTY 连续输出 | 10,000 行含中文内容约 254.5 ms；逐行校验编号、中文和尾部字符，完整有序 |
| 连续输出期间 WebView 16 ms 定时器 | 最大间隔 29.1 ms |
| 130,000 字节中文/emoji 输入 | 跨 32 KiB 批次，子进程 SHA-256 与原文本一致 |
| 终端功能 | 中文往返、alternate screen、恢复主屏、滚动到历史顶部、输出后继续输入均通过 |
| GUI 集成 | 新建两个 OMP 标签、切换并保留各自画面、侧栏拖动后 114→103 列、关闭确认显示/取消、预览与终端互切通过 |
| Rust Release 测试 | 58 通过，0 失败，包含真实 PTY 等待前端/输入确认/绕过原解析器测试 |
| 原终端回退 | `--native-terminal` GUI 冒烟正常自动退出 |

10 次 `/model` onRender 原始毫秒数：154.9、523.3、212.4、157.7、218.6、231.6、247.9、229.9、142.5、147.0。
这是启动完成并预先打开过面板后的重复测试，不是冷启动基准。
onRender 与下一次 requestAnimationFrame 是绘制调度指标，不是物理键盘到屏幕像素的精确延迟。
WebView 帧间隔与以前 Iced 主线程定时探针的指标不同，不能直接作为严格速度倍数比较。

## 实现与兼容边界

输出每会话最多一个约 64 KiB 批次等待 xterm 完成确认；后端读队列最多 128×8192 字节。
输入最多缓存 1 MiB，每次最多 32 KiB，PTY 实际写入后才继续下一批。重启使用独立 epoch，拒绝旧实例回调。
该路径绕过原终端解析和快照；为减少平台改动，Session 中原引擎对象仍存在但不处理数据。

Windows 需要已安装 WebView2 Runtime，用户数据目录为工作目录 `.tmp/webview/<pid>`。
WebView 进程增加内存与运行时依赖，本轮没有进行与原版的完整内存对照。
Linux 仍使用原终端；`cargo tree --target x86_64-unknown-linux-gnu --locked --offline` 确认不包含 wry、GTK、WebKit。
锁文件未删除或替换原有依赖版本。没有内部老系统实机结果，不能声称 CentOS 7 已验收。
中文测试使用 CDP 已提交文本；Windows 输入法候选窗口、远程桌面、完整 vim 行为尚未验收。

## 复测

在项目根目录执行，临时输出、依赖缓存和独立状态都放在 `.tmp/`：

```powershell
$env:TEMP="$PWD/.tmp"; $env:TMP=$env:TEMP
$env:CARGO_HOME="$PWD/.tmp/cargo-home"; $env:CARGO_TARGET_DIR="$PWD/.tmp/target"
cargo test --release --locked --offline --lib
cargo rustc --release --locked --offline --bin agentdock -- -o "$PWD/.tmp/agentdock-xterm.exe"
$env:AGENTDOCK_WEBVIEW_DEBUG_PORT='19341'
$env:PI_CODING_AGENT_SESSION_DIR="$PWD/.tmp/xterm-omp-sessions"
Start-Process .tmp/agentdock-xterm.exe -ArgumentList '--state-dir .tmp/xterm-test-state --command omp'
node verification/xterm-model-test.mjs 19341
```

PTY 检查另启实例，调试端口 19342，状态目录 `.tmp/xterm-fixture-state`，命令参数：
`--command node.exe --arg <项目绝对路径>/verification/xterm-pty-fixture.mjs`，然后执行
`node verification/xterm-pty-test.mjs 19342`。脚本只向固定测试程序发送数据，不执行用户文本。
脚本使用 Node 24 自带 WebSocket，无 npm 测试依赖。原始 JSON 和截图写入 `.tmp/xterm-*`。
调试端口仅在显式设置 `AGENTDOCK_WEBVIEW_DEBUG_PORT` 时开启；日常运行应移除此环境变量。
