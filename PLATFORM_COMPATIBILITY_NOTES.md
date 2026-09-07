已按你现在确认的真实拓扑修正：**A、B 两机 `DISPLAY` 完全相同；A 上 GNOME Terminal 可正常运行；同一 DISPLAY 无 XKEYBOARD，且 `glxinfo -B` 无可用 GLX visual/fbconfig；B 上 WezTerm 当前首先死于 fallback XKB 数据路径错误。**

另外核对了几个容易写错的基础事实：CentOS Linux 7 于 2024-06-30 EOL；Ubuntu 24.04 的初始基线为 Linux 6.8、glibc 2.39、GCC 14、Python 3.12；libxkbcommon 从 0.6.0 起支持 `XKB_CONFIG_ROOT`；WezTerm 当前支持 `Software` frontend，并且其 X11 EGL 配置文档明确说明 X11 路径使用 `libEGL.so`。([红帽][1])

下面这版可以直接替换 `docs/PLATFORM_COMPATIBILITY.md`。

# 跨平台兼容性与验证基线

本仓库默认需要同时考虑以下平台：

1. Windows；
2. 最新 Linux；
3. 公司内部 CentOS 7；
4. 外部普通 CentOS 7。

核心原则：

> 当前环境能动态验证什么就验证什么；无法运行的平台只允许做静态检查，禁止把静态检查描述为动态验证。

---

## 1. 公司内部真实运行拓扑

当前已确认的公司内部 GUI 使用方式：

```text
用户终端
   │
Citrix Receiver
   │
CentOS 7 A 机
   │
   ├── 本地桌面 / GNOME Terminal
   │
   └── SSH
        │
        ▼
     CentOS 7 B 机
     程序、源码、数据均在 B 机
```

但 GUI 的 X11 路径不是普通的：

```text
A DISPLAY = 10.236.101.88:11.0
B DISPLAY = 10.236.101.88:11.0
```

因此实际应理解为：

```text
                         ┌── A 机 GUI 程序
                         │
10.236.101.88:11.0  ◄────┤
统一远端 X Server         │
                         └── B 机 GUI 程序
```

A、B 两机当前都直接使用同一个 `DISPLAY`。

这不是典型的：

```text
ssh -X
DISPLAY=localhost:10.0
```

因此不能简单把公司内部 GUI 问题归类为“SSH X11 forwarding 问题”。

### 当前确认结果

A 机：

```text
DISPLAY=10.236.101.88:11.0
GNOME Terminal 3.28.2
VTE 0.52.4
```

GNOME Terminal 可以正常显示。

B 机：

```text
DISPLAY=10.236.101.88:11.0
```

`xdpyinfo`、`xset` 等可以访问该 X Server。

因此目前已经确认：

```text
基础 X11 窗口能力：可用
B → DISPLAY 网络连接：可用
GUI 并非整体不可用
```

不能因为 WezTerm 启动失败就得出：

```text
公司机器不能运行 GUI
CentOS7 不能运行 GUI
DISPLAY 不可用
```

---

## 2. 当前环境自判

Agent 在进行设计、修改、dry-run、启动验证和打包验证前，必须先判断当前运行环境。

Linux：

```bash
echo "===== HOST ====="
hostname
uname -a
cat /etc/redhat-release 2>/dev/null || true
cat /etc/os-release 2>/dev/null || true

echo "===== LIBC ====="
getconf GNU_LIBC_VERSION 2>/dev/null || true
ldd --version 2>/dev/null | head -1 || true

echo "===== DISPLAY ====="
echo "DISPLAY=${DISPLAY:-}"
echo "WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-}"
echo "XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-}"
echo "SSH_AUTH_SOCK=${SSH_AUTH_SOCK:-}"

if command -v xdpyinfo >/dev/null 2>&1 && [ -n "${DISPLAY:-}" ]; then
    echo "===== X SERVER ====="
    xdpyinfo 2>&1 | egrep \
        'name of display|vendor string|vendor release|XKEYBOARD|GLX|RANDR|RENDER|XFIXES|XInput|MIT-SHM'
fi

if command -v setxkbmap >/dev/null 2>&1 && [ -n "${DISPLAY:-}" ]; then
    echo "===== XKB SERVER ====="
    setxkbmap -query 2>&1 || true
fi

if command -v glxinfo >/dev/null 2>&1 && [ -n "${DISPLAY:-}" ]; then
    echo "===== GLX ====="
    glxinfo -B 2>&1 || true
fi
```

