import { listen } from "@tauri-apps/api/event";
import { Radar, ChannelBars, type SourcePoint } from "./radar";
import { ControlPanel } from "./panel";

interface FramePayload {
  ts: number;
  sources: SourcePoint[];
  channel_energies: number[];
}

const canvas = document.getElementById("radar") as HTMLCanvasElement;
const barsEl = document.getElementById("bars") as HTMLElement;
const panelEl = document.getElementById("panel") as HTMLElement;

const radar = new Radar(canvas);
const bars = new ChannelBars(barsEl);
new ControlPanel(panelEl);

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
