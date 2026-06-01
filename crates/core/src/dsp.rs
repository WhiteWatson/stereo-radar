//! DSP 分析层：把 [`AudioFrame`] 变换为 [`SourcePoint`]。
//!
//! 阶段 1 只做"主方位"：每声道 RMS → 噪声门限 → 能量加权向量合成 → EMA 平滑。
//! 多音源分离（聚类 / 频段）留到阶段 2，在 [`Analyzer`] 内增强即可，契约不变。

use crate::model::{AudioFrame, FramePayload, SourcePoint, CHANNEL_ANGLES_71};

/// 可调参数（由控制通道注入，见 ARCHITECTURE.md §4）。
#[derive(Debug, Clone, Copy)]
pub struct AnalyzerParams {
    /// 噪声门限：声道 RMS 低于此值视为 0。
    pub gate: f32,
    /// EMA 平滑系数 0~1：越大越稳但越迟钝。
    pub smoothing: f32,
    /// 强度→视觉增益。
    pub sensitivity: f32,
}

impl Default for AnalyzerParams {
    fn default() -> Self {
        Self { gate: 0.0008, smoothing: 0.6, sensitivity: 1.0 }
    }
}

/// 持有跨帧状态（平滑）的分析器。单线程拥有，不共享可变状态。
pub struct Analyzer {
    params: AnalyzerParams,
    /// 上一帧平滑后的方位向量（用于 EMA）。
    smoothed_vec: (f32, f32),
}

impl Analyzer {
    pub fn new(params: AnalyzerParams) -> Self {
        Self { params, smoothed_vec: (0.0, 0.0) }
    }

    pub fn set_params(&mut self, params: AnalyzerParams) {
        self.params = params;
    }

    /// 每声道 RMS。
    fn channel_rms(frame: &AudioFrame) -> Vec<f32> {
        frame
            .data
            .iter()
            .map(|ch| {
                if ch.is_empty() {
                    return 0.0;
                }
                let sum_sq: f32 = ch.iter().map(|s| s * s).sum();
                (sum_sq / ch.len() as f32).sqrt()
            })
            .collect()
    }

    /// 处理一帧，输出推送给前端的载荷。
    pub fn process(&mut self, frame: &AudioFrame, ts: u64) -> FramePayload {
        let energies = Self::channel_rms(frame);

        // 能量加权向量合成（按各声道标准角度）。
        let (mut vx, mut vy) = (0.0f32, 0.0f32);
        for (i, &e_raw) in energies.iter().enumerate() {
            let angle = CHANNEL_ANGLES_71.get(i).copied().unwrap_or(f32::NAN);
            if angle.is_nan() {
                continue; // 忽略 LFE / 越界声道
            }
            let e = if e_raw < self.params.gate { 0.0 } else { e_raw };
            let rad = angle.to_radians();
            vx += e * rad.cos();
            vy += e * rad.sin();
        }

        // EMA 平滑向量，缓解抖动。
        let a = self.params.smoothing.clamp(0.0, 1.0);
        self.smoothed_vec.0 = a * self.smoothed_vec.0 + (1.0 - a) * vx;
        self.smoothed_vec.1 = a * self.smoothed_vec.1 + (1.0 - a) * vy;
        let (sx, sy) = self.smoothed_vec;

        let magnitude = (sx * sx + sy * sy).sqrt();
        let mut sources = Vec::new();
        if magnitude > self.params.gate {
            let angle_deg = sy.atan2(sx).to_degrees();
            let intensity = (magnitude * self.params.sensitivity).clamp(0.0, 1.0);
            sources.push(SourcePoint { id: 0, angle: angle_deg, intensity });
        }

        FramePayload { ts, sources, channel_energies: energies }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一帧：只有某个声道有能量，断言算出的角度≈该声道标准角度。
    fn frame_with_channel(ch: usize, amp: f32) -> AudioFrame {
        let mut data = vec![vec![0.0f32; 256]; 8];
        for s in data[ch].iter_mut() {
            *s = amp;
        }
        AudioFrame::new(8, 48_000, data)
    }

    #[test]
    fn front_left_channel_maps_near_minus_45() {
        // smoothing=0 关闭平滑，单帧即收敛。
        let mut a = Analyzer::new(AnalyzerParams { smoothing: 0.0, ..Default::default() });
        let payload = a.process(&frame_with_channel(0 /* FL */, 0.5), 0);
        let src = payload.sources.first().expect("应识别出一个音源");
        assert!((src.angle - (-45.0)).abs() < 1.0, "FL 角度应≈-45°, got {}", src.angle);
    }

    #[test]
    fn silence_yields_no_source() {
        let mut a = Analyzer::new(AnalyzerParams { smoothing: 0.0, ..Default::default() });
        let payload = a.process(&AudioFrame::new(8, 48_000, vec![vec![0.0; 256]; 8]), 0);
        assert!(payload.sources.is_empty(), "静音不应产生音源");
    }
}