Windows：

```powershell
$PSVersionTable
[System.Environment]::OSVersion.VersionString
$env:PATH
```

---

## 3. 动态验证与静态检查边界

| 当前环境            | 可以动态验证                        | 不能冒充已验证                                       |
| --------------- | ----------------------------- | --------------------------------------------- |
| Windows         | Windows 启动、路径、权限、打包           | Linux / CentOS7 启动                            |
| 最新 Linux        | Linux 启动、依赖、GUI 基础功能          | CentOS7 ABI、公司内部 X Server                     |
| 外部普通 CentOS7    | glibc 2.17、ELF、ldd、启动脚本、实际启动  | 公司内部特殊 X Server                               |
| WSL2 CentOS 用户态 | 脚本、部分 ABI/依赖                  | 真实 CentOS7 kernel、X Server、GUI、system service |
| 公司内部 CentOS7    | 内部机器、DISPLAY、真实 X Server、真实启动 | 外部普通 CentOS7 的完整代表性                           |

最终报告必须明确区分：

```text
已动态验证：
已静态检查：
未在该平台验证：
已知风险：
```

禁止写：

```text
已支持所有 Linux
已验证 CentOS7
跨平台完成
```

除非对应平台确实完成了动态验证。

---

## 4. 公司内部 CentOS7 基线

当前已确认的 B 机基础环境：

| 项目          | 当前值                           |
| ----------- | ----------------------------- |
| OS          | CentOS Linux 7.9.2009 Core    |
| Kernel      | `3.10.0-1160.95.1.el7.x86_64` |
| glibc       | `2.17`                        |
| GCC 基线      | `4.8.x` 级别                    |
| Arch        | `x86_64`                      |
| Wayland     | 无                             |
| XDG runtime | `/run/user/<uid>`             |
| 字体系统        | 可用                            |

X11 / XKB 软件：

| 项目               | 当前值                  |
| ---------------- | -------------------- |
| libX11           | `1.6.7`              |
| libxcb           | `1.13`               |
| libxkbcommon     | `0.7.1`              |
| libxkbcommon-x11 | `0.7.1`              |
| xkeyboard-config | `2.24`               |
| 系统 XKB 数据        | `/usr/share/X11/xkb` |
| `rules/evdev`    | 存在                   |

当前 DISPLAY：

```text
A: 10.236.101.88:11.0
B: 10.236.101.88:11.0
```

当前远端 X Server 能力：

| 能力                         | 当前结果 |
| -------------------------- | ---- |
| 基础 X11                     | ✅    |
| MIT-SHM                    | ✅    |
| RANDR                      | ✅    |
| RENDER                     | ✅    |
| XFIXES                     | ✅    |
| XInputExtension            | ✅    |
| XKEYBOARD                  | ❌    |
| usable GLX visual/fbconfig | ❌    |
| Wayland                    | ❌    |

其中：

```bash
setxkbmap -query
```

结果：

```text
XKB extension not present on 10.236.101.88:11.0
```

A 机：

```bash
glxinfo -B
```

结果：

```text
name of display: 10.236.101.88:11.0
Error: couldn't find RGB GLX visual or fbconfig
```

同时 A 机 GNOME Terminal 可以正常运行。

因此当前公司环境应该描述为：

```text
基础 X11 GUI 可用；
XKEYBOARD 不可用；
GLX 图形能力不可用或不完整；
不能假设现代 Xorg 常见扩展全部存在。
```

不能简化为：

```text
没有 OpenGL，所以不能运行 GUI
```

GNOME Terminal 已经证明普通 GUI 可以在该环境工作。

---

## 5. 当前 WezTerm 已确认问题

当前 WezTerm 启动日志：

```text
XKEYBOARD extension is not available;
using basic keyboard fallback
```

随后：

