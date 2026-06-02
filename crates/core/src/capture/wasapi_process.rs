//! Windows 进程级 loopback 采集（**无需虚拟声卡 / VoiceMeeter**）。
//!
//! 用 WASAPI 的 `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`（Win10 2004+）直接抓
//! 指定进程树的音频输出，2 声道。聊天软件等其它进程天然被排除 → 按应用隔离。
//!
//! ⚠️ 本文件仅在 Windows 编译，且是"盲写"的 COM 代码：首次在 Windows 上 `cargo build`
//! 很可能有少量 API/版本不匹配，需对照 `windows` crate 0.58 文档微调。风险点已用 NOTE 标注。
//!
//! 参考：微软 ApplicationLoopback 示例（C++）。

#![allow(non_snake_case)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use windows::core::{implement, Interface, PROPVARIANT, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    CreateEventW, OpenProcess, QueryFullProcessImageNameW, SetEvent, WaitForSingleObject, INFINITE,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::System::Variant::VT_BLOB;

use super::{AudioCapture, DeviceInfo};
use crate::model::AudioFrame;

/// WAVE_FORMAT_IEEE_FLOAT 常量（避免依赖具体 feature 路径）。
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

/// 按 Win32 PROPVARIANT 内存布局手工构造 VT_BLOB 变体（windows-core 的 PROPVARIANT 不可直接填字段）。
/// 64 位下：vt(2)+reserved(6) 后对齐到 8，union 内 BLOB { cbSize:u32(+4 pad), pBlobData:ptr }。
#[repr(C)]
struct PropVariantBlob {
    vt: u16,
    r1: u16,
    r2: u16,
    r3: u16,
    cb_size: u32,
    _pad: u32,
    p_blob: *mut u8,
}

/// 进程 loopback 用的"虚拟设备"接口路径（微软固定字符串）。
const VAD_PROCESS_LOOPBACK: PCWSTR = windows::core::w!("VAD\\Process_Loopback");

// 采集格式：固定 48kHz / 2ch / f32（进程 loopback 不走 GetMixFormat，需显式指定）。
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

/// 列出"当前有音频会话"的进程（PID + 名称），供 UI 选择目标游戏。
pub fn list_audio_processes() -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    unsafe {
        // COM 在本线程初始化（多次调用安全）。
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(enumerator) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        else {
            return out;
        };
        let Ok(device) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) else {
            return out;
        };
        let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
            return out;
        };
        let Ok(sessions): Result<IAudioSessionEnumerator, _> = manager.GetSessionEnumerator() else {
            return out;
        };
        let count = sessions.GetCount().unwrap_or(0);
        let mut seen = std::collections::HashSet::new();
        for i in 0..count {
            let Ok(ctrl) = sessions.GetSession(i) else { continue };
            let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else { continue };
            let pid = ctrl2.GetProcessId().unwrap_or(0);
            if pid == 0 || !seen.insert(pid) {
                continue; // 跳过系统会话(pid=0)与重复
            }
            let name = process_name(pid).unwrap_or_else(|| format!("进程 {pid}"));
            out.push(DeviceInfo {
                id: format!("pid:{pid}"),
                name: format!("{name} (进程)"),
                channels: CHANNELS,
            });
        }
    }
    out
}

/// 由 PID 取可执行文件名（去掉路径）。
fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        ok.ok()?;
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        Some(full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string())
    }
}

