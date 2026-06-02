# CLAUDE.md — 项目记忆 / 开发交接

> 本文件供 Claude Code 自动加载，承接跨设备/跨会话的上下文。
> 详细内容见：[design.md](./design.md)（产品+技术方案）、[ARCHITECTURE.md](./ARCHITECTURE.md)（架构）、[WINDOWS.md](./WINDOWS.md)（Windows 开发与实机）、[BUILD.md](./BUILD.md)（打包发版）。

## 远程仓库
- GitHub: **https://github.com/WhiteWatson/stereo-radar**
- clone：`git clone https://github.com/WhiteWatson/stereo-radar.git`
- 提交身份用个人邮箱 `White <734004037@qq.com>`（本仓库 --local 已配，勿用工作邮箱）。

## 项目是什么
把 FPS 游戏的**音源方位实时可视化**的桌面 overlay 工具。用户耳机差、听不出敌人方位，
本工具以玩家为中心画 360° 雷达，实时显示多个声源的方位与强度。

**定位**：**Nahimic Sound Tracker 的开源替代品**。Nahimic（常预装于 MSI 等）做的是同一件事，
但驱动级、系统级、闭源、绑硬件。我们的差异化卖点：①**按应用隔离**（不混聊天语音，Nahimic 做不好）
②开源可定制 ③不绑硬件、跨平台。决定"做到底"，不止做技术练习。

## 已锁定的核心决策（不要轻易推翻）
> ⚠️ **重大转向（2026-06）**：用户耳机只能输入 **2 声道**（厂商"7.1"是 HRTF 模拟，游戏实际输出立体声）。
> 已从 7.1 方案**转为立体声方案**。7.1 代码保留为 ≥6ch 时的兜底，但主线是立体声。

1. **方位原理（立体声）**：逐频点 `atan2(|R|,|L|)` 反演恒功率声像 → 180° 前向直方图 → 多峰。
   - 左右定位很准（基准 **平均 0.69°**），**前后物理不可分**（雷达后半圆置灰标"未知"）。
   - DSP 按声道数分派：2ch 走立体声(ILD)，≥6ch 走旧 7.1 向量合成（零额外成本保留）。
2. **应用隔离（已不用 VoiceMeeter）**：Windows **WASAPI 进程级 loopback**（`WasapiProcessCapture`），
   直接抓目标进程(如 PUBG)的 2 声道，聊天等其它进程天然排除。**零配置、无需任何虚拟声卡**。
3. **技术栈**：Rust 核心 + Tauri/Web UI。采集 trait `AudioCapture` 多实现：
   `SyntheticCapture`(合成,含orbit/stereo两种) · `CpalCapture`(跨平台输入设备) · `WasapiProcessCapture`(Win进程loopback)。
4. **平台**：**Windows 一等公民（正式使用）**，macOS 仅开发机。
5. **验证技巧**：Windows-only COM 代码可在 Mac 上交叉检查：
   `cargo check -p stereo-radar-core --target x86_64-pc-windows-gnu`（已验证 core 通过）。
   注：`cargo check` 整个 app 在 Mac 会因 tauri-winres 交叉编译限制失败，与业务代码无关，真 Windows 构建正常。

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
- [x] 阶段5-C 系统托盘/菜单栏图标（显隐/锁定/退出，左键显隐）
- [x] 阶段5-B 设置持久化（参数/设备/透明度=localStorage；窗口位置=window-state 插件）
- [x] 阶段5-D 设备检测引导（按声道数提示方位完整度 + 配置指引）
- [x] 立体声转向：ILD 算法(基准0.69°) + 2ch合成源 + 雷达后方置灰 + stereo_accuracy 基准
- [x] #1 设备刷新按钮 · #3 alert→toast 告警治理
- [x] #2 WasapiProcessCapture 进程级 loopback(无需虚拟声卡)，core 已交叉编译验证
- [ ] **下一步(需 Windows)：pull→在 Windows 上 build→下拉选游戏进程→验证抓取+左右方位**
  - WASAPI COM 代码是盲写+交叉检查过的，真机首跑可能仍有运行期问题(激活/格式/PROPVARIANT)，需联调
- [ ] 真实游戏音频调 gate/平滑默认值，抗底噪/瞬态
- [ ] 可选：雷达视觉风格、音源标签、配置预设(每游戏)、开机自启

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
- 默认分支 main；远程已配 origin（见上「远程仓库」）。

## 跨设备继续开发的第一步
1. 在另一台机器 `git clone https://github.com/WhiteWatson/stereo-radar.git`。
2. 装工具链 → `npm install` → `npm run tauri dev` 确认合成源雷达能跑。
3. Windows 上按 WINDOWS.md 配 7.1 声卡，切设备做实机验证。
