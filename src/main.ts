import { listen } from "@tauri-apps/api/event";
import { Radar, ChannelBars, type SourcePoint } from "./radar";
import { ControlPanel } from "./panel";
import { DeviceSelector } from "./device";
import { Overlay } from "./overlay";

interface FramePayload {
  ts: number;
  sources: SourcePoint[];
  channel_energies: number[];
}

const canvas = document.getElementById("radar") as HTMLCanvasElement;
const barsEl = document.getElementById("bars") as HTMLElement;
const panelEl = document.getElementById("panel") as HTMLElement;

const deviceSel = document.getElementById("device") as HTMLSelectElement;
const deviceCh = document.getElementById("device-ch") as HTMLElement;

const radar = new Radar(canvas);
const bars = new ChannelBars(barsEl);
new ControlPanel(panelEl);
new DeviceSelector(deviceSel, deviceCh);

// Overlay 行为：锁定/穿透、显隐、热键、透明度。
new Overlay(
  document.getElementById("hint") as HTMLElement,
  document.getElementById("lock-btn") as HTMLElement,
  document.getElementById("opacity") as HTMLInputElement,
  document.getElementById("app") as HTMLElement,
);

// 后端推送与渲染解耦：事件只更新最新帧，rAF 负责绘制（带背压，丢旧帧）。
let latest: FramePayload | null = null;

listen<FramePayload>("sources", (event) => {
  latest = event.payload;
});

function loop() {
  if (latest) {
    radar.render(latest.sources);
    bars.update(latest.channel_energies);
  }
  requestAnimationFrame(loop);
}

// 首帧画个空网格，避免黑屏。
radar.render([]);
requestAnimationFrame(loop);
