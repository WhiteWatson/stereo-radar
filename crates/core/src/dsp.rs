//! DSP 分析层：把 [`AudioFrame`] 变换为 [`SourcePoint`] 列表（多音源）。
//!
//! 算法（阶段 2，角度直方图法）：
//! 1. 每声道加窗 FFT。
//! 2. 逐频点用 8 声道幅度做向量合成 → 该频点的方位角 + 方向性(directivity)。
//!    方向性低（能量在各声道均摊、弥散）的频点被抑制。
//! 3. 把各频点能量按方位累加成一个 360° 角度直方图。
//! 4. 在（环形平滑后的）直方图上挑多个峰 → 多个音源。
//! 5. 跨帧最近邻跟踪：给每个音源稳定 id + 角度/强度 EMA 平滑（用于拖尾）。
//!
//! 不同方位的声源落在直方图不同峰上 → 空间分离；
//! 不同频段的声源（低频脚步 / 高频枪声）频点天然分布不同 → 也被一并定位。

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

use crate::model::{AudioFrame, FramePayload, SourcePoint, CHANNEL_ANGLES_71};

/// 角度直方图分辨率（5°/bin）。
const NUM_BINS: usize = 72;
const BIN_WIDTH_DEG: f32 = 360.0 / NUM_BINS as f32;

/// 可调参数（由控制通道注入）。
#[derive(Debug, Clone, Copy)]
pub struct AnalyzerParams {
    /// 强度门限：归一化后峰值强度低于此值的音源被丢弃。
    pub gate: f32,
    /// EMA 平滑系数 0~1：越大越稳但越迟钝（作用于被跟踪音源的角度/强度）。
    pub smoothing: f32,
    /// 强度→视觉增益。
    pub sensitivity: f32,
    /// 最多输出几个音源。
    pub max_sources: usize,
    /// 两个音源的最小角度间隔（度），更近的弱峰被并掉。
    pub min_separation_deg: f32,
    /// 次峰相对最强峰的最小高度比，低于则忽略（抑制噪声小峰）。
    pub peak_ratio: f32,
}

impl Default for AnalyzerParams {
    fn default() -> Self {
        Self {
            gate: 0.05,
            smoothing: 0.5,
            sensitivity: 1.4,
            max_sources: 4,
            min_separation_deg: 22.0,
            peak_ratio: 0.22,
        }
    }
}

/// 持有跨帧状态（FFT 计划、窗、被跟踪音源、id 计数）的分析器。单线程拥有。
pub struct Analyzer {
    params: AnalyzerParams,
    planner: FftPlanner<f32>,
    fft: Option<(usize, Arc<dyn Fft<f32>>)>,
    window: Vec<f32>, // Hann 窗，长度随帧长缓存
    tracked: Vec<SourcePoint>,
    next_id: u32,
}

impl Analyzer {
    pub fn new(params: AnalyzerParams) -> Self {
        Self {
            params,
            planner: FftPlanner::new(),
            fft: None,
            window: Vec::new(),
            tracked: Vec::new(),
            next_id: 0,
        }
    }

    pub fn set_params(&mut self, params: AnalyzerParams) {
        self.params = params;
    }

    /// 每声道时域 RMS（供调试用的声道能量条）。
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

    /// 确保 FFT 计划与窗匹配当前帧长。
    fn ensure_fft(&mut self, n: usize) {
        if self.fft.as_ref().map(|(len, _)| *len) != Some(n) {
            let fft = self.planner.plan_fft_forward(n);
            self.fft = Some((n, fft));
            // Hann 窗
            self.window = (0..n)
                .map(|i| {
                    let x = std::f32::consts::PI * i as f32 / (n as f32 - 1.0);
                    x.sin().powi(2)
                })
                .collect();
        }
    }

