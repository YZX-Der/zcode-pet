# zcode-pet

> ZCode 的桌面宠物 —— 一只住在你屏幕上、实时反映 AI 编程任务状态的像素小伙伴。

对标 OpenAI Codex 桌面端的 Pets 功能，为 ZCode 打造的**独立** macOS 桌宠应用：透明悬浮窗像素动画 + 通过 ZCode hooks 实现的任务状态联动。

## 特性

- 🐣 **像素风桌宠**：透明悬浮窗，始终置顶，可拖拽
- 🔄 **状态联动**：通过 ZCode hooks 实时反映任务状态 —— Running / Needs input / Ready / Blocked / Sleep
- 👆 **点击跳回**：点击宠物激活 ZCode / 终端窗口
- 🐾 **多会话多宠物**：每个 ZCode 会话对应一只宠物，并行任务一目了然
- 🎨 **兼容 Codex 宠物格式**：可直接加载 [awesome-codex-pet](https://github.com/legeling/awesome-codex-pet) / [Petdex](https://github.com/crafter-station/petdex) 社区宠物精灵表
- 🛠️ **代码手绘内置宠物**：自带 5 只程序化生成的像素宠物

## 截图

> 📸 截图占位（首次 release 附带）

## 快速开始

### 环境要求

- macOS 13+（Apple Silicon / Intel）
- [Node.js 22+](https://nodejs.org/)（推荐用 [nvm](https://github.com/nvm-sh/nvm) 管理）
- [Rust](https://rustup.rs/)（stable 工具链）
- ZCode CLI（已安装并配置好 `~/.zcode/cli/config.json`）

### 方式一：从源码构建（当前推荐）

```bash
git clone https://github.com/YZX-Der/zcode-pet.git
cd zcode-pet

# 安装依赖
npm install

# 构建 release 版本（自动编译前端 + Rust）
npx tauri build
```

构建产物在 `src-tauri/target/release/bundle/macos/zcode-pet.app`。

### 方式二：安装 hooks 联动

```bash
# 部署 hook 脚本并自动合并写入 ZCode 配置
bash scripts/install.sh
```

安装脚本会：
1. 将 `zcode-hook` 部署到 `~/.zcode-pet/bin/`
2. 备份后向 `~/.zcode/cli/config.json` 合并写入 7 个 hooks 事件
3. 自动备份原配置

### 生成内置宠物

```bash
python3 tools/petgen.py build --out ~/.zcode-pet/pets
```

这会在 `~/.zcode-pet/pets/` 下生成 5 只宠物（zbuddy、shiba、ducky、slime、rocky），每只包含 `pet.json` + `spritesheet.webp`（1536×1872）。

### 启动

```bash
# 开发模式
npx tauri dev

# 或直接运行构建好的 app
open src-tauri/target/release/bundle/macos/zcode-pet.app
```

启动后：
- 打开任意 ZCode 会话开始对话，宠物会自动出现并随任务状态变化
- 系统托盘菜单可切换宠物、唤醒/收起全部、退出
- 点击宠物跳回 ZCode 窗口，拖拽移动宠物位置

## 状态联动原理

```
ZCode 7 个 hooks 事件
  → ~/.zcode-pet/bin/zcode-hook（<10ms 原子写入状态文件）
  → ~/.zcode-pet/state/<session_id>.json
  → Tauri 应用（notify 文件监听）
  → 对应会话的宠物窗口更新动画状态
```

| ZCode 事件 | 宠物状态 | 动画 |
|-----------|---------|------|
| SessionStart | idle | 静息呼吸 |
| UserPromptSubmit / PreToolUse / PostToolUse | running | 工作中 |
| PermissionRequest | needs_input | 等待确认（感叹号） |
| Stop | ready | 任务完成（对勾） |
| PostToolUseFailure | blocked | 出错（红叉） |
| —（300s 无 ready 后） | idle→sleep | 打瞌睡（z） |

详见 [状态协议规范](docs/03-state-protocol.md)。

## 自定义宠物

### 方法一：用内置生成器

```bash
# 编辑 tools/petgen.py 中的 PetDef，定义自己的像素造型
python3 tools/petgen.py build --pet my-pet --out ~/.zcode-pet/pets
```

宠物定义使用 Grid 绘图基元（`rect`/`box`/`hline`/`vline`/`ellipse`），自动保证网格宽度一致。

### 方法二：加载 Codex 社区宠物

从 [awesome-codex-pet](https://github.com/legeling/awesome-codex-pet) 或 [Petdex](https://github.com/crafter-station/petdex) 下载宠物包，放到：

```bash
# 放到 Codex 目录（与 Codex 桌面端共享）
cp -r downloaded-pet ~/.codex/pets/my-pet/

# 或放到 zcode-pet 自己的目录
cp -r downloaded-pet ~/.zcode-pet/pets/my-pet/
```

zcode-pet 自动识别 Codex 格式的 `pet.json`（只需 `id`/`displayName`/`description`/`spritesheetPath`），无需额外配置。详见 [Codex 兼容文档](docs/05-codex-compat.md)。

## 托盘菜单

| 菜单项 | 功能 |
|-------|------|
| 切换宠物 → | 选择当前宠物（列表来自三个宠物目录） |
| 唤醒全部 | 显示所有宠物窗口 |
| 收起全部 | 隐藏所有宠物窗口 |
| 退出 | 退出应用 |

## 开发

### 项目结构

```
zcode-pet/
├── docs/              # 设计文档（PRD、架构、协议、计划、兼容）
├── tools/
│   ├── petgen.py      # 像素宠物生成器
│   └── test_petgen.py # 生成器测试
├── scripts/
│   ├── zcode-hook     # hooks 状态写入脚本（POSIX sh）
│   ├── install.sh     # 一键部署 hooks
│   └── test_hook.sh   # hooks 测试
├── src/               # 前端 TypeScript
│   ├── main.ts        # 入口：加载宠物、状态监听、交互
│   ├── animator.ts    # Canvas 精灵表动画引擎
│   ├── compat.ts      # Codex 格式适配层
│   └── protocol.ts    # 状态协议与宠物清单类型
├── src-tauri/         # Rust 后端（Tauri 2）
│   └── src/
│       ├── lib.rs     # 应用入口、托盘菜单、AppState
│       ├── window.rs  # 透明窗口管理、衰减规则
│       ├── watcher.rs # notify 文件监听、衰减定时器
│       ├── pet.rs     # 宠物路径解析、状态文件读写
│       └── activate.rs# 点击激活（osascript）
└── index.html         # 前端入口 HTML
```

### 运行测试

```bash
# Rust 单元测试（状态衰减规则）
cd src-tauri && cargo test

# 前端测试（Codex 兼容层）
npx vitest run

# 像素宠物生成器测试
python3 tools/test_petgen.py

# Hooks 脚本测试
bash scripts/test_hook.sh
```

### 开发模式

```bash
# 前端热更新 + Rust 即时重编译
npx tauri dev

# 预览宠物造型（生成 PNG 预览图）
python3 tools/petgen.py preview --pet zbuddy --out /tmp/preview
```

## 文档

- [需求文档（PRD）](docs/01-requirements.md)
- [架构设计](docs/02-architecture.md)
- [状态协议规范](docs/03-state-protocol.md)
- [开发计划](docs/04-dev-plan.md)
- [Codex 格式兼容](docs/05-codex-compat.md)

## 技术栈

- **桌面框架**：[Tauri 2](https://v2.tauri.app/)（Rust + WebView）
- **前端**：Vite + TypeScript（vanilla，无框架）
- **像素生成**：Python + Pillow
- **文件监听**：[notify](https://docs.rs/notify) + notify-debouncer-full
- **状态协议**：JSON 文件（原子写入，<10ms）

## FAQ

**Q: 宠物不出现？**
A: 检查 `~/.zcode-pet/state/` 下是否有状态文件（启动一个 ZCode 会话发条消息试试）。确认 `~/.zcode-pet/pets/` 下有宠物文件（运行 `python3 tools/petgen.py build`）。

**Q: 点击宠物弹出自动化授权？**
A: macOS 首次通过 AppleScript 激活其他应用会弹授权，允许一次即可。

**Q: 如何用 Codex 桌面端的宠物？**
A: 如果你已装了 Codex 桌面端，宠物在 `~/.codex/pets/`，zcode-pet 会自动扫描发现它们。

**Q: 支持 Windows/Linux 吗？**
A: 架构上保留了可移植性，但目前仅适配了 macOS。透明窗口和激活逻辑需要平台特定实现。

## License

[MIT](LICENSE)
