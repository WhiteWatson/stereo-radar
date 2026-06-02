//! 采集层：把"音频来源差异"关进 [`AudioCapture`] trait 后面。
//!
//! 这是系统里唯一允许出现平台相关代码的地方（见 ARCHITECTURE.md §3.1）。
//! 阶段 1 仅提供 [`SyntheticCapture`]；`FileCapture`(读 WAV) / `CpalCapture`
//! 作为同接口的兄弟实现后续补入。

mod cpal_capture;
mod synthetic;
#[cfg(windows)]
mod wasapi_process;

pub use cpal_capture::CpalCapture;
pub use synthetic::SyntheticCapture;
#[cfg(windows)]
pub use wasapi_process::WasapiProcessCapture;

/// 列出可按进程采集的音频进程（仅 Windows 有内容；其它平台返回空）。
pub fn list_audio_processes() -> Vec<DeviceInfo> {
    #[cfg(windows)]
    {
        wasapi_process::list_audio_processes()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

use crate::model::AudioFrame;

/// 一个可选的音频来源设备。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub channels: u16,
}

/// 音频来源的统一接口。push 模型：底层驱动每产出一帧，就调用上游回调。
pub trait AudioCapture: Send {
    /// 列出可选设备。
    fn list_devices() -> Vec<DeviceInfo>
    where
        Self: Sized;

    /// 开始采集。`on_frame` 在采集线程上被反复调用，**不可阻塞**。
    fn start(
        &mut self,
        device_id: &str,
        on_frame: Box<dyn FnMut(AudioFrame) + Send>,
    ) -> Result<(), String>;

    /// 停止采集并释放资源。
    fn stop(&mut self);
}
