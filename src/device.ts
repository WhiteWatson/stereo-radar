// 声源选择：列出可选设备，切换时调用后端 start_capture，并持久化所选。
import { invoke } from "@tauri-apps/api/core";
import { getSettings, updateSettings } from "./settings";

interface DeviceInfo {
  id: string;
  name: string;
  channels: number;
}

export class DeviceSelector {
  constructor(
    private select: HTMLSelectElement,
    private channelsLabel: HTMLElement,
    private statusEl: HTMLElement,
  ) {
    void this.refresh();
    this.select.addEventListener("change", () => void this.onChange());
  }

  private async refresh() {
    try {
      const devices = await invoke<DeviceInfo[]>("list_devices");
      this.select.innerHTML = "";
      for (const d of devices) {
        const opt = document.createElement("option");
        opt.value = d.id;
        opt.textContent = d.channels > 0 ? `${d.name} (${d.channels}ch)` : d.name;
        opt.dataset.channels = String(d.channels);
        this.select.appendChild(opt);
      }

      // 恢复上次选择的设备（若仍存在），并切换到它。
      const saved = getSettings().deviceId;
      if (saved && devices.some((d) => d.id === saved)) {
        this.select.value = saved;
        await this.onChange();
      }
      this.updateChannelLabel();
    } catch (e) {
      console.error("list_devices 失败", e);
    }
  }

  private updateChannelLabel() {
    const opt = this.select.selectedOptions[0];
    const ch = opt?.dataset.channels ?? "0";
    this.channelsLabel.textContent = `${ch}ch`;
    this.renderStatus(this.select.value, Number(ch));
  }

  /** 按声道数提示方位完整度，引导用户配置 7.1。 */
  private renderStatus(id: string, ch: number) {
    let text: string;
    let cls: "ok" | "warn" | "info";
    if (id === "synthetic-orbit") {
      text = "● 合成测试源（占位，非真实声音）";
      cls = "info";
    } else if (ch >= 8) {
      text = "✓ 7.1 环绕：方位完整（360°）";
      cls = "ok";
    } else if (ch >= 6) {
      text = `✓ ${ch}ch 多声道：方位基本完整`;
      cls = "ok";
    } else if (ch === 2) {
      text = "⚠ 立体声：只有左右方位。要完整 360°，请把游戏路由到 7.1 虚拟声卡 ↓";
      cls = "warn";
    } else {
      text = `⚠ ${ch}ch：方位信息不足，建议改用 7.1 虚拟声卡 ↓`;
      cls = "warn";
    }
    this.statusEl.textContent = text;
    this.statusEl.className = `device-status ${cls}`;
  }

  private async onChange() {
    this.updateChannelLabel();
    const deviceId = this.select.value;
    updateSettings({ deviceId });
    try {
      await invoke("start_capture", { device_id: deviceId });
    } catch (e) {
      console.error("start_capture 失败", e);
      alert(`切换声源失败：${e}`);
    }
  }
}
