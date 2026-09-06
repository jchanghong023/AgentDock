# FDTH Core

一个面向工程师的终端优先工作目录。

## 项目简介

FDTH Core 是一个专注于电磁仿真（FDTD）的工程项目，提供从建模、求解到后处理的完整工作流，基于终端的高效交互体验。

- 面向工程师的简洁高效设计
- 支持大规模并行计算（OpenMP / MPI）
- 模块化结构，便于扩展和二次开发
- 完善的示例与文档

## 快速开始

1. **克隆项目**

   ```bash
   git clone https://github.com/example/fdth-core.git
   cd fdth-core
   ```

2. **安装依赖**

   ```bash
   pip install -r requirements.txt
   ```

3. **运行示例**

   ```bash
   python examples/run_fdtd.py
   ```

## 目录结构

```text
fdth-core/
├── src/          # 核心算法代码
├── docs/         # 项目文档
├── examples/     # 项目与测试用例
├── README.md     # 项目说明
└── notes.md      # 开发笔记
```

> 此文档是参考效果图的排版样例，以上工程、链接和命令仅供展示；AgentDock 不附带仿真程序，也不会执行预览中的命令。
