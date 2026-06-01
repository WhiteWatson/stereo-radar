# 立体声音源方位可视化工具 —— 产品 & 技术设计文档

> 版本：v0.1（设计稿）
> 最后更新：2026-06-01
> 状态：设计阶段，待开发

---

## 1. 背景与问题

### 1.1 用户痛点
玩 FPS 游戏（如 PUBG）时，**靠耳机辨别敌人方位**是核心能力。但当耳机素质差、或玩家听觉辨位能力弱时，听不出脚步声/枪声来自哪个方向，导致总是找不到敌人、被偷袭。

### 1.2 产品目标
做一个桌面工具，把游戏里**立体声/环绕声的音源方位实时可视化**出来：
- 以玩家为中心，呈现 **360° 方位**（前后左右及各斜向）。
- **支持同时显示多个音源**（如同时有脚步声 + 枪声 + 载具声）。
- **按应用隔离声源**：只可视化目标游戏（PUBG）的声音，**不混入聊天软件**（和朋友语音不需要辨位）。

### 1.3 非目标（本期不做）
- 不做声音内容识别（不区分"脚步声 vs 枪声"的语义分类）—— 只做方位与强度。
- 不做全屏独占游戏的画面注入式 overlay（先用无边框置顶窗 + 游戏窗口化模式）。
- 不做距离精确测量（强度可作为远近的粗略参考，但不标定米数）。

---

## 2. 核心设计决策（已确认）

这些是经过讨论拍板的关键决策，后续开发以此为准：

| 决策点 | 选择 | 理由 |
|--------|------|------|
| **方位检测原理** | 让游戏输出 **7.1 环绕声**，抓 8 声道，按声道能量反推方位 | 不要硬解立体声 HRTF（不可靠）。7.1 下游戏引擎已把方位信息分配到离散声道，方位是"现成的"。经 CanetisRadar 验证可行。 |
| **应用隔离方式** | **虚拟 7.1 声卡路由**（VoiceMeeter / BlackHole） | 比进程级抓取更通用：工具只管"抓某个虚拟设备"，与具体游戏解耦，任何能路由到该设备的应用都能用。 |
| **采集实现** | 跨平台 **`cpal`** 读虚拟设备的输入端 | 虚拟声卡路线把"抓音频"简化成"读输入设备"，cpal 一套代码覆盖 Win/macOS。 |
| **技术栈** | **Rust 核心 + Tauri / Web UI** | 性能好、体积小、Rust 后端与采集/DSP 无缝、Web 前端画雷达灵活。 |
| **平台定位** | **Windows 一等公民（正式使用），macOS 仅开发机** | 游戏跑在 Windows。Mac 用于写代码/调 UI/调算法，不做实机保证。 |
| **跨平台策略** | 同一代码库，采集层 trait 抽象 + 多实现 | DSP/UI 100% 共享；Mac 上用 `FileCapture` 喂测试音频开发，无需虚拟声卡。 |

---

## 3. 同类方案调研

