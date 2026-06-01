//! 跨边界数据对象。这些类型是组件间的"接缝"，应保持稳定。

use serde::Serialize;

/// 一帧多声道音频。planar 布局：`data[channel][sample]`，样本归一化到 [-1, 1]。
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub channels: u16,
    pub sample_rate: u32,
    pub data: Vec<Vec<f32>>,
}

impl AudioFrame {
    pub fn new(channels: u16, sample_rate: u32, data: Vec<Vec<f32>>) -> Self {
        debug_assert_eq!(channels as usize, data.len(), "声道数与数据维度不一致");
        Self { channels, sample_rate, data }
    }

    /// 每声道样本数（假设各声道等长）。
    pub fn samples_per_channel(&self) -> usize {
        self.data.first().map_or(0, |c| c.len())
    }
}

/// 一个被定位出来的音源。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SourcePoint {
    /// 跨帧稳定 id，便于做拖尾/跟踪。
    pub id: u32,
    /// 方位角（度）。0 = 正前，顺时针为正，范围 (-180, 180]。
    pub angle: f32,
    /// 归一化强度 0~1。
    pub intensity: f32,
}

/// 每帧推送给前端的载荷（对应 design.md §7）。
#[derive(Debug, Clone, Serialize)]
pub struct FramePayload {
    /// 时间戳（毫秒）。
    pub ts: u64,
    pub sources: Vec<SourcePoint>,
    /// 8 声道原始能量，供调试视图（声道能量条）使用。
    pub channel_energies: Vec<f32>,
}

/// 7.1 各声道的标准物理角度（度）。索引对应 WAVE_FORMAT_EXTENSIBLE 常见顺序：
/// FL, FR, C, LFE, RL/BL, RR/BR, SL, SR。LFE 无方位，用 NaN 标记并在 DSP 中忽略。
pub const CHANNEL_ANGLES_71: [f32; 8] = [
    -45.0,  // FL 前左
    45.0,   // FR 前右
    0.0,    // C  中置
    f32::NAN, // LFE 低频（忽略）
    -135.0, // RL 后左
    135.0,  // RR 后右
    -90.0,  // SL 侧左
    90.0,   // SR 侧右
];
