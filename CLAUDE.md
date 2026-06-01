# CLAUDE.md — 项目记忆 / 开发交接

> 本文件供 Claude Code 自动加载，承接跨设备/跨会话的上下文。
> 详细内容见：[design.md](./design.md)（产品+技术方案）、[ARCHITECTURE.md](./ARCHITECTURE.md)（架构）、[WINDOWS.md](./WINDOWS.md)（Windows 开发与实机）。

## 项目是什么
把 FPS 游戏的**音源方位实时可视化**的桌面 overlay 工具。用户耳机差、听不出敌人方位，
本工具以玩家为中心画 360° 雷达，实时显示多个声源的方位与强度。

## 已锁定的核心决策（不要轻易推翻）
1. **方位原理**：不硬解立体声。让游戏输出 **7.1 环绕声**，抓 8 声道，按声道能量反推方位。
2. **应用隔离**：用**虚拟 7.1 声卡**路由（Windows=VoiceMeeter Potato / macOS=BlackHole）。
   工具只"抓某个虚拟设备"，与具体游戏解耦 → 通用。游戏路由到虚拟设备，聊天软件留在真实耳机。
   ⚠️ 虚拟设备必须**同时回放到耳机**（监听），否则玩家听不见。
3. **技术栈**：Rust 核心 + Tauri/Web UI。采集用跨平台 **cpal**（Win 走 WASAPI、mac 走 CoreAudio）。
4. **平台**：**Windows 是一等公民（正式使用）**，macOS 仅开发机（游戏在 Windows）。
5. **跨平台**：同一代码库；采集层 `AudioCapture` trait 抽象，DSP/UI 100% 共享。

## 架构一句话
单向管线：`Capture(平台相关,trait后) → DSP(无状态,平台无关) → Bridge(Tauri编排+emit) → UI(Canvas雷达)`。
依赖只能从外向内指向 `core`，`core` 不依赖 Tauri/UI。详见 ARCHITECTURE.md。

## 代码结构
```
crates/core/         平台无关核心
  src/model.rs        AudioFrame / SourcePoint / 7.1声道角度表(CHANNEL_ANGLES_71)
  src/capture/        AudioCapture trait + 三实现:
    synthetic.rs        合成绕圈源(开发占位, 当前默认启动)
    cpal_capture.rs     CpalCapture 抓真实输入设备(规避macOS Stream非Send)
  src/dsp.rs          Analyzer: 加窗FFT→逐频点向量合成→角度直方图→多峰→跨帧跟踪EMA
  examples/accuracy.rs 精度基准(无需声卡): cargo run -p stereo-radar-core --example accuracy
src-tauri/           Tauri外壳: CaptureManager(运行期切设备) + 命令 set_params/list_devices/start_capture
src/                 前端: radar.ts(雷达+拖尾) panel.ts(调参) device.ts(选设备) overlay.ts(穿透/热键)
```

## 算法精度（已用 examples/accuracy 验证）
- 单源定位：平均 **2.43°**、最大 2.5°（达直方图 5°/bin 量化极限），全方位准。
- 双源**不同频谱**（脚步vs枪声，典型场景）：干净分离到 **~30°** 间隔。
- 双源**同频谱**（两个白噪声，最坏情况）：需 ~180° 才分开 —— 已知局限，对真实场景影响不大。
- 改算法后请重跑该 example 做回归。

## 进度
- [x] 阶段1 端到端骨架（合成源→DSP→Tauri事件→Canvas雷达）
- [x] 阶段2 多音源分离(角度直方图+FFT) + 拖尾 + 控制面板 + 前后区分
- [x] 阶段3 CpalCapture + 运行期设备切换 + 设备下拉 + 精度基准（**代码完成，待 Windows 实机验证**）
- [x] 阶段4 overlay：透明无边框置顶 + 点击穿透 + 全局热键(⌘⇧L锁定/⌘⇧H显隐) + 拖动 + 透明度
- [ ] **下一步：上 Windows，按 WINDOWS.md 配 VoiceMeeter 7.1，选其输出端实机验证方位**
- [ ] 可选打磨：雷达视觉风格、音源标签、空闲动画、配置预设

## 怎么跑
```bash
npm install
npm run tauri dev      # 启动；默认跑合成源(两个红点绕圈=测试数据)
cargo test -p stereo-radar-core                          # 单元测试
cargo run -p stereo-radar-core --example accuracy        # 精度基准
```
- 默认启动的两个绕圈红点是 `SyntheticCapture` 占位数据；选别的「声源」即切真实采集。
- Windows 实机：选 VoiceMeeter 7.1 输出端，确认显示 8ch；游戏内开环绕声。详见 WINDOWS.md。

## 环境/工具链注意
- 需要：Rust(rustup)、Node18+、Tauri 前置（Win 需 VS C++ Build Tools + WebView2）。
- macOS 透明窗需 `app.macOSPrivateApi:true` + tauri crate `macos-private-api` feature（已配）。
- 全局热键需 macOS 输入监控/辅助功能授权。
- `.gitignore` 已排除 target/node_modules/dist；本项目是 app，**Cargo.lock 入库**。

## 约定
- 提交信息中文、结尾带 `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`。
- 默认分支 main；目前**无远程**，跨设备需先建 GitHub 仓库 push（见 WINDOWS.md §1）。

## 跨设备继续开发的第一步
1. 在另一台机器 `git clone`（或先在本机建 GitHub remote 并 push）。
2. 装工具链 → `npm install` → `npm run tauri dev` 确认合成源雷达能跑。
3. Windows 上按 WINDOWS.md 配 7.1 声卡，切设备做实机验证。