```text
xkbcommon:
Failed to add any default include path

system path:
/opt/micromamba/envs/dev/share/xkeyboard-config-2

Couldn't find file "rules/evdev"

Cannot load XKB rules "evdev"

Failed to load system default keymap
```

最终：

```text
initializing fallback keyboard without XKB:
Failed to load system default keymap;
terminating
```

### 当前故障链

已经确认：

```text
远端 X Server
    │
    └── 没有 XKEYBOARD
            │
            ▼
WezTerm 进入 basic keyboard fallback
            │
            ▼
fallback 使用 libxkbcommon 加载本地 keymap
            │
            ▼
libxkbcommon 默认数据路径错误
/opt/micromamba/envs/dev/share/xkeyboard-config-2
            │
            ├── 该目录不存在
            │
            ▼
找不到 rules/evdev
            │
            ▼
WezTerm 终止
```

B 机实际已经有正确数据：

```text
/usr/share/X11/xkb
/usr/share/X11/xkb/rules/evdev
```

因此当前第一个致命问题不是：

```text
CentOS7 没有 xkeyboard-config
```

而是：

```text
当前 WezTerm/libxkbcommon 构建产物的默认 XKB 数据路径
与目标 CentOS7 实际路径不一致。
```

---

## 6. XKB fallback 验证规则

B 机当前的：

```text
libxkbcommon 0.7.1
```

已经具备通过 `XKB_CONFIG_ROOT` 指定 XKB 根目录的能力。

测试命令必须精确写成：

```bash
env XKB_CONFIG_ROOT=/usr/share/X11/xkb ./wezterm start
```

或者：

```bash
export XKB_CONFIG_ROOT=/usr/share/X11/xkb
./wezterm start
```

变量名必须是：

```text
XKB_CONFIG_ROOT
```

路径必须是：

```text
/usr/share/X11/xkb
```

以下均不是有效验证命令：

```text
XKB CONFIG ROOT=...
XKB_CONFIG_R00T=...
XKB_CONFIG_ROOT=/usr/share/x
```

当前出现过类似：

```text
XKB CONFIG R00T=/usr/share/x
```

的输入，因此对应的：

```text
not found
```

不能用于判断 `XKB_CONFIG_ROOT` 方案是否有效。

### 验证成功标准

执行正确命令之后，如果以下错误消失：

```text
Failed to add any default include path
Couldn't find file "rules/evdev"
Failed to load system default keymap
```

则证明：

```text
XKEYBOARD 缺失
        ↓
fallback
        ↓
使用本地 /usr/share/X11/xkb
        ↓
fallback keymap 初始化成功
```

此时即使出现新的 renderer/EGL 等错误，也应该视为：

```text
XKB 问题已解决；
进入下一层 GUI 初始化问题。
```

不得把不同阶段的问题混在一起。

---

## 7. 图形渲染验证规则

当前已经确认：

```text
glxinfo -B
→ couldn't find RGB GLX visual or fbconfig
```

但：

```text
GNOME Terminal
→ 可以正常显示
```

因此：

```text
GLX 失败 ≠ 基础 X11 GUI 失败
```

对于 WezTerm，还必须单独验证其实际使用的 renderer。

优先测试：

```bash
env XKB_CONFIG_ROOT=/usr/share/X11/xkb \
    ./wezterm -n \
    --config 'front_end="Software"' \
    start --always-new-process
```

如果仓库当前 WezTerm 版本、CLI 或配置语法不同，应以该版本源码和配置文档为准。

还应检查：

```bash
command -v eglinfo && eglinfo 2>&1 | head -150

ldconfig -p | egrep \
'libEGL|libGLES|libGLX|libGL\.so|libgbm'

rpm -qa | egrep -i \
'mesa|libglvnd' | sort
```

验收时必须区分：

```text
X11 windowing
XKB keyboard
EGL
GLX
software renderer
GPU renderer
```

任何一个失败都不能直接推导出其他层失败。

---

## 8. 当前 WezTerm 其他已知风险

### 8.1 SSH agent

曾观察到：

```text
failed to create symlink
/run/user/.../wezterm/agent...
```

指向一个已经不存在的：

```text
/tmp/ssh-.../agent....
```