    /// 处理一帧，输出推送给前端的载荷。
    pub fn process(&mut self, frame: &AudioFrame, ts: u64) -> FramePayload {
        let energies = Self::channel_rms(frame);
        let n = frame.samples_per_channel();

        if n < 16 {
            return FramePayload { ts, sources: vec![], channel_energies: energies };
        }
        self.ensure_fft(n);
        let (_, fft) = self.fft.clone().unwrap();

        // 每声道 FFT（加 Hann 窗）。
        let half = n / 2;
        let mut spectra: Vec<Vec<f32>> = Vec::with_capacity(frame.channels as usize);
        for ch in &frame.data {
            let mut buf: Vec<Complex<f32>> = (0..n)
                .map(|i| Complex::new(ch.get(i).copied().unwrap_or(0.0) * self.window[i], 0.0))
                .collect();
            fft.process(&mut buf);
            spectra.push(buf[..half].iter().map(|c| c.norm()).collect());
        }

        // 频点范围：跳过 DC 和极低/极高，聚焦有方位意义的频段。
        let bin_lo = ((50.0 * n as f32) / frame.sample_rate as f32).floor() as usize;
        let bin_hi = (((12_000.0 * n as f32) / frame.sample_rate as f32).ceil() as usize).min(half);

        // 角度直方图：逐频点向量合成，按方向性加权累加。
        let mut hist = [0.0f32; NUM_BINS];
        for k in bin_lo..bin_hi {
            let (mut vx, mut vy, mut mag) = (0.0f32, 0.0f32, 0.0f32);
            for (c, ang) in CHANNEL_ANGLES_71.iter().enumerate() {
                if ang.is_nan() {
                    continue; // LFE 无方位
                }
                let m = spectra.get(c).and_then(|s| s.get(k)).copied().unwrap_or(0.0);
                mag += m;
                let rad = ang.to_radians();
                vx += m * rad.cos();
                vy += m * rad.sin();
            }
            if mag <= 1e-9 {
                continue;
            }
            let directivity = (vx * vx + vy * vy).sqrt() / mag; // 0(弥散)~1(集中)
            let energy = mag * directivity * directivity; // 抑制弥散频点
            let angle = vy.atan2(vx).to_degrees();
            Self::deposit(&mut hist, angle, energy);
        }

        // 环形 3-tap 平滑。
        let hist = Self::smooth_circular(&hist);
        let global_max = hist.iter().cloned().fold(0.0f32, f32::max);

        let mut detected = Vec::new();
        if global_max > 1e-9 {
            detected = self.pick_peaks(&hist, global_max);
        }

        // 跨帧跟踪 + 平滑，赋稳定 id。
        let sources = self.track(detected);
        self.tracked = sources.clone();

        FramePayload { ts, sources, channel_energies: energies }
    }

    /// 把能量投到最近直方图 bin，并以三角权重溢出到相邻 bin（平滑）。
    fn deposit(hist: &mut [f32; NUM_BINS], angle_deg: f32, energy: f32) {
        let pos = (angle_deg + 180.0) / BIN_WIDTH_DEG; // [0, NUM_BINS)
        let center = pos.floor() as isize;
        for (off, w) in [(-1isize, 0.25f32), (0, 0.5), (1, 0.25)] {
            let idx = (center + off).rem_euclid(NUM_BINS as isize) as usize;
            hist[idx] += energy * w;
        }
    }

    fn smooth_circular(hist: &[f32; NUM_BINS]) -> [f32; NUM_BINS] {
        let mut out = [0.0f32; NUM_BINS];
        for i in 0..NUM_BINS {
            let l = hist[(i + NUM_BINS - 1) % NUM_BINS];
            let r = hist[(i + 1) % NUM_BINS];
            out[i] = 0.25 * l + 0.5 * hist[i] + 0.25 * r;
        }
        out
    }

    /// 在环形直方图上挑峰，返回原始（未跟踪）音源，按强度降序、满足最小间隔。
    fn pick_peaks(&self, hist: &[f32; NUM_BINS], global_max: f32) -> Vec<SourcePoint> {
        let abs_thresh = global_max * self.params.peak_ratio;

        // 收集局部极大值。
        let mut cands: Vec<(f32, f32)> = Vec::new(); // (angle_deg, height)
        for i in 0..NUM_BINS {
            let h = hist[i];
            let l = hist[(i + NUM_BINS - 1) % NUM_BINS];
            let r = hist[(i + 1) % NUM_BINS];
            if h >= l && h > r && h >= abs_thresh {
                // 抛物线插值求亚 bin 角度。
                let denom = l - 2.0 * h + r;
                let delta = if denom.abs() > 1e-9 { 0.5 * (l - r) / denom } else { 0.0 };
                let angle = -180.0 + (i as f32 + 0.5 + delta) * BIN_WIDTH_DEG;
                cands.push((normalize_angle(angle), h));
            }
        }
        cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // 强制最小角度间隔，取前 max_sources 个。
        let mut chosen: Vec<(f32, f32)> = Vec::new();
        for (angle, h) in cands {
            if chosen.len() >= self.params.max_sources {
                break;
            }
            if chosen
                .iter()
                .all(|(a, _)| angular_diff(*a, angle).abs() >= self.params.min_separation_deg)
            {
                chosen.push((angle, h));
            }
        }

        chosen
            .into_iter()
            .filter_map(|(angle, h)| {
                let intensity = (h.sqrt() * self.params.sensitivity).clamp(0.0, 1.0);
                if intensity < self.params.gate {
                    None
                } else {
                    Some(SourcePoint { id: 0, angle, intensity })
                }
            })
            .collect()
    }

