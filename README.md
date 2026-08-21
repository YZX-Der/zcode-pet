# zcode-pet

> ZCode 的桌面宠物 -- 一只住在你屏幕上、实时反映 AI 编程任务状态的像素小伙伴。

为 ZCode 打造的 macOS 桌宠应用：透明悬浮窗像素动画 + 通过 ZCode hooks 实现的任务状态联动 + Dashboard 控制面板。单一桌宠始终跟随当前活跃会话，任务执行时奔跑、等待输入时提醒、完成后待命、长时间无活动变淡但不消失。

## 截图

![zbuddy 宠物预览](docs/screenshots/zbuddy-preview.png)

> 内置 5 只像素宠物：zbuddy、shiba、ducky、slime、rocky

## 特性

- 🐾 **单一桌宠**：一只宠物始终跟随当前 ZCode 会话，切换会话时毫秒级检测并即时跟随（不重建窗口）；后台会话/其他对话的状态不干扰桌宠显示
- 🔄 **状态联动**：通过 ZCode hooks 实时反映任务状态 -- Running / Needs input / Ready / Blocked / Idle / Sleep
- 💬 **状态气泡**：状态变化时气泡闪现（像素风箭头指向宠物），时长可配置；鼠标移入宠物可随时查看
- ⚠️ **权限确认弹窗**：ZCode 需要确认权限（执行命令/写文件等）时，桌宠在气泡下方弹出确认窗，显示工具名/风险/参数摘要，一键跳转 ZCode 确认；**已在 ZCode 窗口时只显示气泡提示，不弹窗遮挡**
- 💤 **智能衰减**：长时间无活动桌宠变淡（35% 透明度）但不消失；非当前会话自动回收
- ⚙️ **Dashboard 控制面板**：macOS 26 液态玻璃风格，配置页 + 会话列表 + 使用说明
- 🎨 **宠物图片网格**：可视化选择宠物，实时预览大小/透明度效果
- 📋 **会话列表**：显示活跃会话的任务名、状态，当前会话高亮标记
- 👆 **点击跳回**：点击宠物激活 ZCode 窗口（先点击聚焦，再点击跳转）
- 🖱️ **右键菜单**：切换宠物 / 显示隐藏宠物 / 显示主窗口 / 退出
- 🖼️ **兼容 Codex 宠物格式**：可直接加载 [awesome-codex-pet](https://github.com/legeling/awesome-codex-pet) / [Petdex](https://github.com/crafter-station/petdex) 社区宠物精灵表
- 🛠️ **代码手绘内置宠物**：自带 5 只程序化生成的像素宠物

## 快速开始

### 方式一：下载发行包（推荐）

1. 从 [Releases](https://github.com/YZX-Der/zcode-pet/releases) 下载 `zcode-pet_0.2.8_aarch64.dmg`
2. 打开 dmg，将 zcode-pet 拖到「应用程序」文件夹
3. 首次打开：右键 -> 打开（绕过 Gatekeeper）
4. 启动后打开「说明」页，点击「一键安装」启用 hooks 联动
5. 内置 5 只宠物已随应用打包，无需额外生成

### 方式二：从源码构建

```bash
git clone https://github.com/YZX-Der/zcode-pet.git
cd zcode-pet

# 安装依赖
npm install

# 构建 release 版本（自动编译前端 + Rust + 打包 .app/.dmg）
npx tauri build
```

构建产物在 `src-tauri/target/release/bundle/macos/`。

### 安装 Hooks 联动

**方式一（推荐）**：启动应用 -> 打开「说明」页 -> 点击「一键安装」按钮，自动部署 hook 脚本并合并 ZCode 配置（自动备份）。

**方式二（终端）**：

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

## 状态联动

```
ZCode 7 个 hooks 事件
  -> ~/.zcode-pet/bin/zcode-hook（<10ms 原子写入状态文件）
  -> ~/.zcode-pet/state/<session_id>.json
  -> Tauri 应用（notify 文件监听）
  -> 桌宠窗口更新动画状态（单一桌宠跟随当前会话）
```

| ZCode 事件 | 宠物状态 | 动画 |
|-----------|---------|------|
| SessionStart | idle | 静息呼吸 |
| UserPromptSubmit / PreToolUse / PostToolUse | running | 工作中 |
| PermissionRequest | needs_input | 等待确认 |
| Stop | ready | 任务完成 |
| PostToolUseFailure | blocked | 出错 |
| -（600s 无活动） | sleep | 睡觉（透明度降至 35%） |

### 衰减与回收

| 规则 | 阈值 | 效果 |
|------|------|------|
| R1 ready 衰减 | ready 持续 300s | ready -> idle |
| R2 睡眠 | 任意状态 600s | -> sleep（变淡显示） |
| R3 回收 | 1800s 无活动 | 删状态文件（当前会话豁免，不消失） |

详见 [状态协议规范](docs/03-state-protocol.md)。

## Dashboard 控制面板

### 配置页

- **显示桌宠** / **始终置顶**：全局开关
- **状态气泡** / **气泡消失时间**：气泡开关 + 自动消失时长（2/3/5/10 秒；永久模式任务状态常驻显示，空闲/休眠 3 秒后自动隐藏）
- **宠物大小** / **不透明度**：滑块实时预览
- **选择宠物**：图片网格展示所有宠物，点击切换

### 会话页

- 显示活跃会话列表（idle/running/needs_input/ready/blocked 五种状态）
- 当前会话带「当前」徽章并排第一
- 每条会话显示任务名（从 ZCode 会话索引读取）+ 状态标签
- 长时间无活动的会话自动清理

## 自定义宠物

### 方法一：用内置生成器

```bash
# 编辑 tools/petgen.py 中的 PetDef，定义自己的像素造型
python3 tools/petgen.py build --pet my-pet --out ~/.zcode-pet/pets
```

### 方法二：加载 Codex 社区宠物

从 [awesome-codex-pet](https://github.com/legeling/awesome-codex-pet) 或 [Petdex](https://github.com/crafter-station/petdex) 下载宠物包，放到：

```bash
# 放到 Codex 目录（与 Codex 桌面端共享）
cp -r downloaded-pet ~/.codex/pets/my-pet/

# 或放到 zcode-pet 自己的目录
cp -r downloaded-pet ~/.zcode-pet/pets/my-pet/
```

详见 [Codex 格式兼容](docs/05-codex-compat.md)。

## 托盘 / 右键菜单

| 菜单项 | 功能 |
|-------|------|
| 🦊 当前会话 | 会话信息区：模型 / 思考等级 / 上下文 / Token / 缓存命中 / 思考 |
| 切换宠物 -> | 选择当前宠物 |
| 显示宠物 / 隐藏宠物 | 显示或隐藏桌宠窗口 |
| 显示主窗口 | 打开 Dashboard 控制面板 |
| 退出 | 退出应用 |

> 托盘右键菜单顶部显示**当前会话详情**（实时读取）：模型、思考等级（reasoning effort）、上下文容量、Token 使用量、缓存命中率、思考 token。每次打开菜单都是最新数据。
> 桌宠上右键可弹出同样的菜单（在宠物右侧展开），无需到托盘操作。

## 开发

### 项目结构

```
zcode-pet/
├── docs/                 # 设计文档 + 截图
├── tools/petgen.py       # 像素宠物生成器
├── scripts/
│   ├── zcode-hook        # hooks 状态写入脚本（POSIX sh）
│   └── install.sh        # 一键部署 hooks
├── src/                  # 前端 TypeScript
│   ├── dashboard.ts      # Dashboard 控制面板逻辑
│   ├── pet-window.ts     # 宠物浮窗逻辑
│   ├── animator.ts       # Canvas 精灵表动画引擎
│   ├── compat.ts         # Codex 格式适配层
│   └── protocol.ts       # 状态协议类型
├── src-tauri/src/        # Rust 后端（Tauri 2）
│   ├── lib.rs            # 应用入口、托盘菜单
│   ├── window.rs         # 桌宠窗口管理、衰减规则
│   ├── watcher.rs        # notify 文件监听、衰减定时器
│   ├── dashboard.rs      # 会话列表、桌宠开关命令
│   ├── settings.rs       # 用户设置持久化
│   ├── pet.rs            # 宠物路径解析、当前会话识别
│   └── activate.rs       # 点击激活 ZCode
└── index.html / pet.html # 前端入口
```

### 运行测试

```bash
# Rust 单元测试
cd src-tauri && cargo test

# 前端测试
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
- **UI 风格**：macOS 26 液态玻璃（backdrop-filter + 自定义组件）
- **像素生成**：Python + Pillow
- **文件监听**：[notify](https://docs.rs/notify) + notify-debouncer-full
- **会话索引**：rusqlite（只读 ZCode tasks-index.sqlite）

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
