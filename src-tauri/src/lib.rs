//! Tauri 外壳（Bridge）：唯一的编排者。
//!
//! 职责（见 ARCHITECTURE.md §3.3）：
//! 1. 构造一个 [`AudioCapture`] 实现 + 一个 [`Analyzer`]。
//! 2. 在采集回调里跑 DSP，把 `FramePayload` 作为 `sources` 事件 emit 给前端。
//! 3. 暴露控制通道命令（`set_params` / `list_devices`）。

use std::sync::{Arc, Mutex};
use std::time::Instant;

use stereo_radar_core::capture::SyntheticCapture;
use stereo_radar_core::{Analyzer, AnalyzerParams, AudioCapture, DeviceInfo};
use tauri::{Emitter, Manager};

/// 共享给"控制通道"的可调参数。数据通道（音频回调）只读它，控制命令写它。
struct Shared {
    params: Mutex<AnalyzerParams>,
}

/// 持有采集实例，防止被 drop（其内部线程依赖它存活）。
struct CaptureGuard(#[allow(dead_code)] Mutex<SyntheticCapture>);

/// 控制通道：更新 DSP 参数（其余字段保持当前值）。
#[tauri::command]
fn set_params(shared: tauri::State<'_, Arc<Shared>>, gate: f32, smoothing: f32, sensitivity: f32) {
    let mut p = shared.params.lock().unwrap();
    p.gate = gate;
    p.smoothing = smoothing;
    p.sensitivity = sensitivity;
}

/// 控制通道：列出可选音频来源。
#[tauri::command]
fn list_devices() -> Vec<DeviceInfo> {
    SyntheticCapture::list_devices()
}

pub fn run() {
    let shared = Arc::new(Shared {
        params: Mutex::new(AnalyzerParams::default()),
    });

    tauri::Builder::default()
        .manage(shared.clone())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let shared = shared.clone();

            // DSP 跨帧状态由采集回调线程单独拥有。
            let mut analyzer = Analyzer::new(*shared.params.lock().unwrap());
            let start = Instant::now();

            let mut capture = SyntheticCapture::new();
            capture
                .start(
                    "synthetic-orbit",
                    Box::new(move |frame| {
                        // 每帧同步一次最新参数（控制通道→数据通道）。
                        analyzer.set_params(*shared.params.lock().unwrap());
                        let ts = start.elapsed().as_millis() as u64;
                        let payload = analyzer.process(&frame, ts);
                        let _ = app_handle.emit("sources", payload);
                    }),
                )
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            app.manage(CaptureGuard(Mutex::new(capture)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_params, list_devices])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用出错");
}
