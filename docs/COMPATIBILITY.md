# 平台与构建边界

## 当前已知状态

| 平台 | 源码目标 | 本次编译 | 本次运行 |
| --- | --- | --- | --- |
| Windows x64 | Win32 窗口、ConPTY、软件渲染 | 未执行 | 未执行 |
| 新 Linux x64 | X11、Unix PTY、软件渲染 | 未执行 | 未执行 |
| CentOS 7 x64 | glibc 2.17、X11、Unix PTY、无物理 GPU | 未执行 | 未执行 |

开发环境实际为 Debian 13 x86_64，没有 rustc/cargo。不能从源码静态检查得出
任何上述平台已通过编译的结论。CentOS 7 是设计目标，不是已经通过的兼容标签。

## CentOS 7

GUI 运行需要 X11 显示服务器。纯 SSH 文本登录且没有可用 DISPLAY 的服务器不能
凭空显示 GUI。可使用实际桌面、远程桌面或另行配置的 X11 显示；这些不是本仓库
自动部署的功能。没有物理 GPU 不等于不需要显示系统、字体和图形相关用户态库。

构建必须控制完整依赖链的 ABI，而不是只给 Rust 添加某个“支持 CentOS 7”开关。
应在 provision 好的 glibc 2.17 构建根中准备可运行的现代 Rust/C/C++ 构建工具、
X11/xcb、xkbcommon、Fontconfig 及传递依赖需要的开发库。应用代码不用 C++，
但这不保证传递依赖完全不调用 C/C++ 编译器。

`scripts/build-centos7.sh` 不会安装软件，只在检测到 glibc 2.17 时允许构建。
即使构建成功，还须执行：

```bash
python3 scripts/audit_abi.py target/x86_64-unknown-linux-gnu/release/devhub --glibc-max 2.17
```

审计工具需 Python 3.11+，可在现代分析机上对二进制执行，不是应用运行依赖。
用 readelf 而不是执行目标二进制；检查所有随包分发 `.so` 的 GLIBC、GLIBCXX 和
CXXABI 要求。GLIBC 达标仍不足以证明 X11、字体或内核兼容。

不要使用 `-C target-cpu=native` 构建可分发的老服务器版本。不要通过覆盖服务器
系统 glibc 解决问题。若在现代宿主内的 CentOS 7 容器测试，仍没有验证旧内核
3.10；最终必须在实际 CentOS 7 机器上验收终端输入与显示。

## Windows

目标为具有 ConPTY 的现代 Windows 10/11 x64；不声明支持 Windows 7。
建议使用 MSVC Rust 工具链和 Visual Studio C++ Build Tools。Rust 应用本身没有
C++ 源码，但原生构建依赖可能需要工具链。运行时字体使用系统字体。

没有嵌入 Windows Terminal 程序；窗口由 Iced 管理，PTY 由 portable-pty 创建，
终端字格由 WezTerm core 解析并交由应用绘制。用户需要的 PowerShell/OMP 程序
仍须在机器上存在。

## 依赖锁定

当前 Cargo.toml 固定 Iced 与 portable-pty 的直接版本，WezTerm 以固定 revision
引用。没有在缺少 Cargo 的环境中伪造 Cargo.lock。首个成功构建必须保存真实
锁文件，并复核传递依赖、许可证和最低工具链；之后使用 --locked 验证。

CI 的 Ubuntu 成功结果不能替代 CentOS 7。提供工作流不等于工作流已经运行。