| 项目 | 技术栈 | 方位思路 | 借鉴点 |
|------|--------|---------|--------|
| [CanetisRadar](https://github.com/SamuelTulach/CanetisRadar) / [CanetisRadar2](https://github.com/Alaanor/CanetisRadar2) | C# / WPF | 7.1 环绕 → 各声道能量定方位 | **核心思路来源**；但音量法有抖动/响应慢问题，需用平滑+门限改进 |
| [StereoSoundView](https://github.com/Shibi-bala/StereoSoundView) | C# / NAudio | 直接分析立体声 L/R | 反面教材：硬解立体声只能左右，做不出 360° |
| [Audio Radar](https://audioradar.com/) / SonicSight | 商业硬件 | 7.1 → LED 灯 | 印证 7.1 路线的市场可行性 |

**两条最重要经验：**
1. 不要试图从 2 声道立体声反推 360°，不可靠。
2. 正确姿势是"让游戏自己用 7.1 把方位告诉你"。

---

## 4. 系统架构

### 4.1 数据流总览

```
任意应用(PUBG/COD/...)
      │  Windows 按应用输出路由 / macOS 应用输出选择
      ▼
 虚拟 7.1 声卡 (VoiceMeeter / BlackHole)
      │
      ├──────────────► 真实耳机（回放/监听，玩家照常听声音）   ★必须保留，否则听不见
      │
      └──────────────► 本工具
                          │
              ┌───────────┴────────────┐
              │   采集层 AudioCapture    │  读虚拟设备 → 多声道 PCM 帧
              └───────────┬────────────┘
                          ▼
              ┌────────────────────────┐
              │     DSP 分析层 dsp       │  每声道 RMS → 向量合成 → 多源 → 平滑
              └───────────┬────────────┘
                          ▼  SourcePoint[]
              ┌────────────────────────┐
              │   桥接 Tauri event       │  ~30–60fps 推送给前端
              └───────────┬────────────┘
                          ▼
              ┌────────────────────────┐
              │  可视化层 360° 雷达 UI   │  透明/置顶/click-through overlay
              └────────────────────────┘
```

### 4.2 分层职责

1. **采集层（Capture）**：从虚拟声卡读多声道 PCM，按固定帧长输出。平台/来源差异封装在这里。
2. **DSP 分析层**：把 PCM 帧转成"音源方位点"列表。纯计算，平台无关。
3. **桥接层（Tauri）**：把 `SourcePoint[]` 通过事件推给前端；管理窗口（透明、置顶、点击穿透）。
4. **可视化层（Web）**：渲染以人为中心的 360° 雷达，多光点表示多音源。

---

## 5. 采集层设计

### 5.1 统一接口

```rust
/// 一帧多声道音频（交错或分平面，约定为分声道 planar）
pub struct AudioFrame {
    pub channels: u16,         // 声道数（期望 8）
    pub sample_rate: u32,      // 采样率，如 48000
    pub data: Vec<Vec<f32>>,   // [channel][sample]，归一化到 [-1, 1]
}

pub trait AudioCapture: Send {
    /// 列出可选的音频输入设备
    fn list_devices() -> Vec<DeviceInfo> where Self: Sized;
    /// 开始采集，每帧回调一次
    fn start(&mut self, device_id: &str, on_frame: impl FnMut(AudioFrame) + Send + 'static) -> Result<()>;
    fn stop(&mut self);
}
```

### 5.2 三个实现

| 实现 | 平台 | 用途 | 说明 |
|------|------|------|------|
| `CpalCapture` | 跨平台 | **主力采集** | 用 [`cpal`](https://docs.rs/cpal) 读虚拟设备的录音端。Windows 读 VoiceMeeter Out，macOS 读 BlackHole。一套代码。 |
| `FileCapture` | 跨平台 | **Mac 开发用 ★** | 循环播放一段录好的 8 声道 WAV（如"声源绕圈"），驱动整条 DSP→UI 链路。不依赖虚拟声卡/真游戏，也用于自动化测试。 |
| `WasapiCapture`（可选） | Windows | 性能/兼容兜底 | 若 cpal 在 Windows 多声道上有问题，用 [`wasapi`](https://docs.rs/wasapi) 直接实现 loopback 兜底。**优先用 cpal，非必要不做。** |

> 设计意图：DSP/UI 只依赖 `AudioCapture` trait，**永远不感知**底层是文件、cpal 还是 wasapi。切换实现 = 换一行构造代码。

### 5.3 帧参数约定
- 帧长：**512 或 1024 sample**（@48kHz ≈ 10.7 / 21.3 ms），在延迟与稳定间取平衡。
- 内部统一转 `f32` planar，便于 DSP 按声道处理。

---

## 6. DSP 方位算法（核心）

### 6.1 7.1 声道 → 标准方位角

以玩家正前方为 0°，顺时针为正，约定各声道的物理角度：

| 声道 | 缩写 | 角度 |
|------|------|------|
| 前左 | FL | −45° |
| 前右 | FR | +45° |
| 中置 | C | 0° |
| 低频 | LFE | 忽略（无方位） |
| 后左 | RL | −135° |
| 后右 | RR | +135° |
| 侧左 | SL | −90° |
| 侧右 | SR | +90° |

> 注：具体声道顺序以设备上报的 channel mask 为准，做一次映射校准。常见 WAVE_FORMAT_EXTENSIBLE 顺序为 FL, FR, C, LFE, RL/BL, RR/BR, SL, SR。

### 6.2 算法流水线

```
每帧 PCM
  └─ 1. 每声道 RMS：  e_c = sqrt(mean(sample_c²))
  └─ 2. 噪声门限：     e_c < gate → 置 0（滤掉环境底噪）
  └─ 3. 向量合成（主方位）：
          V = Σ_c  e_c · (cos θ_c, sin θ_c)      // θ_c 为声道角度
          主方位角 = atan2(V.y, V.x)
          强度 = |V|
  └─ 4. 多音源分离（进阶，阶段2）：
          对各声道能量做峰值/聚类，识别 1~N 个能量簇，
          每簇独立向量合成 → 多个 SourcePoint
  └─ 5. 时间平滑：     每个方位点用指数平滑(EMA) + 峰值保持衰减，
                       缓解 CanetisRadar 的抖动/闪烁问题
  └─ 输出 SourcePoint[]
```

### 6.3 多音源策略（分阶段）
- **MVP（阶段1）**：先不做真正分离，输出"主方位 + 各声道能量条"。
- **阶段2**：基于声道能量分布做简单聚类（例如前向簇 / 后向簇 / 侧向簇分别合成），同时输出多个点。
- **可选增强**：按频段拆分（低频=脚步/载具，高频=枪声/玻璃），每频段独立定位，天然分离多音源。后续评估收益再做。

### 6.4 关键调参项（暴露给用户）
- `gate`：噪声门限，过滤底噪。
- `smoothing`：平滑系数，越大越稳但越迟钝。
- `sensitivity`：强度→视觉映射增益。

---

## 7. 数据契约（采集/DSP → 前端）

```ts
// Tauri 事件 "sources" 每帧推送的 payload
interface SourcePoint {
  id: number;        // 音源稳定 id（跨帧跟踪，便于做拖尾动画）
  angle: number;     // 方位角，度，0=正前，顺时针为正，范围 (-180, 180]
  intensity: number; // 归一化强度 0~1（兼作远近/响度参考）
  band?: 'low' | 'mid' | 'high'; // 可选：频段（若启用频段分离）
}

interface FramePayload {
  ts: number;            // 时间戳(ms)
  sources: SourcePoint[];
  channelEnergies: number[]; // 8 声道原始能量，供"方位条"调试视图
}
```

推送频率：**30–60 fps**（与渲染解耦，可对 DSP 帧做节流/合并）。

---

## 8. 可视化设计

### 8.1 形态
- **无边框、透明背景、永远置顶、鼠标点击穿透**的 overlay 窗口。
- 玩家把它摆在屏幕角落；游戏用**无边框窗口化**模式即可叠加（MVP 不做独占全屏注入）。

### 8.2 雷达视图
- 以中心点代表玩家，画一个 360° 圆形雷达（前方在上）。
- 每个 `SourcePoint` = 一个光点/扇形：
  - **角度** → 光点在圆周上的位置。
  - **强度** → 光点大小 + 颜色（弱→黄，强→红）+ 不透明度。
  - **拖尾**：按 `id` 跨帧跟踪做短拖尾，方向移动更直观。
- 可选辅助：前/后半区底色区分，解决"前后混淆"。

### 8.3 调试视图（开发期）
- 8 根声道能量条（仿 CanetisRadar2），直观看每声道能量，方便校准声道映射。

### 8.4 控制面板
- 设备选择下拉（选要抓哪个虚拟设备）。
- 灵敏度 / 平滑 / 门限 滑杆。
- 显隐热键、透明度调节。

---

## 9. 技术栈与工程结构

### 9.1 技术栈
- **核心 / 采集 / DSP**：Rust（`cpal`、`hound` 读 WAV、必要时 `rustfft` 做频段分离）。
- **桌面外壳 / 桥接**：Tauri（透明窗、置顶、click-through、跨平台打包）。
- **前端**：TypeScript + Canvas/WebGL（雷达渲染），轻量框架或纯 Canvas 皆可。

### 9.2 目录结构

```
stereo-radar/
├── design.md                 # 本文档
├── Cargo.toml                # Rust workspace
├── crates/
│   └── core/                 # 平台无关核心库
│       ├── src/
│       │   ├── capture/
│       │   │   ├── mod.rs        # AudioCapture trait + AudioFrame
│       │   │   ├── cpal.rs       # CpalCapture（跨平台主力）
│       │   │   ├── file.rs       # FileCapture（Mac 开发 / 测试）
│       │   │   └── wasapi.rs     # WasapiCapture（Windows 兜底，可选）
│       │   ├── dsp.rs            # RMS / 向量合成 / 多源 / 平滑
│       │   ├── model.rs          # SourcePoint / FramePayload
│       │   └── lib.rs
│       └── tests/               # 喂固定 WAV → 断言角度
├── src-tauri/                # Tauri 后端：窗口配置 + 事件推送
│   ├── src/main.rs
│   └── tauri.conf.json       # 透明/置顶/穿透/无边框
├── src/                      # 前端
│   ├── radar.ts              # 360° 雷达渲染
│   ├── panel.ts              # 控制面板
│   └── index.html
└── assets/
    └── test_orbit_8ch.wav    # 声源绕圈测试音频（FileCapture 用）
```

---

## 10. 实施路线（里程碑）

| 阶段 | 目标 | 交付物 | 平台 |
|------|------|--------|------|
| **0. 环境** | 跑通虚拟 7.1 声卡链路 | VoiceMeeter 配出 7.1 设备；确认"游戏→设备→耳机能听见 + 能 loopback 抓到 8 声道" | Windows |
| **1. MVP（端到端）** | 打通采集→DSP→UI | `FileCapture` 喂测试音频 → RMS → Tauri 推送 → 前端 8 段方位条 + 主方位光点。Mac 上即可见效。 | Mac 开发 |
| **2. 360° + 多源** | 完整可视化 | 向量合成连续角度雷达；多音源聚类；拖尾动画 | Mac 开发 |
| **3. 实机接入** | 真游戏可用 | 切 `CpalCapture` 抓 VoiceMeeter；设备选择 UI；实机抓 PUBG 验证方位正确 | Windows |
| **4. 打磨** | 体验完善 | 灵敏度/平滑/门限调节、置顶 overlay、热键、前后区分、（可选）频段分离 | 双端 |

**开发节奏**：阶段 1–2 主要在 Mac 用 `FileCapture` 迭代算法和 UI（不依赖真游戏/虚拟声卡）；阶段 3 起在 Windows 实机验证与使用。

---

## 11. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| 虚拟声卡只暴露立体声，拿不到 7.1 | 方位退化成左右 | 阶段 0 务必确认虚拟设备以 7.1 呈现给游戏（VoiceMeeter Potato 支持多声道）；裸 VB-CABLE 默认立体声，需配置或换设备 |
| **玩家听不见声音**（声音被虚拟设备吃掉） | 不可用 | 必须用支持"监听回放"的虚拟设备（VoiceMeeter 同时输出到耳机 A1 + 暴露 loopback）；裸 mute-cable 不行 |
| 音量法抖动 / 响应慢（CanetisRadar 老问题） | 体验差 | EMA 平滑 + 峰值保持 + 噪声门限；参数可调；必要时引入频段分离提升分辨力 |
| 声道顺序因设备而异 | 方位错乱 | 按设备 channel mask 做映射；提供调试视图（8 声道能量条）人工校准 |
| cpal 在 Windows 多声道输入上有坑 | 采集失败 | 保留 `WasapiCapture` 作为兜底实现 |
| macOS 游戏多为立体声 | Mac 上方位弱 | 已接受：Mac 仅作开发机，不保证实机效果 |
| 全屏独占游戏盖住 overlay | 看不到雷达 | MVP 要求游戏用无边框窗口化；独占注入留待后续 |

---

## 12. 待确认 / 后续讨论
- 雷达 UI 的具体视觉风格（拟物雷达 vs 极简光点）。
- 是否需要多音源的频段分离（投入产出比待阶段2评估）。
- 是否需要配置预设（不同游戏不同灵敏度/门限）。
- 打包与分发方式（Tauri 安装包；是否附带 VoiceMeeter 配置引导）。

---

## 附录 A：参考资料
- CanetisRadar: https://github.com/SamuelTulach/CanetisRadar
- CanetisRadar2: https://github.com/Alaanor/CanetisRadar2
- StereoSoundView: https://github.com/Shibi-bala/StereoSoundView
- VoiceMeeter (虚拟声卡): https://vb-audio.com/Voicemeeter/
- BlackHole (macOS 多声道虚拟声卡): https://github.com/ExistentialAudio/BlackHole
- cpal (跨平台音频): https://docs.rs/cpal
- wasapi (Rust WASAPI): https://docs.rs/wasapi
- Tauri: https://tauri.app/
