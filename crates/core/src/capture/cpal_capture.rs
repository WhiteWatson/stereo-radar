//! [`CpalCapture`]：跨平台从一个音频**输入设备**采集（[`cpal`]）。
//!
//! 用途：抓虚拟声卡的录音端 —— macOS 的 BlackHole、Windows 的 VoiceMeeter Out 等。
//! 把目标应用路由到该虚拟设备后，这里读到的就是该应用的（多）声道音频。
//!
//! 注意：cpal 的 `Stream` 在 macOS 上不是 `Send`，不能跨线程移动。因此这里
//! 在一个**独立线程内**创建、持有并最终丢弃 `Stream`，对外只暴露 Send 的句柄。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::{AudioCapture, DeviceInfo};
use crate::model::AudioFrame;

pub struct CpalCapture {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Default for CpalCapture {
    fn default() -> Self {
        Self { running: Arc::new(AtomicBool::new(false)), handle: None }
    }
}

impl CpalCapture {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AudioCapture for CpalCapture {
    fn list_devices() -> Vec<DeviceInfo> {
        let host = cpal::default_host();
        let mut out = Vec::new();
        if let Ok(devs) = host.input_devices() {
            for d in devs {
                let name = d.name().unwrap_or_else(|_| "未知输入设备".into());
                let ch = d.default_input_config().map(|c| c.channels()).unwrap_or(0);
                out.push(DeviceInfo { id: name.clone(), name, channels: ch });
            }
        }
        out
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
        let device_id = device_id.to_string();
        // 用于把"流是否成功建立"的结果同步回 start() 调用方。
        let (tx, rx) = mpsc::channel::<Result<(), String>>();

        let handle = thread::spawn(move || {
            let host = cpal::default_host();
            let device = host
                .input_devices()
                .ok()
                .and_then(|mut ds| ds.find(|d| d.name().map(|n| n == device_id).unwrap_or(false)));
            let device = match device {
                Some(d) => d,
                None => {
                    let _ = tx.send(Err(format!("找不到输入设备: {device_id}")));
                    return;
                }
            };
            let config = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(format!("读取设备配置失败: {e}")));
                    return;
                }
            };
            let sr = config.sample_rate().0;
            let channels = config.channels() as usize;
            let sample_format = config.sample_format();
            let stream_config: cpal::StreamConfig = config.into();

            // 把交错样本拆成 planar AudioFrame 并回调。
            macro_rules! build_stream {
                ($t:ty, $conv:expr) => {{
                    let conv = $conv;
                    let cb = move |data: &[$t], _: &cpal::InputCallbackInfo| {
                        if data.is_empty() || channels == 0 {
                            return;
                        }
                        let frames = data.len() / channels;
                        let mut planar: Vec<Vec<f32>> =
                            (0..channels).map(|_| Vec::with_capacity(frames)).collect();
                        for f in 0..frames {
                            for c in 0..channels {
                                planar[c].push(conv(data[f * channels + c]));
                            }
                        }
                        on_frame(AudioFrame::new(channels as u16, sr, planar));
                    };
                    device.build_input_stream(
                        &stream_config,
                        cb,
                        move |e| eprintln!("cpal 流错误: {e}"),
                        None,
                    )
                }};
            }

            let stream = match sample_format {
                cpal::SampleFormat::F32 => build_stream!(f32, |x: f32| x),
                cpal::SampleFormat::I16 => build_stream!(i16, |x: i16| x as f32 / 32768.0),
                cpal::SampleFormat::U16 => build_stream!(u16, |x: u16| (x as f32 - 32768.0) / 32768.0),
                other => {
                    let _ = tx.send(Err(format!("不支持的采样格式: {other:?}")));
                    return;
                }
            };
            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(format!("创建输入流失败: {e}")));
                    return;
                }
            };
            if let Err(e) = stream.play() {
                let _ = tx.send(Err(format!("启动流失败: {e}")));
                return;
            }
            let _ = tx.send(Ok(()));

            // 持有 stream 直到收到停止信号；随后在本线程内 drop。
            while running.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(50));
            }
            drop(stream);
        });

        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                self.handle = Some(handle);
                Ok(())
            }
            Ok(Err(e)) => {
                self.running.store(false, Ordering::SeqCst);
                Err(e)
            }
            Err(_) => {
                self.running.store(false, Ordering::SeqCst);
                Err("启动采集超时".into())
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