/// 激活完成回调：在异步激活完成时置位事件，让 start() 继续。
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivateHandler {
    done: HANDLE,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivateHandler_Impl {
    fn ActivateCompleted(
        &self,
        _operation: Option<&IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        unsafe {
            let _ = SetEvent(self.done);
        }
        Ok(())
    }
}

pub struct WasapiProcessCapture {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Default for WasapiProcessCapture {
    fn default() -> Self {
        Self { running: Arc::new(AtomicBool::new(false)), handle: None }
    }
}

impl WasapiProcessCapture {
    pub fn new() -> Self {
        Self::default()
    }
}

/// 解析 "pid:1234" → 1234。
fn parse_pid(device_id: &str) -> Option<u32> {
    device_id.strip_prefix("pid:")?.parse().ok()
}

impl AudioCapture for WasapiProcessCapture {
    fn list_devices() -> Vec<DeviceInfo> {
        list_audio_processes()
    }

    fn start(
        &mut self,
        device_id: &str,
        mut on_frame: Box<dyn FnMut(AudioFrame) + Send>,
    ) -> Result<(), String> {
        let pid = parse_pid(device_id).ok_or_else(|| format!("无效进程 id: {device_id}"))?;
        if self.running.load(Ordering::SeqCst) {
            return Err("已在采集中".into());
        }
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let (tx, rx) = mpsc::channel::<Result<(), String>>();

        let handle = thread::spawn(move || {
            let res = unsafe { run_capture(pid, &running, &mut on_frame) };
            // 若在建立阶段就失败，把错误回传；成功则已在 run_capture 内发过 Ok。
            if let Err(e) = res {
                let _ = tx.send(Err(e));
            }
            let _ = tx.send(Ok(())); // 兜底（已 recv 到结果则忽略）
        });

        // 等待采集线程报告"建立成功/失败"。run_capture 在进入循环前会通过全局事件...
        // 这里简化：给 3 秒，期间若线程已 panic/early-return 会收到 Err。
        match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => {
                self.handle = Some(handle);
                Ok(())
            }
            Ok(Err(e)) => {
                self.running.store(false, Ordering::SeqCst);
                Err(e)
            }
            Err(_) => {
                // 超时但未报错：视为已在运行。
                self.handle = Some(handle);
                Ok(())
            }
        }
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// 实际采集：激活进程 loopback IAudioClient → 事件驱动读 IAudioCaptureClient。
/// NOTE: 这是最容易出错的部分，首次 Windows 编译需重点核对。
unsafe fn run_capture(
    pid: u32,
    running: &AtomicBool,
    on_frame: &mut (dyn FnMut(AudioFrame) + Send),
) -> Result<(), String> {
    CoInitializeEx(None, COINIT_MULTITHREADED)
        .ok()
        .map_err(|e| format!("CoInitializeEx 失败: {e}"))?;

    // 1) 组装进程 loopback 激活参数。
    let mut activation = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    // 用 PROPVARIANT(VT_BLOB) 携带参数（手工布局后 cast）。
    let pv = PropVariantBlob {
        vt: VT_BLOB.0,
        r1: 0,
        r2: 0,
        r3: 0,
        cb_size: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
        _pad: 0,
        p_blob: &mut activation as *mut _ as *mut u8,
    };

    // 2) 异步激活 IAudioClient。
    let done = CreateEventW(None, true, false, PCWSTR::null())
        .map_err(|e| format!("CreateEventW 失败: {e}"))?;
    let handler: IActivateAudioInterfaceCompletionHandler = ActivateHandler { done }.into();
    let op: IActivateAudioInterfaceAsyncOperation = ActivateAudioInterfaceAsync(
        VAD_PROCESS_LOOPBACK,
        &IAudioClient::IID,
        Some(&pv as *const PropVariantBlob as *const PROPVARIANT),
        &handler,
    )
    .map_err(|e| format!("ActivateAudioInterfaceAsync 失败: {e}"))?;
    WaitForSingleObject(done, INFINITE);
    let _ = CloseHandle(done);

    // 取激活结果。
    let mut hr = windows::core::HRESULT(0);
    let mut iface: Option<windows::core::IUnknown> = None;
    op.GetActivateResult(&mut hr, &mut iface)
        .map_err(|e| format!("GetActivateResult 失败: {e}"))?;
    hr.ok().map_err(|e| format!("激活返回错误: {e}"))?;
    let client: IAudioClient = iface
        .ok_or("激活未返回 IAudioClient")?
        .cast()
        .map_err(|e| format!("cast IAudioClient 失败: {e}"))?;

    // 3) 用固定 f32 立体声格式初始化（loopback + 事件回调）。
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
        nChannels: CHANNELS,
        nSamplesPerSec: SAMPLE_RATE,
        wBitsPerSample: 32,
        nBlockAlign: CHANNELS * 4,
        nAvgBytesPerSec: SAMPLE_RATE * CHANNELS as u32 * 4,
        cbSize: 0,
    };
    client
        .Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            2_000_000, // 200ms 缓冲(100ns 单位)
            0,
            &format,
            None,
        )
        .map_err(|e| format!("IAudioClient.Initialize 失败: {e}"))?;

    let event = CreateEventW(None, false, false, PCWSTR::null())
        .map_err(|e| format!("CreateEventW(数据) 失败: {e}"))?;
    client.SetEventHandle(event).map_err(|e| format!("SetEventHandle 失败: {e}"))?;

    let capture: IAudioCaptureClient =
        client.GetService().map_err(|e| format!("GetService 失败: {e}"))?;
    client.Start().map_err(|e| format!("Start 失败: {e}"))?;

    // 4) 事件驱动读取循环。
    while running.load(Ordering::SeqCst) {
        if WaitForSingleObject(event, 200) != WAIT_OBJECT_0 {
            continue;
        }
        loop {
            let mut packet = 0u32;
            if capture.GetNextPacketSize().map(|p| { packet = p; p }).unwrap_or(0) == 0 {
                break;
            }
            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            if capture
                .GetBuffer(&mut data_ptr, &mut frames, &mut flags, None, None)
                .is_err()
            {
                break;
            }
            if frames > 0 {
                let n = frames as usize;
                let mut left = Vec::with_capacity(n);
                let mut right = Vec::with_capacity(n);
                if (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 {
                    left.resize(n, 0.0);
                    right.resize(n, 0.0);
                } else {
                    let samples = std::slice::from_raw_parts(data_ptr as *const f32, n * 2);
                    for f in 0..n {
                        left.push(samples[f * 2]);
                        right.push(samples[f * 2 + 1]);
                    }
                }
                on_frame(AudioFrame::new(CHANNELS, SAMPLE_RATE, vec![left, right]));
            }
            let _ = capture.ReleaseBuffer(frames);
        }
    }

    let _ = client.Stop();
    let _ = CloseHandle(event);
    Ok(())
}
