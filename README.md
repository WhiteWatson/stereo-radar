# stereo-radar

将游戏的立体声/环绕声**音源方位实时可视化**的桌面工具，帮助辨别 FPS 游戏中敌人来自哪个方向。

- 以玩家为中心呈现 **360° 方位**，支持**同时显示多个音源**。
- 通过**虚拟 7.1 声卡路由**实现按应用隔离：只可视化目标游戏（如 PUBG），不混入聊天软件。
- 技术栈：**Rust 核心 + Tauri / Web UI**，跨平台代码库（Windows 正式使用，macOS 仅开发机）。

## 文档

完整的产品与技术设计见 **[design.md](./design.md)**。

## 开发 / 运行（阶段 1）

前置：Rust 工具链、Node 18+。

```bash
npm install            # 安装前端依赖
npm run tauri dev      # 启动应用（首次编译 Rust 依赖较慢）
```

启动后会看到一个 360° 雷达，**合成音源（`SyntheticCapture`）驱动一个光点顺时针绕圈**，下方 8 根声道能量条同步跳动——证明 `采集 → DSP → 事件 → 渲染` 全链路打通。无需虚拟声卡或真实游戏。

只跑核心库测试：

```bash
cargo test -p stereo-radar-core
```

## 状态

阶段 1（端到端骨架）已完成：`SyntheticCapture` + DSP 向量合成 + Tauri 事件推送 + Canvas 雷达。
后续：阶段 2 多音源、阶段 3 接入 `CpalCapture` 抓虚拟 7.1 声卡（详见 design.md §10）。
