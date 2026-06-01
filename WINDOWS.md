# 在 Windows 上继续开发与实机使用

Mac 是开发机，Windows 是正式战场。好消息：**代码是跨平台的，Windows 上不用改任何代码**——
采集层用的是跨平台的 `cpal`，到 Windows 自动走 WASAPI 枚举设备；你只需配好环境 + 选对设备。

---

## 1. 把代码弄到 Windows

当前仓库还是纯本地（无远程）。三选一：

- **A. GitHub（推荐）**：在 Mac 上
  ```bash
  # 装 gh 或用网页建一个空仓库，然后：
  git remote add origin https://github.com/<你>/stereo-radar.git
  git push -u origin main
  ```
  Windows 上 `git clone` 即可，之后两端用 git 同步。
- **B. 局域网/U盘**：直接拷贝整个 `stereo-radar/` 文件夹（**别拷 `target/` 和 `node_modules/`**，到 Windows 重新装即可）。
- **C. 共享盘/网盘**：同 B，注意排除 `target/`、`node_modules/`、`dist/`。

---

## 2. Windows 环境准备

1. **Rust**：装 [rustup](https://rustup.rs/)（默认 MSVC 工具链）。
2. **C++ 生成工具**：装 [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，勾选 "Desktop development with C++"（Tauri 链接需要）。
3. **WebView2**：Win10/11 一般已预装；没有就装 [Evergreen Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)。
4. **Node 18+**：[nodejs.org](https://nodejs.org/)。

验证：
```powershell
rustc --version ; node --version
```

---

## 3. 编译运行

```powershell
cd stereo-radar
npm install
npm run tauri dev      # 首次编译 Rust 依赖较慢
```

启动后默认跑合成源（光点绕圈），证明链路通。接下来配真实声卡。

---

## 4. 配置虚拟 7.1 声卡（实机方位的前提）

目标：把游戏音频以 **7.1 环绕** 送进一个虚拟设备，**同时**你还能从耳机听见。

1. 装 **[VoiceMeeter Potato](https://vb-audio.com/Voicemeeter/potato.htm)**（Potato 版支持多声道/7.1）。
2. 在 VoiceMeeter 里：
   - 把一个 **Virtual Input** 配置成 **7.1**（8 声道）。
   - **A1（硬件输出）= 你的耳机** → 这样你能正常听到声音（关键，别漏）。
3. Windows 声音设置：把 **游戏（如 PUBG）的输出设备** 指到 VoiceMeeter 的 Virtual Input
   （`设置 → 系统 → 声音 → 音量合成器`，给游戏单独指定输出设备）。
   聊天软件保持原耳机 → 天然不被采集。
4. **游戏内音频设置选 7.1 / 环绕声**（否则只渲染立体声，方位信息就没了）。

> 不用 7.1？退而求其次设成立体声也能跑，但只有左右方位（参考 Mac 上的局限说明）。

---

## 5. 在 app 里选对采集设备

1. `npm run tauri dev` 启动后，顶部「声源」下拉里会列出所有输入设备
   （`CpalCapture` 通过 WASAPI 枚举）。
2. 选 **VoiceMeeter 的输出/回放端**（通常叫 `VoiceMeeter Out B1` 或 `VoiceMeeter VAIO Output` 之类的**录音设备**）——它就是游戏 7.1 流的 loopback。
3. 看右侧声道数应显示 **8ch**。若只有 2ch，说明 VoiceMeeter 那路没配成 7.1，回第 4 步。
4. 进游戏，雷达应实时显示敌人脚步/枪声方位。用控制面板的灵敏度/平滑/门限微调。

---

## 6. 排错速查

| 现象 | 可能原因 / 处理 |
|------|----------------|
| 听不到游戏声 | VoiceMeeter A1 没指到耳机；或游戏输出设备选错 |
| 声道数只有 2ch | VoiceMeeter 那路没设 7.1；或游戏内没开环绕声 |
| 雷达只在前方动 | 游戏输出是立体声，没走 7.1 |
| 下拉里找不到 VoiceMeeter | VoiceMeeter 没装好/没重启；重启后再试 |
| 链接/编译报错缺 MSVC | 第 2 步的 C++ 生成工具没装 |
| 方位整体偏转/镜像 | 声道顺序与预期不符，需校准 `CHANNEL_ANGLES_71` 映射（用 8 声道能量条逐个核对） |

---

## 7. 开发分工建议（双机）

- **Mac**：写 DSP / 调雷达 UI / 跑 `cargo run --example accuracy` 看精度回归。用合成源与文件源迭代，不依赖真游戏。
- **Windows**：实机验证方位、用 VoiceMeeter 抓真游戏、最终自己用。
- 用 git 在两端同步；采集层已抽象，两端共用同一套代码，无需分叉。
