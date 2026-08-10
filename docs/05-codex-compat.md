# Codex 宠物格式兼容

zcode-pet 兼容 [OpenAI Codex](https://github.com/openai/codex) 桌面应用的宠物精灵表格式，可直接加载 Codex 社区宠物资源（如 [awesome-codex-pet](https://github.com/legeling/awesome-codex-pet)、[Petdex](https://codex-pet.com)）。

## 格式对比

| 维度 | zcode-pet 原生 | Codex |
|------|---------------|-------|
| 精灵表尺寸 | 1536×1872（v1） | **完全一致** |
| 网格 | 8 列 × 9 行，192×208/帧 | **完全一致** |
| pet.json 字段 | 含 `frame`/`cols`/`rows`/`states` | 仅 `id`/`displayName`/`description`/`spritesheetPath` |
| 状态布局 | 在 pet.json 中声明 | 由 app 硬编码 |

> Codex 的几何布局与 zcode-pet v1 完全一致（1536×1872，8×9，192×208），差异仅在元数据存储方式。

## Codex 状态行映射

Codex 有 9 个动画行，zcode-pet 的 6 状态按语义映射：

| zcode-pet 状态 | Codex 行 | Codex 状态名 | 说明 |
|---------------|---------|-------------|------|
| `idle` | 0 | idle | 静息呼吸/眨眼 |
| `running` | 7 | running | 原地工作循环 |
| `needs_input` | 6 | waiting | 等待用户输入 |
| `ready` | 3 | waving | 任务完成招呼 |
| `blocked` | 5 | failed | 出错/受挫 |
| `sleep` | 0 | idle（复用） | Codex 无 sleep 状态 |

## 加载优先级

扫描宠物时按以下顺序查找（同名的只取第一个）：

1. `~/.zcode-pet/pets/<name>/` — zcode-pet 用户自定义
2. `~/.codex/pets/<name>/` — Codex 社区宠物
3. 内置宠物（`assets/pets/` 或回退到 `~/.zcode-pet/pets/`）

## 如何使用 Codex 社区宠物

1. 从 [awesome-codex-pet](https://github.com/legeling/awesome-codex-pet) 或 [Petdex](https://codex-pet.com) 下载宠物包
2. 解压到 `~/.codex/pets/<pet-name>/`（应包含 `pet.json` + `spritesheet.webp`）
3. 启动 zcode-pet，从托盘菜单「切换宠物」中选择

或直接放到 zcode-pet 自己的目录：

```bash
cp -r downloaded-pet ~/.zcode-pet/pets/my-pet/
```

## Codex v2 支持

Codex v2 精灵表为 1536×2288（8 列 × 11 行），额外两行为 16 方向注视动画。
zcode-pet 检测 `spriteVersionNumber: 2` 后自动识别为 11 行布局，方向行（9-10）不参与状态映射。

## 适配层实现

- **前端**：`src/compat.ts` — `isCodexFormat()` 检测格式，`adaptCodexPet()` 转换为 `PetManifest`
- **后端**：`src-tauri/src/pet.rs` — `codex_pets_dir()` 扫描 `~/.codex/pets/`，`list_pets()` 合并三个来源
