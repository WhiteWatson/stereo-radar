//! 算法精度基准（无需任何声卡）。
//!
//! 做法：用"恒功率成对平移"(constant-power pairwise panning) 把一个宽带噪声源
//! 放到**已知角度** —— 这是真实 7.1 游戏渲染器常用的声像法，且与识别算法
//! （能量加权向量合成）**不同**，因此不是自证。然后跑完整 DSP 管线，比较识别
//! 出的角度与真值，扫一圈统计误差。
//!
//! 运行：`cargo run -p stereo-radar-core --example accuracy`

use stereo_radar_core::model::CHANNEL_ANGLES_71;
use stereo_radar_core::{Analyzer, AnalyzerParams, AudioFrame};

const SR: u32 = 48_000;
const N: usize = 2048;

/// 7.1 扬声器环（按 [0,360) 角度升序），元素为 (角度, 声道下标)。
/// 注意：7.1 没有正后方(180°)扬声器，背后由 RL/RR(±135°) 覆盖 —— 这是真实布局。
fn speaker_ring() -> Vec<(f32, usize)> {
    let mut ring: Vec<(f32, usize)> = CHANNEL_ANGLES_71
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.is_nan()) // 跳过 LFE
        .map(|(i, &a)| (if a < 0.0 { a + 360.0 } else { a }, i))
        .collect();
    ring.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    ring.push((ring[0].0 + 360.0, ring[0].1)); // 环绕闭合
    ring
}

/// 可复现的白噪声（LCG）。
struct Noise(u32);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }
}

/// 把一个噪声源按恒功率平移到真值角度 target（度，[0,360)），叠加进 frame。
fn pan_source_into(frame: &mut [Vec<f32>], target_deg: f32, amp: f32, seed: u32, ring: &[(f32, usize)]) {
    // 找包住 target 的相邻扬声器对。
    let mut pair = (ring[0], ring[1]);
    for w in ring.windows(2) {
        if target_deg >= w[0].0 && target_deg <= w[1].0 {
            pair = (w[0], w[1]);
            break;
        }
    }
    let (a, b) = pair;
    let t = if (b.0 - a.0).abs() < 1e-6 { 0.0 } else { (target_deg - a.0) / (b.0 - a.0) };
    // 恒功率：g_a²+g_b²=1
    let g_a = (t * std::f32::consts::FRAC_PI_2).cos();
    let g_b = (t * std::f32::consts::FRAC_PI_2).sin();

    let mut noise = Noise(seed);
    for i in 0..N {
        let s = noise.next() * amp;
        frame[a.1][i] += s * g_a;
        frame[b.1][i] += s * g_b;
    }
}

fn empty() -> Vec<Vec<f32>> {
    vec![vec![0.0f32; N]; 8]
}

/// 真值角归一到 (-180,180]，与识别输出一致，便于做环形差。
fn to_signed(deg: f32) -> f32 {
    let mut a = deg % 360.0;
    if a > 180.0 {
        a -= 360.0;
    }
    if a <= -180.0 {
        a += 360.0;
    }
    a
}

