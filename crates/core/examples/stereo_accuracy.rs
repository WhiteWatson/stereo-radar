//! 立体声方位精度基准（无需声卡）。
//!
//! 用恒功率声像平移把宽带噪声源放到**已知左右位置**，过完整立体声 DSP 管线，
//! 比较识别出的角度与真值，扫一遍统计误差。范围仅前向 [-90,90]（立体声无法判前后）。
//!
//! 运行：`cargo run -p stereo-radar-core --example stereo_accuracy`

use stereo_radar_core::{Analyzer, AnalyzerParams, AudioFrame};

const SR: u32 = 48_000;
const N: usize = 2048;

struct Noise(u32);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }
}

/// 把噪声源按恒功率平移到 pan∈[-1,1]，叠加进 2 声道。
fn pan_noise(l: &mut [f32], r: &mut [f32], pan: f32, amp: f32, seed: u32) {
    let t = (pan + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
    let (gl, gr) = (t.cos(), t.sin());
    let mut noise = Noise(seed);
    for i in 0..N {
        let s = noise.next() * amp;
        l[i] += s * gl;
        r[i] += s * gr;
    }
}

fn pan_tone(l: &mut [f32], r: &mut [f32], pan: f32, freq: f32, amp: f32) {
    let t = (pan + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
    let (gl, gr) = (t.cos(), t.sin());
    for i in 0..N {
        let s = amp * (std::f32::consts::TAU * freq * i as f32 / SR as f32).sin();
        l[i] += s * gl;
        r[i] += s * gr;
    }
}

/// 真值角(度) → pan[-1,1]：恒功率下 pan = 角度/90。
fn angle_to_pan(deg: f32) -> f32 {
    deg / 90.0
}

fn main() {
    let params = AnalyzerParams { smoothing: 0.0, ..Default::default() };

    println!("== 单源精度扫描（每 10°，前向 [-90,90]）==");
    println!("{:>6} | {:>8} | {:>7}", "真值°", "识别°", "误差°");
    println!("{}", "-".repeat(28));

    let mut errs = Vec::new();
    let mut worst = (0.0f32, 0.0f32);
    let mut seed = 0x1234_5678u32;
    for tgt in (-90..=90).step_by(10) {
        let target = tgt as f32;
        let (mut l, mut r) = (vec![0.0f32; N], vec![0.0f32; N]);
        pan_noise(&mut l, &mut r, angle_to_pan(target), 0.6, seed);
        seed = seed.wrapping_add(1);
        let mut a = Analyzer::new(params);
        let p = a.process(&AudioFrame::new(2, SR, vec![l, r]), 0);
        match p.sources.first() {
            Some(s) => {
                let err = s.angle - target;
                errs.push(err.abs());
                if err.abs() > worst.1 {
                    worst = (target, err.abs());
                }
                println!("{:>6} | {:>8.1} | {:>+7.1}", tgt, s.angle, err);
            }
            None => println!("{:>6} | {:>8} | {:>7}", tgt, "未检出", "-"),
        }
    }
    let n = errs.len() as f32;
    let mean = errs.iter().sum::<f32>() / n;
    let mut sorted = errs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let max = sorted.last().copied().unwrap_or(0.0);
    println!("\n单源误差: 平均 {:.2}°, 最大 {:.2}°(@{:.0}°)", mean, max, worst.0);

    println!("\n== 双源分离·不同频谱（左偏脚步260Hz vs 右偏枪声3500Hz）==");
    println!("{:>8} | {:>6} | {:>16}", "间隔°", "检出数", "识别角度°");
    println!("{}", "-".repeat(36));
    let left_at: f32 = -40.0;
    for sep in [20.0f32, 30.0, 45.0, 60.0, 90.0, 120.0] {
        let right_at = (left_at + sep).min(90.0);
        let (mut l, mut r) = (vec![0.0f32; N], vec![0.0f32; N]);
        pan_tone(&mut l, &mut r, angle_to_pan(left_at), 260.0, 0.6);
        pan_tone(&mut l, &mut r, angle_to_pan(right_at), 3500.0, 0.55);
        let mut a = Analyzer::new(params);
        let p = a.process(&AudioFrame::new(2, SR, vec![l, r]), 0);
        let mut angles: Vec<f32> = p.sources.iter().map(|s| s.angle).collect();
        angles.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let astr = angles.iter().map(|a| format!("{:.0}", a)).collect::<Vec<_>>().join(", ");
        println!("{:>8.0} | {:>6} | {:>16}", sep, p.sources.len(), astr);
    }
    println!("\n真值: 左源={:.0}°, 右源=(左+间隔)° (封顶90°)", left_at);
    println!("\n注：立体声仅给左右(前向)方位，前后无法区分——这是 2 声道的物理限制。");
}
