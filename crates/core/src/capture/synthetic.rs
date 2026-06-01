//! [`SyntheticCapture`]：程序合成一个"绕圈移动"的音源，平移到 7.1 八声道。
//!
//! 用途：阶段 1 开发期零素材驱动整条 DSP→UI 管线，Mac 上立刻可见效。
//! 它实现 [`AudioCapture`]，因此与真实采集实现可无缝替换。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::{AudioCapture, DeviceInfo};
use crate::model::{AudioFrame, CHANNEL_ANGLES_71};

pub struct SyntheticCapture {
    sample_rate: u32,
    frame_size: usize,
    /// 音源绕一圈的周期（秒）。
    orbit_period_s: f32,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Default for SyntheticCapture {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            frame_size: 1024,
            orbit_period_s: 6.0,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

impl SyntheticCapture {
    pub fn new() -> Self {
        Self::default()
    }

    /// 把一个位于 `src_angle`(度) 的音源，按"距角度越近增益越大"平移到 8 声道增益。
    /// 简单余弦平移：gain = max(0, cos(Δangle))^2，对侧/背向声道自然衰减。
    fn pan_gains(src_angle_deg: f32) -> [f32; 8] {
        let mut gains = [0.0f32; 8];
        for (i, ch_angle) in CHANNEL_ANGLES_71.iter().enumerate() {
            if ch_angle.is_nan() {
                continue; // LFE 无方位
            }
            let delta = (src_angle_deg - ch_angle).to_radians();
            let c = delta.cos();
            gains[i] = if c > 0.0 { c * c } else { 0.0 };
        }
        gains
    }
}

impl AudioCapture for SyntheticCapture {
    fn list_devices() -> Vec<DeviceInfo> {
        vec![DeviceInfo {
            id: "synthetic-orbit".into(),
            name: "合成音源（绕圈，开发用）".into(),
            channels: 8,
        }]
    }

    fn start(
        &mut self,
        _device_id: &str,
        mut on_frame: Box<dyn FnMut(AudioFrame) + Send>,
    ) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("已在采集中".into());
        }
        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let sample_rate = self.sample_rate;
        let frame_size = self.frame_size;
        let orbit_period_s = self.orbit_period_s;

        // 用一个 440Hz 正弦作为声源波形；相位与绕圈角度随时间推进。
        let handle = thread::spawn(move || {
            let dt_frame = Duration::from_secs_f64(frame_size as f64 / sample_rate as f64);
            let mut t: f64 = 0.0; // 全局时间（秒）
            let two_pi = std::f32::consts::TAU;
            let tone_freq = 440.0f32;

            while running.load(Ordering::SeqCst) {
                let mut data: Vec<Vec<f32>> = vec![Vec::with_capacity(frame_size); 8];

                for n in 0..frame_size {
                    let time = t + n as f64 / sample_rate as f64;
                    // 音源绕圈：角度随时间线性扫过 360°。
                    let src_angle = ((time / orbit_period_s as f64).fract() as f32) * 360.0 - 180.0;
                    let gains = SyntheticCapture::pan_gains(src_angle);
                    let tone = (two_pi * tone_freq * time as f32).sin() * 0.6;
                    for ch in 0..8 {
                        data[ch].push(tone * gains[ch]);
                    }
                }

                t += frame_size as f64 / sample_rate as f64;
                on_frame(AudioFrame::new(8, sample_rate, data));

                // 近似实时节流（开发用，不追求采样级精确）。
                thread::sleep(dt_frame);
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
