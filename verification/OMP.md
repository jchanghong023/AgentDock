# OMP Agent 会话接入验证

日期：2026-09-07。本机 OMP：v18.1.10+fork.172。

用户最终选择“恢复点选的具体历史”。因此历史双击使用
`omp --profile NAME --resume /完整路径/所选会话.jsonl`，不是恢复最近一条的 `-c`。
单击只选中未运行记录；已运行的会话直接切换。新建会话启动 OMP。

DevHub 支持 `--profile NAME` 和 `--profile=NAME`。历史列表读取对应 profile 的
会话目录，只解析头部工作目录、会话 ID 和可选标题，不读取聊天正文或认证配置。
后台独立扫描并缓存文件元数据，默认每 5 秒检查变化。旧 Shell 占位记录不再作为
OMP 历史显示。系统目录树仍只有操作系统根目录。

实际验证：

- Release 构建通过，44 项测试全部通过，见 omp-tests.log。
- 默认 profile 的 GUI 冒烟退出码 0，加载 28 个工作目录、64 条真实历史；
  验证输出只记录数量，见 omp-import-counts.json。
- 已在真实 ConPTY 中启动本机 OMP，子进程参数为 `--profile default`。
- 独立测试包含较旧、较新的两条人工测试记录。单击较旧记录时 OMP 子进程数为 0；
  双击后真实 OMP 以 `--profile default --resume .../older-selected.jsonl` 启动，
  窗口标题与 OMP 会话标题均显示 older-selected，见 omp-resume-process.json。
- 初次人工夹具缺少 OMP 识别的标题头格式，被 OMP 拒绝，文件未修改；修正夹具为
  type 在前且含 pad 的标题头后重测通过。DevHub 未通过回退到最近会话掩盖该错误。
- 未向任何 Agent 发送任务消息。测试实例使用独立 DevHub 状态和会话目录。
- CentOS 7 未实机运行；命名 profile 的隔离路径和参数构造有代码检查/测试，
  本次实际 OMP 启动使用 default profile。

参数依据为本机 `omp --help`；profile 路径与环境变量规则参考
[OMP 官方路径实现](https://github.com/can1357/oh-my-pi/blob/main/packages/utils/src/dirs.ts)。