    /// 最近邻把本帧峰关联到上一帧音源，继承 id 并 EMA 平滑；未匹配的给新 id。
    fn track(&mut self, detected: Vec<SourcePoint>) -> Vec<SourcePoint> {
        let a = self.params.smoothing.clamp(0.0, 0.95);
        let assoc_thresh = (self.params.min_separation_deg * 1.5).max(20.0);

        let mut prev = self.tracked.clone();
        let mut out = Vec::with_capacity(detected.len());

        for mut s in detected {
            // 找最近的上一帧音源。
            let mut best: Option<(usize, f32)> = None;
            for (i, p) in prev.iter().enumerate() {
                let d = angular_diff(p.angle, s.angle).abs();
                if d <= assoc_thresh && best.map_or(true, |(_, bd)| d < bd) {
                    best = Some((i, d));
                }
            }
            match best {
                Some((i, _)) => {
                    let p = prev.remove(i);
                    s.id = p.id;
                    s.angle = ema_angle(p.angle, s.angle, a);
                    s.intensity = a * p.intensity + (1.0 - a) * s.intensity;
                }
                None => {
                    s.id = self.next_id;
                    self.next_id = self.next_id.wrapping_add(1);
                }
            }
            out.push(s);
        }
        out
    }
}

/// 归一化到 (-180, 180]。
fn normalize_angle(mut a: f32) -> f32 {
    while a <= -180.0 {
        a += 360.0;
    }
    while a > 180.0 {
        a -= 360.0;
    }
    a
}

/// 环形角度差（度），范围 (-180, 180]。
fn angular_diff(a: f32, b: f32) -> f32 {
    normalize_angle(a - b)
}

/// 角度的 EMA：在向量域插值，避免 ±180 跳变。
fn ema_angle(prev: f32, new: f32, a: f32) -> f32 {
    let (pr, nr) = (prev.to_radians(), new.to_radians());
    let x = a * pr.cos() + (1.0 - a) * nr.cos();
    let y = a * pr.sin() + (1.0 - a) * nr.sin();
    y.atan2(x).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在某声道注入一个指定频率的正弦。
    fn add_tone(data: &mut [Vec<f32>], ch: usize, freq: f32, sr: u32, amp: f32) {
        let n = data[ch].len();
        for i in 0..n {
            data[ch][i] += amp * (std::f32::consts::TAU * freq * i as f32 / sr as f32).sin();
        }
    }

    fn empty_frame(n: usize, _sr: u32) -> Vec<Vec<f32>> {
        vec![vec![0.0f32; n]; 8]
    }

    #[test]
    fn single_source_front_left() {
        let sr = 48_000;
        let mut a = Analyzer::new(AnalyzerParams { smoothing: 0.0, ..Default::default() });
        let mut d = empty_frame(1024, sr);
        add_tone(&mut d, 0 /* FL */, 600.0, sr, 0.5);
        let p = a.process(&AudioFrame::new(8, sr, d), 0);
        assert_eq!(p.sources.len(), 1, "应只有一个音源");
        assert!((p.sources[0].angle - (-45.0)).abs() < 8.0, "FL≈-45°, got {}", p.sources[0].angle);
    }

    #[test]
    fn two_sources_distinct_angles() {
        let sr = 48_000;
        let mut a = Analyzer::new(AnalyzerParams { smoothing: 0.0, ..Default::default() });
        let mut d = empty_frame(1024, sr);
        add_tone(&mut d, 0 /* FL  -45° */, 500.0, sr, 0.5);
        add_tone(&mut d, 5 /* RR  135° */, 3000.0, sr, 0.5);
        let p = a.process(&AudioFrame::new(8, sr, d), 0);
        assert_eq!(p.sources.len(), 2, "应识别出两个音源, got {:?}", p.sources);
        let mut angles: Vec<f32> = p.sources.iter().map(|s| s.angle).collect();
        angles.sort_by(|x, y| x.partial_cmp(y).unwrap());
        assert!((angles[0] - (-45.0)).abs() < 10.0, "源1≈-45°, got {}", angles[0]);
        assert!((angles[1] - 135.0).abs() < 10.0, "源2≈135°, got {}", angles[1]);
    }

    #[test]
    fn silence_yields_no_source() {
        let sr = 48_000;
        let mut a = Analyzer::new(AnalyzerParams { smoothing: 0.0, ..Default::default() });
        let p = a.process(&AudioFrame::new(8, sr, empty_frame(1024, sr)), 0);
        assert!(p.sources.is_empty(), "静音不应产生音源");
    }

    #[test]
    fn stable_id_across_frames() {
        let sr = 48_000;
        let mut a = Analyzer::new(AnalyzerParams::default());
        let mut d = empty_frame(1024, sr);
        add_tone(&mut d, 1 /* FR 45° */, 800.0, sr, 0.5);
        let f = AudioFrame::new(8, sr, d);
        let id1 = a.process(&f, 0).sources[0].id;
        let id2 = a.process(&f, 1).sources[0].id;
        assert_eq!(id1, id2, "同一持续音源跨帧 id 应稳定");
    }
}
