# 依赖与 API 依据

核对日期：2026-09-06。以下是公开上游源码，不代表本仓库已经编译通过。

- Iced 0.14.0 清单与 features：
  https://github.com/iced-rs/iced/blob/0.14.0/Cargo.toml
- Iced 应用启动 API：
  https://github.com/iced-rs/iced/blob/0.14.0/src/application.rs
- Iced 自定义 Widget 接口：
  https://github.com/iced-rs/iced/blob/0.14.0/core/src/widget.rs
- Iced 输入法：
  https://github.com/iced-rs/iced/blob/0.14.0/core/src/input_method.rs
- Iced Markdown：
  https://github.com/iced-rs/iced/blob/0.14.0/widget/src/markdown.rs
- Iced 软件渲染后端：
  https://github.com/iced-rs/iced/tree/0.14.0/tiny_skia
- WezTerm 固定版本：20240203-110809-5046fc22，Cargo rev 为 5046fc22。
  https://github.com/wezterm/wezterm/tree/20240203-110809-5046fc22/term
- WezTerm Screen / CellRef：
  https://github.com/wezterm/wezterm/blob/20240203-110809-5046fc22/term/src/screen.rs
  https://github.com/wezterm/wezterm/blob/20240203-110809-5046fc22/termwiz/src/surface/line/line.rs
- portable-pty 0.9.0：
  https://docs.rs/portable-pty/0.9.0/portable_pty/

没有导入用户自己的 WezTerm fork，也没有编造其修改。替换 fork 时需将
wezterm-term 与 termwiz 固定到同一个经过验证的 commit，并重跑终端测试。

本仓库原创代码采用 MIT。依赖保留各自的许可证；发布二进制时，需从实际
Cargo.lock 和构建依赖中生成第三方许可证清单。这里没有声称所有依赖都是 MIT。
