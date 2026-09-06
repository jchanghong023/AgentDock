# 架构与边界

```
Iced event loop
  ├─ 左上虚拟列表 -> Store -> JSON 单写者线程
  ├─ OMP 历史扫描线程 -> 头部元数据缓存 -> Store -> 左上会话列表
  ├─ 左下虚拟列表 -> 操作系统根目录 / 分页目录请求 -> 文件线程
  └─ 单内容区
       ├─ TerminalView -> input actions -> wezterm-term
       │                                    ↑↓ bytes
       │                              bounded PTY queues
       │                                    ↑↓
       │                         Unix PTY / Windows ConPTY
       └─ Markdown / text preview <- 独立只读文件线程
```

Sessions 的所有权独立于 active content。切换预览只改变内容路由，不销毁 PTY。
启动进程在独立线程，完成后交回应用；被取消的启动结果丢弃时关闭 PTY。
输出按预算消费，并使用有界队列施加背压。组件只绘制可见终端字符，但当前渲染层
仍可能整区重绘；没有宣称已实现严格的系统级 dirty rectangle 最优方案。

目录按页消费 ReadDir，每个展开目录只缓存当前页，折叠后丢弃其子树。generation
用于忽略旧的返回。每个目录排序只在当前页内进行；不做全盘搜索或全局排序。
原生路径保存时保留 Unix 非 UTF-8 路径字节或 Windows 宽字符，显示时允许替代。

应用自己的状态文件是有限大小的 JSON，fs2 文件锁约束单实例写入。后台保存繁忙
时保留 dirty 并重试；原子替换失败不会主动清空旧状态。应用没有自动保存终端
屏幕或重建历史滚屏。OMP 历史由独立线程每 5 秒检查，缓存文件修改时间与长度，
仅解析标题和会话头部，不读取聊天正文。按当前 profile 的历史目录和头部 cwd
归入工作目录；旧 Shell 占位历史隐藏。单击未运行历史只选中，双击以
`omp --profile NAME --resume FILE` 恢复所选文件；文件缺失时提示，不回退到最近历史。
新建会话运行 OMP，显式 `--command` 可用于启动其他程序。

Terminal core 是成熟组件，但 TerminalView 是新实现。跨格 shaping、图像协议、
完整可访问性和输入法平台差异不会仅通过调用 core 自动解决。保留明确验收边界，
先在目标机建立测试结果再声称“体验等同 WezTerm”。
