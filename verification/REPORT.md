# 本次实际验证记录

日期：2026-09-06。

## 结果

**已交付全部自有仓库源码；编译与运行验收受阻。没有生成或宣称存在可运行的
Windows / CentOS 7 二进制。**

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| 仓库结构、TOML/JSON/YAML 解析、Python/Shell 语法、Rust 词法括号 | 53 项通过，0 失败 | source-checks.json / source-checks.log |
| Python 验证与打包工具单元测试 | 11 / 11 通过 | python-tests.log |
| Python compileall | 退出码 0 | 本次直接执行；只证明辅助脚本可编译为 Python 字节码 |
| Rust 验证脚本 | 退出码 2，缺少 cargo | rust-validation.log |
| Rust 工具链下载尝试 | 退出码 6，域名解析失败 | toolchain-download-attempt.log |
| Rust 单元测试 | 未执行；源码包含 38 个测试声明 | src/ 各模块 #[test] |
| rustfmt / cargo check / clippy | 未执行 | 缺少 Rust 工具链 |
| 真实 PTY 自测 / GUI 测试 | 未执行 | 无法构建二进制 |
| Windows / CentOS 7 | 未验证 | 当前容器不是这两个目标系统 |

完整环境信息在 environment.json。机器为 Debian 13 x86_64，Python 3.13.5，
有 GCC、CMake、Xvfb、readelf，但没有 rustc、cargo、rustfmt 或 PowerShell。
因此虽然存在 Xvfb，也不能在没有可执行文件时声称 GUI 冒烟通过。

## 检查的含义

53 项检查包括文件存在性和配置字段检查，**不是 53 个应用功能测试**。
Rust 词法括号检查使用 Pygments 去除注释/字符串影响后匹配括号；不解析类型、
不检查借用、不验证 crate API、不链接，不能替代 Rust 编译器。

11 项 Python 测试覆盖实际编写的 ABI 版本比较、ZIP 清单/路径/校验和检查和
源码检查工具；它们不是用 Python 重写应用逻辑来冒充 Rust 测试。

## 代码核对中已处理的问题

- Iced 0.14.0 的启动、Widget update、Markdown、clipboard、IME 接口按该 tag 核对。
- 修正 fill_text 的顶部坐标与垂直对齐匹配，避免以顶部点误用中心对齐。
- 区分目录树焦点和终端输入焦点；本次没有实机验证其所有事件顺序。
- 会话进程所有权独立于文件预览，关闭进程需要明确确认。
- 目录分页和结果 generation 防止一次加载整盘以及旧结果重新展开已折叠节点。

以上为源代码层面的核对和修正，不是运行后的“已修复”结论。

## 尚未完成

真实 Cargo.lock、依赖解析、编译/链接、Rust 单元测试、输入法/剪贴板/终端协议
交互、实际 GUI 画面、启动速度、CPU 吞吐、Windows ConPTY、CentOS 7 的 glibc
和图形库兼容性，均需在具备工具链及对应系统的环境中完成。

当前终端控件是新实现，不应按成熟 WezTerm GUI 的完整替代品直接投入使用。
自定义 WezTerm fork 没有被导入；使用的是固定版本的公开上游 core。

仓库提供 scripts/verify.sh、scripts/verify.ps1、scripts/gui-smoke.sh、
scripts/audit_abi.py 和 CI 工作流；这些脚本的存在不代表对应平台检查已经执行。

最终 ZIP 由 scripts/package_source.py 创建并在本机执行 CRC、路径安全、完整清单
和每个文件的 SHA-256 校验。校验清单位于压缩包内 MANIFEST.sha256。
