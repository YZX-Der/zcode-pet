# 03 状态协议规范

| | |
|---|---|
| 版本 | 1 |
| 日期 | 2026-08-10 |

本协议是 ZCode hooks 与 zcode-pet 应用之间的**唯一事实来源**。hook 脚本只负责写，应用只负责读，双方通过本目录契约解耦。

## 1. 目录布局

```
~/.zcode-pet/
├── bin/
│   └── zcode-hook              # hook 脚本（POSIX sh，无扩展名）
├── state/
│   └── <session_id>.json       # 每个 ZCode 会话一个状态文件
├── pets/
│   └── <name>/                 # 用户自定义/第三方宠物（pet.json + spritesheet）
└── config.json                 # 应用配置（点击激活目标列表等）
```

## 2. 状态文件 schema

`<session_id>.json`：

```json
{
  "version": 1,
  "session_id": "2f8c1a3e-...",
  "state": "running",
  "event": "PreToolUse",
  "tool": "Bash",
  "project": "zcode-pet",
  "project_dir": "/Users/xxx/work/zcode-pet",
  "ts": 1786000000
}
```

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| version | number | ✅ | 协议版本，当前恒为 1 |
| session_id | string | ✅ | ZCode 会话 ID（环境变量注入） |
| state | string | ✅ | 见下方状态枚举 |
| event | string | ✅ | 触发本次写入的 hook 事件名 |
| tool | string | ❌ | 工具事件时的工具名（PreToolUse 等） |
| project | string | ❌ | 项目目录 basename（用于宠物标签展示） |
| project_dir | string | ❌ | 项目目录全路径 |
| ts | number | ✅ | Unix 秒级时间戳 |

**状态枚举**：`idle` | `running` | `needs_input` | `ready` | `blocked`

（`sleep` 不写文件，由应用侧根据失联规则推导。）

## 3. 事件 → 状态映射

| ZCode hook 事件 | 写入状态 | 说明 |
|---|---|---|
| SessionStart | idle | 会话启动（打开对话），仅记录状态，不创建桌宠 |
| UserPromptSubmit | running | 用户提交任务 |
| PreToolUse | running | 工具调用开始 |
| PostToolUse | running | 工具调用完成（任务仍在继续） |
| PermissionRequest | needs_input | 等待用户批准 |
| PostToolUseFailure | blocked | 工具失败/出错 |
| Stop | ready | 本轮任务完成，有未读结果 |

## 4. 应用侧创建、衰减与回收规则

**创建时机（C1）**：单一桌宠模式--只为 ZCode 当前活跃会话（rollout 最近修改的 sess_* 文件）创建桌宠，其他会话不显示桌宠。切换会话时旧桌宠关闭、新当前会话的桌宠出现。当前会话即使 idle/ready/sleep 也保持显示（sleep 变淡），不回收。

| 规则 | 阈值 | 效果 |
|---|---|---|
| R1 ready 衰减 | ready 持续 300s 无新事件 | ready → idle |
| R2 睡眠 | 任意状态 600s 无新事件 | → sleep（睡眠动画） |
| R3 失联回收 | 任意状态 1800s 无新事件 | 会话判定死亡，关闭对应宠物窗并删除状态文件（**当前活跃会话豁免**，保持淡显示不回收） |
| R4 聚合优先级 | needs_input > blocked > ready > running > idle | 超出窗口上限（5）时按优先级取舍 |

> R2/R3 的考量：长时间运行的 Bash 命令期间不会产生任何事件，因此 running 不单独设短超时，统一由 R2/R3 兜底；ZCode 崩溃（无 Stop）导致的孤儿会话由 R3 回收。

## 4.1 会话管理（Dashboard）

- **活跃定义**：仅 `running` / `needs_input` / `blocked`（有效状态）算执行中；会话列表只展示执行中的会话，任务完成后自动移出列表（桌宠按 R1-R3 渐隐回收）。
- **桌宠开关**：按会话持久化在 `~/.zcode-pet/config.json` 的 `disabled_sessions`；被禁用的会话不再创建宠物窗口（已有窗口立即关闭），重新打开后如仍在执行则恢复。
- **关闭会话**：立即关闭宠物窗口、删除状态文件、移出列表；若该会话在 ZCode 中仍在执行，后续事件会重新写入状态文件并恢复。
- **任务名**：从 ZCode 会话索引 `~/.zcode/v2/tasks-index.sqlite`（`tasks.title`）读取，与 `session_id` 一一对应；数据库不可用或尚未生成标题时 fallback 到项目名。

## 5. 写入规范（hook 脚本）

1. 从环境变量读取：`CLAUDE_SESSION_ID`（会话 ID）、`CLAUDE_PROJECT_DIR`（项目目录）；事件名与可选工具名来自命令行参数。
2. 缺失 session_id 时静默退出 0（不阻断 ZCode）。
3. **原子写入**：先写 `<file>.tmp` 再 `mv` 覆盖，杜绝应用读到半个 JSON。
4. 不输出任何 stdout/stderr 内容，退出码恒为 0（避免被 ZCode 误判为 block/error）。
5. 执行耗时目标 < 10ms，hook 配置 timeoutMs 2000 兜底。
6. 不写入 prompt 文本、工具参数、文件路径内容等敏感信息（仅工具名与项目目录名）。

## 6. 版本兼容

- 读取方必须忽略未知字段。
- version 升级时，读取方对低于自身版本的文件按本规范 v1 语义处理；高于自身版本时按"未知状态 → idle"降级渲染。