fn circ_err(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

fn main() {
    let ring = speaker_ring();
    let params = AnalyzerParams { smoothing: 0.0, ..Default::default() };

    println!("== 单源精度扫描（每 5°，恒功率平移真值）==");
    println!("{:>6} | {:>8} | {:>7}", "真值°", "识别°", "误差°");
    println!("{}", "-".repeat(30));

    let mut errs: Vec<f32> = Vec::new();
    let mut worst = (0.0f32, 0.0f32); // (真值, 误差绝对值)
    let mut step = 0;
    for tgt in (0..360).step_by(5) {
        let target = tgt as f32;
        let mut data = empty();
        pan_source_into(&mut data, target, 0.6, 0x1234_5678 + step, &ring);
        step += 1;
        let mut an = Analyzer::new(params);
        let payload = an.process(&AudioFrame::new(8, SR, data), 0);
        match payload.sources.first() {
            Some(s) => {
                let err = circ_err(s.angle, to_signed(target));
                errs.push(err.abs());
                if err.abs() > worst.1 {
                    worst = (target, err.abs());
                }
                // 仅打印部分行，避免刷屏
                if tgt % 30 == 0 {
                    println!("{:>6} | {:>8.1} | {:>+7.1}", tgt, s.angle, err);
                }
            }
            None => println!("{:>6} | {:>8} | {:>7}", tgt, "未检出", "-"),
        }
    }

    let n = errs.len() as f32;
    let mean = errs.iter().sum::<f32>() / n;
    let mut sorted = errs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    let max = sorted.last().copied().unwrap_or(0.0);
    let p90 = sorted[(sorted.len() as f32 * 0.9) as usize];
    println!("\n单源误差: 平均 {:.2}°, 中位 {:.2}°, P90 {:.2}°, 最大 {:.2}°", mean, median, p90, max);
    println!("(最大误差 {:.1}° 出现在真值 {:.0}° 附近 —— 通常是 7.1 背后 RL/RR 之间的大间隔)", worst.1, worst.0);

    println!("\n== 双源分离测试（一个固定在 -45°，另一个不同间隔）==");
    println!("{:>8} | {:>6} | {:>20}", "间隔°", "检出数", "识别角度°");
    println!("{}", "-".repeat(42));
    let fixed = 315.0; // = -45°
    for sep in [30.0, 45.0, 60.0, 90.0, 120.0, 180.0] {
        let other = (fixed + sep) % 360.0;
        let mut data = empty();
        pan_source_into(&mut data, fixed, 0.6, 0xABCD, &ring);
        pan_source_into(&mut data, other, 0.6, 0x1357, &ring);
        let mut an = Analyzer::new(params);
        let payload = an.process(&AudioFrame::new(8, SR, data), 0);
        let mut angles: Vec<f32> = payload.sources.iter().map(|s| s.angle).collect();
        angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let astr = angles.iter().map(|a| format!("{:.0}", a)).collect::<Vec<_>>().join(", ");
        println!("{:>8.0} | {:>6} | {:>20}", sep, payload.sources.len(), astr);
    }
    println!("\n真值: 源A=-45°, 源B=(-45+间隔)°  [同为宽带噪声=最坏情况]");

    println!("\n== 双源分离·不同频谱（仿脚步260Hz vs 枪声3500Hz）==");
    println!("{:>8} | {:>6} | {:>20}", "间隔°", "检出数", "识别角度°");
    println!("{}", "-".repeat(42));
    for sep in [20.0, 30.0, 45.0, 60.0, 90.0, 150.0] {
        let other = (fixed + sep) % 360.0;
        let mut data = empty();
        pan_tone_into(&mut data, fixed, 260.0, 0.6, &ring);
        pan_tone_into(&mut data, other, 3500.0, 0.6, &ring);
        let mut an = Analyzer::new(params);
        let payload = an.process(&AudioFrame::new(8, SR, data), 0);
        let mut angles: Vec<f32> = payload.sources.iter().map(|s| s.angle).collect();
        angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let astr = angles.iter().map(|a| format!("{:.0}", a)).collect::<Vec<_>>().join(", ");
        println!("{:>8.0} | {:>6} | {:>20}", sep, payload.sources.len(), astr);
    }
    println!("\n真值: 源A=-45°, 源B=(-45+间隔)°  [不同频谱=典型游戏场景]");
}

/// 把一个单频音按恒功率平移到 target 角度（用于"不同频谱"分离测试）。
fn pan_tone_into(frame: &mut [Vec<f32>], target_deg: f32, freq: f32, amp: f32, ring: &[(f32, usize)]) {
    let mut pair = (ring[0], ring[1]);
    for w in ring.windows(2) {
        if target_deg >= w[0].0 && target_deg <= w[1].0 {
            pair = (w[0], w[1]);
            break;
        }
    }
    let (a, b) = pair;
    let t = if (b.0 - a.0).abs() < 1e-6 { 0.0 } else { (target_deg - a.0) / (b.0 - a.0) };
    let g_a = (t * std::f32::consts::FRAC_PI_2).cos();
    let g_b = (t * std::f32::consts::FRAC_PI_2).sin();
    for i in 0..N {
        let s = (std::f32::consts::TAU * freq * i as f32 / SR as f32).sin() * amp;
        frame[a.1][i] += s * g_a;
        frame[b.1][i] += s * g_b;
    }
}
