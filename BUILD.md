# 打包与分发

开发时跑的是 `npm run tauri dev`（热重载、未优化）。要分发用 `npm run tauri build` 出正式安装包。

## 1. 本地打包

前置：Rust、Node 18+、各平台 Tauri 前置（Win 需 VS C++ Build Tools + WebView2）。

```bash
npm install
npm run tauri build
```

产物在 `target/release/bundle/`：

| 平台 | 在哪台机器打 | 产物 |
|------|------------|------|
| macOS | Mac | `macos/stereo-radar.app` + `dmg/stereo-radar_<ver>_<arch>.dmg` |
| Windows | Windows | `msi/*.msi`（WiX）+ `nsis/*-setup.exe`（NSIS） |
| Linux | Linux | `deb/*.deb` + `appimage/*.AppImage` |

> ⚠️ **Tauri 不能跨平台打包**：Windows 安装器必须在 Windows 上构建，Mac 上打不出来。

体积参考：约 3–10 MB（Tauri 用系统 WebView，不内嵌 Chromium，比 Electron 小一个数量级）。

### macOS 通用二进制（Intel + Apple Silicon）
```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

## 2. 签名 / 公证

未签名也能装能用，只是系统会拦一下；**自用或小范围分发可不签名**：
- **macOS**：未签名 → Gatekeeper 提示"无法验证开发者"。让用户**右键 →「打开」**确认一次即可。正式分发需 Apple Developer 账号（$99/年）签名 + 公证。
- **Windows**：未签名 → SmartScreen 蓝色提示。让用户点**「更多信息」→「仍要运行」**。正式分发需代码签名证书。

## 3. 自动发版（GitHub Actions）

仓库已配 `.github/workflows/release.yml`：**推送一个 `v*` tag 就自动在 macOS + Windows 上各打一次包，上传到一个 GitHub Release（草稿）。** 你不用再手动跨机器构建。

发版步骤：
```bash
# 1) 改版本号（三处保持一致）
#    - package.json            "version"
#    - src-tauri/tauri.conf.json "version"
#    - Cargo.toml workspace.package.version
# 2) 提交后打 tag 并推送
git tag v0.1.0
git push origin v0.1.0
```

随后到仓库 **Actions** 看构建，完成后 **Releases** 里会有一个**草稿** release，附 macOS `.dmg` 和 Windows `.msi/.exe`。检查无误后点 **Publish** 发布。

- 草稿模式（`releaseDraft: true`）：给你一次人工确认机会，不会直接公开。
- 需要正式签名时，把证书配成 GitHub Secrets 再在 workflow 里启用（参考 tauri-action 文档），当前为未签名构建。

## 4. 版本号管理

发版前确保这三处版本一致：`package.json` / `src-tauri/tauri.conf.json` / 根 `Cargo.toml`。tag 名（如 `v0.1.0`）即 release 名。
