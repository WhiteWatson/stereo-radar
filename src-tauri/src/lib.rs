//! Tauri 外壳（Bridge）：唯一的编排者。
//!
//! 职责（见 ARCHITECTURE.md §3.3）：
//! 1. 通过 [`CaptureManager`] 管理当前的 [`AudioCapture`] 实现（可运行期切换设备）。
//! 2. 在采集回调里跑 DSP，把 `FramePayload` 作为 `sources` 事件 emit 给前端。
//! 3. 暴露控制通道命令（`set_params` / `list_devices` / `start_capture`）。

use std::sync::{Arc, Mutex};
use std::time::Instant;

use stereo_radar_core::capture::{CpalCapture, SyntheticCapture};
use stereo_radar_core::{Analyzer, AnalyzerParams, AudioCapture, AudioFrame, DeviceInfo};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const SYNTHETIC_ID: &str = "synthetic-orbit";

/// 管理当前采集来源；切换设备 = 停旧、起新。
struct CaptureManager {
    app: AppHandle,
    params: Arc<Mutex<AnalyzerParams>>,
    current: Option<Box<dyn AudioCapture>>,
    current_id: String,
}

impl CaptureManager {
    fn start(&mut self, device_id: &str) -> Result<(), String> {
        if let Some(mut c) = self.current.take() {
            c.stop();
        }

        let mut cap: Box<dyn AudioCapture> = if device_id.starts_with("synthetic-") {
            Box::new(SyntheticCapture::new())
        } else {
            Box::new(CpalCapture::new())
        };

        // 每个采集会话用一份新的 DSP 状态。
        let app = self.app.clone();
        let params = self.params.clone();
        let mut analyzer = Analyzer::new(*params.lock().unwrap());
        let start = Instant::now();

        cap.start(
            device_id,
            Box::new(move |frame: AudioFrame| {
                analyzer.set_params(*params.lock().unwrap());
                let ts = start.elapsed().as_millis() as u64;
                let payload = analyzer.process(&frame, ts);
                let _ = app.emit("sources", payload);
            }),
        )?;

        self.current = Some(cap);
        self.current_id = device_id.to_string();
        Ok(())
    }
}

/// 控制通道：更新 DSP 参数（其余字段保持当前值）。
#[tauri::command]
fn set_params(
    params: tauri::State<'_, Arc<Mutex<AnalyzerParams>>>,
    gate: f32,
    smoothing: f32,
    sensitivity: f32,
) {
    let mut p = params.lock().unwrap();
    p.gate = gate;
    p.smoothing = smoothing;
    p.sensitivity = sensitivity;
}

/// 控制通道：列出可选来源（合成源 + 真实输入设备）。
#[tauri::command]
fn list_devices() -> Vec<DeviceInfo> {
    let mut v = SyntheticCapture::list_devices();
    v.extend(CpalCapture::list_devices());
    v
}

/// 控制通道：切换采集来源。
#[tauri::command]
fn start_capture(
    mgr: tauri::State<'_, Arc<Mutex<CaptureManager>>>,
    device_id: String,
) -> Result<(), String> {
    mgr.lock().unwrap().start(&device_id)
}

pub fn run() {
    let params = Arc::new(Mutex::new(AnalyzerParams::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(params.clone())
        .setup(move |app| {
            let mgr = Arc::new(Mutex::new(CaptureManager {
                app: app.handle().clone(),
                params: params.clone(),
                current: None,
                current_id: String::new(),
            }));
            // 默认先跑合成源，便于即时看到雷达。
            mgr.lock()
                .unwrap()
                .start(SYNTHETIC_ID)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            app.manage(mgr);

            // 系统托盘/菜单栏：无边框穿透窗的控制入口。
            let show_i = MenuItem::with_id(app, "toggle-show", "显示 / 隐藏", true, None::<&str>)?;
            let lock_i = MenuItem::with_id(app, "toggle-lock", "锁定 / 解锁穿透", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &lock_i, &quit_i])?;

            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("stereo-radar")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle-show" => {
                        let _ = app.emit("menu-toggle-show", ());
                    }
                    "toggle-lock" => {
                        let _ = app.emit("menu-toggle-lock", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // 左键点击图标 = 显隐切换。
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = tray.app_handle().emit("menu-toggle-show", ());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_params, list_devices, start_capture])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用出错");
}
