//! stereo-radar 核心库（平台无关）。
//!
//! 架构见 ARCHITECTURE.md：
//! - [`capture`] 把"音频来源差异"关进 [`capture::AudioCapture`] trait 后面。
//! - [`dsp`] 把 [`model::AudioFrame`] 无状态地变换为 [`model::SourcePoint`]。
//! - [`model`] 是两者共享的边界对象，core 不依赖 Tauri / UI。

pub mod capture;
pub mod dsp;
pub mod model;

pub use capture::{AudioCapture, DeviceInfo};
pub use dsp::{Analyzer, AnalyzerParams};
pub use model::{AudioFrame, FramePayload, SourcePoint};