该错误当前不是 GUI 退出的直接原因。

处理优先级：

```text
XKB fatal
    >
renderer / EGL
    >
SSH agent
```

不得因为 SSH agent warning 分散对 GUI fatal error 的排查。

### 8.2 启动器不是 ELF

当前：

```text
./wezterm
```

是约 165 字节的 shell launcher，不是真正 ELF。

因此：

```bash
ldd ./wezterm
```

得到：

```text
not a dynamic executable
```

不能证明真实 WezTerm 二进制没有依赖问题。

必须定位真实 ELF：

```bash
file ./wezterm
sed -n '1,100p' ./wezterm

find . -maxdepth 3 -type f \
    \( -name 'wezterm' \
    -o -name 'wezterm-gui' \
    -o -name 'wezterm-mux-server' \) \
    -exec file {} \;
```

必要时：

```bash
bash -x ./wezterm --version
```

确认 launcher 最终执行哪个文件。

---

## 9. 公司内部 CentOS7 与普通 CentOS7

| 项目        | 公司内部 CentOS7                         | 外部普通 CentOS7               |
| --------- | ------------------------------------ | -------------------------- |
| OS ABI    | CentOS7 / glibc 2.17                 | CentOS7 / glibc 2.17       |
| root      | 不应假设有                                | 环境决定                       |
| 现场安装依赖    | 不应作为前提                               | 可作为辅助方案                    |
| GUI       | 特殊远端 X Server                        | 常见 Xorg/VNC/X forwarding   |
| DISPLAY   | 当前为远端 IP display                     | 常见 `:0` / `localhost:10.0` |
| XKEYBOARD | 当前明确没有                               | 不能假设，但通常条件不同               |
| GLX       | 当前无 usable visual/fbconfig           | 环境决定                       |
| 验证重点      | X11 extensions、fallback、无 root、旧 ABI | glibc、ELF、启动脚本、标准 GUI 栈    |

必须保留以下结论：

```text
外部 CentOS7 成功
不等于
公司内部 CentOS7 成功。
```

同样：

```text
公司内部 CentOS7 的特殊 X Server 导致失败
不等于
普通 CentOS7 不兼容。
```

---

## 10. CentOS7 与现代 Linux

CentOS Linux 7 已于 2024-06-30 EOL。

CentOS7 典型 ABI 基线：

```text
glibc 2.17
3.10 系列 kernel
旧版 GCC / libstdc++
旧版 X11 用户态
```

Ubuntu 24.04 初始发布基线包括：

```text
Linux 6.8
glibc 2.39
GCC 14
Python 3.12
```

因此：

```text
Ubuntu 24.04 编译成功
≠
CentOS7 可以运行
```

面向 CentOS7 发布 Linux native binary 时，至少需要检查：

```text
glibc symbol version
GLIBCXX symbol version
动态库依赖
ELF interpreter
RPATH / RUNPATH
kernel / syscall 要求
native dependency ABI
```

glibc 2.17 是必要基线之一，但不是完整的兼容性证明。

---

## 11. Rust GUI 技术选择

### 一般轻量 GUI：egui / eframe

适合：

```text
内部工具
配置界面
启动器
日志查看器
轻量桌面应用
AgentDock 一类工具
```

### 复杂状态型 GUI：iced

适合：

```text
复杂多页面
大量异步任务
任务队列
长期状态管理
Message / Update / View 架构
```

但技术框架的公开平台支持范围不能替代公司内部验证。

统一原则：

```text
Rust GUI 支持 Linux
≠
支持 CentOS7
≠
支持公司内部 CentOS7 + 当前 X Server
```

只要依赖：

```text
winit
X11
xkbcommon
EGL
GLX
Vulkan
Wayland
```

就必须检查当前实际依赖版本和 fallback 行为。

尤其不能把：

```text
XKEYBOARD
GLX
Wayland
GPU acceleration
```

作为公司内部环境的无条件前提。

---

## 12. Rust 终端技术选择

完整终端应用可参考 WezTerm 的架构。

终端核心建议拆分：

```text
terminal parser / screen / scrollback
        │
        └── wezterm-term 等终端状态机

PTY
        │
        └── portable-pty 等跨平台 PTY

GUI
        │
        └── 独立 window/input/render layer
```

