# zcode-pet

> ZCode 的桌面宠物 —— 一只住在你屏幕上、实时反映 AI 编程任务状态的像素小伙伴。

对标 OpenAI Codex 桌面端的 Pets 功能，为 [ZCode](https://github.com/YZX-Der) 打造的**独立** macOS 桌宠应用：悬浮窗像素动画 + 通过 ZCode hooks 实现的任务状态联动。

## 特性

- 🐣 **像素风桌宠**：透明悬浮窗，始终置顶，可拖拽，位置记忆
- 🔄 **状态联动**：通过 ZCode hooks 实时反映任务状态 —— Running / Needs input / Ready / Blocked / Sleep
- 👆 **点击跳回**：点击宠物激活 ZCode / 终端窗口
- 🐾 **多会话多宠物**：每个 ZCode 会话对应一只宠物，并行任务一目了然
- 🎨 **兼容 Codex 宠物格式**：可直接加载 [Petdex](https://github.com/crafter-station/petdex) / [codexpet.top](https://codexpet.top) 的社区宠物精灵表（1536×1872）
- 🛠️ **代码手绘内置宠物**：自带 5 只程序化生成的像素宠物，可用生成器做你自己的

## 状态

🚧 开发中，见 [开发计划](docs/04-dev-plan.md)。

## 文档

- [需求文档（PRD）](docs/01-requirements.md)
- [架构设计](docs/02-architecture.md)
- [状态协议规范](docs/03-state-protocol.md)
- [开发计划](docs/04-dev-plan.md)

## License

[MIT](LICENSE)
