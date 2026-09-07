# `/model` 打开卡顿修复（Windows，2026-09-08）

问题已在实际 OMP + AgentDock GUI 中复现并修复。此前的历史导入/选区微基准没有覆盖这一瓶颈。

## 原因与实测

同一台 Windows 11 / Ryzen 9 9950X，Release，1240×820 默认软件渲染窗口，OMP v18.1.10+fork.172。
独立会话启动 8 秒后输入 `/model`，随后回车，12 秒时退出菜单。没有发送模型提示词或选择/更改模型。

无 GUI 的真实 ConPTY 测试中，模型面板的完整内容约 155 ms 到达；后续重复打开约 118 / 102 ms。
1～2.5 秒后的附加输出是菜单已打开后的更新，不能计为首次打开时间。解析最高约 0.82 ms，快照约 0.55 ms。

实际 GUI 中，软件渲染器在正常合并后仍保留了大量重绘碎片。每片都重复遍历各图层并更新裁剪遮罩，造成主线程阻塞：

| 原版绘制场景 | 合并后的区域数 | 软件绘制耗时 |
| --- | ---: | ---: |
| 输入命令后的画面更新 | 591 | 1456.36 ms |
| 回车后的过渡更新 | 277 | 648.90 ms |
| 模型选择面板更新 | 476 | 1212.77 ms |

完整 compositor 耗时分别为 1458.15 / 650.30 / 1213.98 ms，说明主要时间花在软件绘制，尚未进入最终窗口提交。应用处理对应输出只需约 0.6 ms。

## 修复

基于原锁定版本 `iced_tiny_skia` 0.14.1 添加本地补丁：正常区域合并后，超过 8 个区域时再合并为覆盖全部变化的包围矩形。
至多 8 个小区域仍保持局部重绘。合并后的空隙也按完整当前场景重绘，不会保留旧像素。

未升级依赖版本、切换 GUI 框架或启用 GPU。Cargo.lock 只将同版本 iced_tiny_skia 改为本地来源；winit 旧 X11 补丁保留。新增源码和许可证见 `vendor/iced_tiny_skia/`。

修复后的首轮真实 GUI 测试，回车后约 149 ms 处理到完整面板输出，打开菜单期间没有超过 40 ms 的事件循环间隔。
最终带最大间隔计数的重复测试中，命令输入至菜单打开后的最大事件循环间隔为 **31.23 ms**，消除了原先约 0.65～1.46 秒的连续绘制阻塞。
这个指标是 16 ms 定时探针在 UI 上的实际处理间隔，不是逐帧呈现时间或输入 P99；没有将 149 ms 声称为精确的输入到屏幕像素延迟。

## 验证与边界

- 应用 Release 测试：57 项通过，0 失败。
- 渲染补丁测试：2 项通过，分别验证小范围更新保持原区域、600 个碎片合并后覆盖所有原始区域。
- 实际 OMP GUI 自动回放正常完成。最终重复验证的第 1 次完成；用户按 Esc 停止 Computer Use 后停止窗口检查并终止仍运行的第 2 次探针，不将其作为通过样本。
- 用户原有 GUI/OMP 会话没有被关闭；修复版是独立生成的可执行文件，旧进程不会自动加载新代码。
- 自动像素截图核验未完成。内部旧系统没有实机测试；补丁继续使用相同的软件渲染和窗口接口，不增加新系统 API 要求。

修复版：`.tmp/agentdock-model-fixed.exe`。

## 复测入口

在项目根目录设置临时目录和隔离会话目录后执行：

```powershell
$env:TEMP = "$PWD/.tmp"
$env:TMP = $env:TEMP
$env:CARGO_HOME = "$PWD/.tmp/cargo-home"
$env:CARGO_TARGET_DIR = "$PWD/.tmp/target"
$env:PI_CODING_AGENT_SESSION_DIR = "$PWD/.tmp/omp-gui-sessions"
cargo run --release --locked --offline --example omp_model_perf
cargo run --release --locked --offline --example omp_model_gui_perf
cargo test --release --locked --offline --lib
cargo test --release --locked --offline -p iced_tiny_skia --lib
```

第二个示例会启动独立 GUI，自动输入 `/model` 并关闭菜单，16 秒后退出。
原始日志保存在 `.tmp/omp-model-perf-events.log`、`.tmp/omp-gui-draw.log`、`.tmp/omp-gui-fixed.log`、`.tmp/omp-gui-final-1.log`、`.tmp/renderer-tests.log` 和 `.tmp/model-fix-tests.log`。
用于定位耗时的底层临时计时已经移除，正式补丁不记录终端内容或渲染日志。
