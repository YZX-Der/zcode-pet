# 04 开发计划

| | |
|---|---|
| 项目 | zcode-pet |
| 日期 | 2026-08-10 |

## 1. 里程碑

| 里程碑 | 内容 | 验收标准 | 状态 |
|---|---|---|---|
| M0 | 工程初始化与治理文档 | git init + docs/ + LICENSE + .gitignore 提交 | ✅ 进行中 |
| M1 | 环境准备 | rustup 安装完成；nvm Node 22；cargo/npm 可用 | ⬜ |
| M2 | 像素宠物生成器 | petgen.py 产出 5 只宠物（pet.json + 1536×1872 webp），尺寸校验通过，截图确认形象 | ⬜ |
| M3 | 状态协议与 hooks | hook 脚本自测通过；config.json 合并写入并校验 | ⬜ |
| M4 | Tauri 应用主体 | 假状态文件驱动四态切换 + 逐态截图核对；拖拽/点击/托盘/多窗口可用 | ⬜ |
| M5 | Codex 格式兼容 | 成功加载至少 1 只社区宠物 | ⬜ |
| M6 | 测试与打磨 | cargo test / vitest / petgen 校验全绿；真实 ZCode 会话 e2e 通过；README 完整 | ⬜ |
| M7 | CI 与 GitHub 上传 | CI workflow 绿；`gh repo create` 推送；tag v0.1.0 | ⬜ |

## 2. 分支与提交规范

- 分支：`main` 保持稳定；每个里程碑用 `feature/<scope>` 短生命周期分支（M0/M1 直接在 main 上引导）。
- 提交：[Conventional Commits](https://www.conventionalcommits.org/)：`feat / fix / docs / chore / test / refactor / ci`，中文描述，如 `feat: 状态目录监听与会话窗口回收`。

## 3. 测试策略

| 层 | 内容 | 工具 |
|---|---|---|
| 单元 | Rust 状态衰减/协议解析；前端状态机；petgen 输出尺寸格式 | cargo test / vitest / python3 校验脚本 |
| 集成 | 手写假状态文件驱动应用四态切换，screencapture 逐态截图核对 | 手动 + 脚本 |
| e2e | 真实 ZCode 会话跑任务，验证 hook 事件流与日志 | 人工验收 |

## 4. 发布计划

- v0.1.0：M0–M7 完成；GitHub Public 仓库 + MIT；附 macOS (arm64) 构建产物说明。