必须避免：

```text
终端状态机正常
=
GUI 一定正常
```

GUI 是独立的兼容性层。

公司环境尤其需要满足：

```text
XKEYBOARD optional
服务器 keymap 获取失败可 fallback
fallback 本地 XKB 路径可定位
renderer 可降级
可选 extension 缺失不 panic
```

---

## 13. Linux 发布包动态库检查

不能只检查顶层 launcher。

必须找到真正 ELF：

```bash
find bin lib -type f -print0 2>/dev/null |
while IFS= read -r -d '' f; do
    file "$f" | grep -q 'ELF' || continue

    echo "===== $f ====="

    LD_LIBRARY_PATH="$PWD/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
        ldd "$f" 2>&1

    readelf -d "$f" 2>/dev/null |
        egrep 'NEEDED|RPATH|RUNPATH' || true
done
```

要求：

```text
不能有 not found
不能依赖用户手工拼复杂 LD_LIBRARY_PATH
不能假设有 root
不能假设可以 yum install
启动器必须支持安装路径包含空格
```

推荐 launcher：

```bash
#!/usr/bin/env bash
set -e

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export LD_LIBRARY_PATH="$HERE/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

exec "$HERE/bin/app" "$@"
```

如果有额外运行时数据，例如：

```text
XKB
fonts
terminfo
resources
```

也必须显式确认其查找路径，不能只检查 `.so`。

---

## 14. glibc / GLIBCXX 检查

对所有真实 ELF：

```bash
find bin lib -type f -print0 2>/dev/null |
while IFS= read -r -d '' f; do
    file "$f" | grep -q 'ELF' || continue

    echo "===== $f ====="

    objdump -T "$f" 2>/dev/null |
        grep -o 'GLIBC_[0-9][0-9.]*' |
        sort -Vu |
        tail -1

    objdump -T "$f" 2>/dev/null |
        grep -o 'GLIBCXX_[0-9][0-9.]*' |
        sort -Vu |
        tail -1
done
```

CentOS7 目标产物：

```text
不能依赖高于目标机器实际 libc/libstdc++ 提供的符号。
```

目标机器本身也应检查：

```bash
strings /usr/lib64/libstdc++.so.6 2>/dev/null |
    grep '^GLIBCXX_' |
    sort -Vu |
    tail -1
```

不要仅根据 GCC 版本猜测目标机可用的最高 `GLIBCXX_*`。

---

## 15. GUI / X11 检查规则

以下能力均不得未经验证直接作为所有 Linux/X11 环境的强制前提：

```text
XKEYBOARD
RANDR
RENDER
XFIXES
XInputExtension
MIT-SHM
DAMAGE
Present
DRI2
DRI3
GLX
EGL
Wayland
```

规则：

```text
可选扩展缺失 → 应降级

必要扩展缺失 → 应给出明确错误

禁止 panic

禁止静默退出

禁止把现代 Xorg 的常见能力
当作所有 X Server 的保证
```

公司环境最低测试：

```bash
xdpyinfo -queryExtensions 2>&1 |
    egrep \
    'XKEYBOARD|GLX|RANDR|RENDER|XFIXES|XInput|MIT-SHM'

setxkbmap -query 2>&1

glxinfo -B 2>&1
```

如果：

```text
NO XKEYBOARD
```

应用应优先做到：

```text
可以使用 fallback 启动；
或者给出准确、可定位错误。
```

不能：

```text
XCB/X11 初始化 panic
无日志退出
错误地要求用户安装本地 Xorg
```

---

## 16. 公司内部专项验收

公司内部 CentOS7 GUI 验收必须在真实环境完成：

```text
A 机
    DISPLAY=10.236.101.88:11.0

B 机
    DISPLAY=10.236.101.88:11.0
```

程序实际运行位置：

```text
B 机
```

必须验证：

```text
B 机直接启动程序
        │
        ▼
连接当前 DISPLAY
        │
        ▼
没有 XKEYBOARD
        │
        ▼
keyboard fallback 正常
        │
        ▼
renderer 正常或降级
        │
        ▼
窗口实际显示
        │
        ▼
键盘输入
        │
        ▼
IME
        │
        ▼
PTY / shell
```

