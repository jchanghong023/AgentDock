# Windows 构建与 GUI 验证

日期：2026-09-07。环境：Windows，Rust 1.98.0，Cargo 1.98.0，x86_64 MSVC。

已生成并实际运行 `target/debug/devhub.exe`（开发构建），生成了真实 `Cargo.lock`。
当前源码目录不是 Git 仓库；修改前的自有源码备份在 `verification/original/src/`。

## 修复与界面

- 修正 Iced 0.14 输入法接口的导入路径，消除阻止编译的错误。
- 修正两个测试对 WezTerm 异步写入的竞态，保留响应字节的精确断言。
- 蓝白主题、Windows 中文字体、项目和文件图标、星标、浅色标签及新会话按钮。
- 深色真实 ConPTY 终端、原生终端标记、应用窗口图标。
- Markdown 的标题、分隔线、浅色代码块及逐块复制；保留源码/只读切换。
- 修正侧栏长路径绘制越界。系统目录树增加当前项目入口，仍保留 Windows 盘符。
- 无参数启动会创建一个新终端，不会自动恢复或执行历史会话的 resume 命令。
- 增加 `examples/fdth-core/README.md` 作为参考图排版样例；不是已实现的仿真项目。

## 实际结果

| 项目 | 结果 |
| --- | --- |
| `cargo build --locked` | 通过，无编译告警 |
| `cargo test --locked` | 37 项通过，0 失败；见 windows-tests.log |
| 真实 Windows PTY 输入输出/尺寸调整 | 通过，包含于上述测试 |
| `--smoke-ui-ms 1500` | 退出码 0，输出 DEVHUB_GUI_SMOKE_EVENT_LOOP_OK |
| GUI 目视检查 | 已打开终端和 Markdown 两种视图，中文可见 |
| 目录操作 | 实际点击展开 examples/fdth-core，双击 README.md 打开预览 |
| 新会话与切换 | 点击标签栏 + 创建第二个终端，返回预览后两个终端标签保留 |
| 退出 | 两轮中间实例均通过退出确认正常关闭 |
| stderr | 最终交互实例日志为空 |
| Clippy | 普通模式通过，27 项告警；`-D warnings` 未通过，见 windows-clippy.log |

Clippy 告警主要为原有紧凑写法、嵌套 if、冗余类型转换和参数个数。
本次没有为了清理风格而重构整个项目。

截图：`windows-terminal.png`、`windows-preview.png`。
最终交互实例使用独立状态目录 `verification/windows-final`，不污染用户默认历史。

## 验证边界

Windows 已编译和运行。CentOS 7 尚未编译或实机验收，保留原有 Linux/X11
和 tiny-skia 架构，不将 Windows 结果等同于 Linux ABI 兼容性验证。
未对中文 IME 组合输入、OMP/vim、远程桌面和全部终端协议做完整验收。
参考图的工程列表、Linux 提示符和仿真输出属于样例内容，运行窗口显示真实的
Windows 目录和 Shell；主要布局与视觉样式已对应，未宣称逐像素一致。
