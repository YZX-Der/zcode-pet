# 02 架构设计

| | |
|---|---|
| 项目 | zcode-pet |
| 日期 | 2026-08-10 |

## 1. 总体架构

```
┌─────────────────────────────────────────────────────────┐
│ ZCode 客户端（任意工作区会话）                            │
│  hooks（~/.zcode/cli/config.json, hooks.enabled=true）  │
│  7 事件 → type:"process" 调用 hook 脚本                  │
└──────────────┬──────────────────────────────────────────┘
               │ <10ms，环境变量注入会话ID/项目目录
               ▼
┌──────────────────────────────────────┐
│ ~/.zcode-pet/bin/zcode-hook          │  POSIX sh，原子写入
└──────────────┬───────────────────────┘
               ▼
┌──────────────────────────────────────┐
│ ~/.zcode-pet/state/<session_id>.json │  状态协议（单一事实来源）
└──────────────┬───────────────────────┘
               ▲ notify crate 监听目录
┌──────────────┴───────────────────────────────────────────┐
│ zcode-pet.app（Tauri 2）                                  │
│ ┌─ Rust 后端 ──────────────────────────────────────────┐ │
│ │ window  会话窗口创建/回收/排列、位置持久化             │ │
│ │ watcher 状态目录监听、状态解析、衰减与失联判定         │ │
│ │ tray    托盘菜单（换宠物/唤醒收起/退出）               │ │
│ │ pet     宠物清单加载、Codex 格式适配层                 │ │
│ │ activate 点击跳回（osascript，目标列表可配）           │ │
│ └──────────────────────────────────────────────────────┘ │
│ ┌─ TS 前端（Vite + TypeScript，每会话一个 Webview）─────┐ │
│ │ Canvas 2D 精灵表切片播放；状态机；拖拽/点击阈值区分    │ │
│ └──────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────┘

┌─ tools/petgen.py（开发期工具，Pillow）────────────────────┐
│ 字符网格造型 + 调色板 + 程序化帧 → pet.json +             │
│ spritesheet.webp（1536×1872，8列×9行，Codex 兼容尺寸）   │
└───────────────────────────────────────────────────────────┘
```

## 2. 技术选型

| 决策点 | 选择 | 理由 | 放弃的备选 |
|---|---|---|---|
| 桌面框架 | Tauri 2 | 体积小（几 MB）、Rust 性能好、透明悬浮窗支持成熟；与作者 agy-byok 项目同栈 | Electron（太重）、Swift 原生（开发量最大）、Python/PyQt（分发不便） |
| 前端 | Vite + TypeScript | 动画调参依赖 HMR；状态协议跨四层需要类型约束；Node 22 经 nvm 就绪 | 纯静态 HTML+JS（零构建但无类型与 HMR） |
| 动画渲染 | Canvas 2D（Webview） | WKWebView 原生支持 WebP 切片，开发快，性能够 | Rust 原生渲染（复杂度高） |
| 进程间通信 | 状态文件 + 目录监听 | hook 脚本零依赖、跨进程解耦、崩溃可恢复 | Unix socket（hook 脚本需额外客户端、生命周期管理复杂） |
| hook 类型 | type:"process" | 无 shell 开销与转义问题，最可移植 | type:"command"（走 shell，慢且有引号坑） |

## 3. 关键设计决策

- **D1 状态文件是唯一事实来源**：宠物应用不持有任何"真实状态"，只渲染 `~/.zcode-pet/state/` 的内容；应用重启后从目录恢复全部会话状态。
- **D2 hook 脚本必须极简**：ZCode hooks 内联执行（async 无效），脚本只做"读环境变量 → 拼 JSON → tmp+mv 原子写"，目标 < 10ms，配 timeoutMs 2000 兜底。
- **D3 每会话一窗口**：窗口 label 为 `pet-<session_id>`；上限 5 只，超出按优先级聚合（needs_input > blocked > ready > running > idle）。
- **D4 衰减在应用侧**：hook 只写原始事件；ready 超时回 idle、长时间无事件转 sleep、会话失联回收等逻辑全部在 Rust watcher 中实现（详见状态协议文档）。
- **D5 兼容层独立**：Codex 社区 pet.json 的解析适配为独立模块，不污染自有清单格式；规范细节以 awesome-codex-pet 仓库实际文档为准，不臆测。

## 4. 目录结构（规划）

```
zcode-pet/
├── docs/                  # 治理文档
├── tools/
│   └── petgen.py          # 像素宠物生成器（Pillow）
├── assets/
│   └── pets/<name>/       # 内置宠物（pet.json + spritesheet.webp）
├── src/                   # TS 前端（Vite）
│   ├── main.ts
│   ├── animator.ts        # Canvas 精灵表播放
│   ├── state-machine.ts   # 状态机与衰减
│   └── protocol.ts        # 状态协议 / pet.json 类型定义
├── src-tauri/             # Rust 后端
│   └── src/
│       ├── main.rs
│       ├── watcher.rs
│       ├── window.rs
│       ├── tray.rs
│       ├── pet.rs
│       └── activate.rs
├── scripts/
│   └── zcode-hook         # 部署到 ~/.zcode-pet/bin/ 的 hook 脚本
└── .github/workflows/     # CI
```