不能只在 A 机启动 GUI 后声称：

```text
B 机已验证
```

---

## 17. Agent 修改前检查清单

只要改动涉及：

```text
构建脚本
发布打包
动态库
启动脚本
GUI
X11
Wayland
键盘
XKB
IME
剪贴板
OpenGL
EGL
GLX
PTY
终端
Rust / C / C++ native binary
路径处理
```

Agent 必须检查本文件对应条目。

提交总结必须写明：

```text
当前运行环境：

已动态验证：
- Windows：
- 最新 Linux：
- 外部 CentOS7：
- 公司内部 CentOS7：

已静态检查：

未验证：

已知风险：

新增动态库：
是否需要 root：
是否需要 yum/apt：
是否需要 systemd：
是否依赖 Wayland：
是否强制依赖 XKEYBOARD：
是否强制依赖 GLX/EGL/GPU：
```

---

## 18. 最低验收标准

若声明：

```text
支持四类平台
```

至少需要：

```text
Windows
    动态启动验证通过

最新 Linux
    动态启动验证通过

外部普通 CentOS7
    动态启动验证通过

公司内部 CentOS7 B 机
    在真实 DISPLAY 上动态启动验证通过

CentOS7
    ELF / ldd / glibc / GLIBCXX 检查通过

公司内部
    无 XKEYBOARD 场景通过

公司内部
    当前图形能力下 renderer 路径通过

无 root
    可以运行

路径包含空格
    可以运行

错误路径
    日志可以定位
```

如果只完成部分验证，必须明确写：

```text
已在当前环境动态验证：
...

仅完成静态检查：
...

未动态验证：
...
```

禁止为了让总结看起来完整而把未执行的测试写成已通过。

---

## 19. 当前公司环境已确认结论

截至当前实际测试：

```text
[确认] A、B 均为 CentOS7 环境

[确认] 程序、源码、数据位于 B 机

[确认] A → B 使用 SSH

[确认] A DISPLAY = 10.236.101.88:11.0

[确认] B DISPLAY = 10.236.101.88:11.0

[确认] A、B 当前指向同一个 X Server endpoint

[确认] 基础 X11 可用

[确认] A 机 GNOME Terminal 3.28.2 可正常显示

[确认] RENDER / RANDR / XFIXES /
       XInputExtension / MIT-SHM 可用

[确认] XKEYBOARD 不可用

[确认] setxkbmap 无法从 X Server 查询 XKB

[确认] GLX 无 usable RGB visual/fbconfig

[确认] B 机存在完整系统 XKB 数据
       /usr/share/X11/xkb

[确认] B 机存在 rules/evdev

[确认] 当前 WezTerm fallback 的默认 XKB 路径错误
       /opt/micromamba/envs/dev/share/xkeyboard-config-2

[确认] 该错误目前导致 WezTerm GUI 初始化终止

[待验证] 使用正确 XKB_CONFIG_ROOT 后
         fallback 是否完全成功

[待验证] XKB 修复后
         WezTerm Software/EGL renderer 是否可用

[待验证] 实际 GUI 成功后
         键盘快捷键、IME、剪贴板、分屏等行为
```

当前排障顺序固定为：

```text
1. 修正 / 覆盖 fallback XKB 数据路径

2. 确认 keyboard fallback 成功

3. 验证 Software / EGL renderer

4. 确认窗口实际显示

5. 验证键盘、IME、剪贴板

6. 最后处理 SSH agent 等非致命问题
```

在第 1 项完成前，不应继续把主要精力投入到其他 warning。

这版最重要的修正是：**不再把 A 当成 B 的 X Server，也不再把 `glxinfo` 失败等价成“整个 GUI 不可用”**。当前真实兼容目标应该定义为“B 机程序在 `10.236.101.88:11.0` 这个特殊 X Server 上正常运行”。

[1]: https://www.redhat.com/en/blog/centos-linux-has-reached-its-end-life-eol?utm_source=chatgpt.com "CentOS Linux has reached its End of Life (EOL)"
