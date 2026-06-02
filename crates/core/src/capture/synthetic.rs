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

    /// 恒功率立体声平移：pan∈[-1(左),1(右)] → (gainL, gainR)。
    fn stereo_pan(pan: f32) -> (f32, f32) {
        let t = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
        (t.cos(), t.sin())
    }
}

/// 周期为 1 的三角波，值域 [-1, 1]（用于让 pan 在左右间来回）。
fn triangle(x: f32) -> f32 {
    let p = x - x.floor(); // [0,1)
    4.0 * (p - 0.5).abs() - 1.0
}

impl AudioCapture for SyntheticCapture {
    fn list_devices() -> Vec<DeviceInfo> {
        vec![
            DeviceInfo {
                id: "synthetic-orbit".into(),
                name: "合成·绕圈 (7.1, 开发用)".into(),
                channels: 8,
            },
            DeviceInfo {
                id: "synthetic-stereo".into(),
                name: "合成·左右移动 (立体声, 开发用)".into(),
                channels: 2,
            },
        ]
    }

    fn start(
        &mut self,
        device_id: &str,
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
        let stereo = device_id == "synthetic-stereo";

        // 两个声源演示多音源：
        //   A) 220Hz（低频，仿脚步）
        //   B) 3000Hz（高频，仿枪声）
        // 7.1 模式：两者反向绕圈（空间+频段分离）。
        // 立体声模式：两者在左右间来回移动（前向 180°，验证 ILD 法）。
        let handle = thread::spawn(move || {
            let dt_frame = Duration::from_secs_f64(frame_size as f64 / sample_rate as f64);
            let mut t: f64 = 0.0; // 全局时间（秒）
            let two_pi = std::f32::consts::TAU;
            let period_a = orbit_period_s as f64;
            let period_b = (orbit_period_s * 1.6) as f64;
            let channels = if stereo { 2 } else { 8 };

            while running.load(Ordering::SeqCst) {
                let mut data: Vec<Vec<f32>> = vec![Vec::with_capacity(frame_size); channels];

                for n in 0..frame_size {
                    let time = t + n as f64 / sample_rate as f64;
                    let tone_a = (two_pi * 220.0 * time as f32).sin() * 0.5;
                    let tone_b = (two_pi * 3000.0 * time as f32).sin() * 0.45;

                    if stereo {
                        // 三角波在 [-1,1] 间来回的 pan；A、B 反相错开
                        let pan_a = triangle((time / period_a) as f32);
                        let pan_b = -triangle((time / period_b) as f32);
                        let (la, ra) = SyntheticCapture::stereo_pan(pan_a);
                        let (lb, rb) = SyntheticCapture::stereo_pan(pan_b);
                        data[0].push(tone_a * la + tone_b * lb);
                        data[1].push(tone_a * ra + tone_b * rb);
                    } else {
                        let angle_a = ((time / period_a).fract() as f32) * 360.0 - 180.0;
                        let angle_b = 90.0 - ((time / period_b).fract() as f32) * 360.0;
                        let gains_a = SyntheticCapture::pan_gains(angle_a);
                        let gains_b = SyntheticCapture::pan_gains(angle_b);
                        for ch in 0..8 {
                            data[ch].push(tone_a * gains_a[ch] + tone_b * gains_b[ch]);
                        }
                    }
                }

                t += frame_size as f64 / sample_rate as f64;
                on_frame(AudioFrame::new(channels as u16, sample_rate, data));

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
