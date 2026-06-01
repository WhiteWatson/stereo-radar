// 声源选择：列出可选设备，切换时调用后端 start_capture。
import { invoke } from "@tauri-apps/api/core";

interface DeviceInfo {
  id: string;
  name: string;
  channels: number;
}

export class DeviceSelector {
  constructor(
    private select: HTMLSelectElement,
    private channelsLabel: HTMLElement,
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
      this.updateChannelLabel();
    } catch (e) {
      console.error("list_devices 失败", e);
    }
  }

  private updateChannelLabel() {
    const opt = this.select.selectedOptions[0];
    const ch = opt?.dataset.channels ?? "0";
    this.channelsLabel.textContent = `${ch}ch`;
  }

  private async onChange() {
    this.updateChannelLabel();
    const deviceId = this.select.value;
    try {
      await invoke("start_capture", { device_id: deviceId });
    } catch (e) {
      console.error("start_capture 失败", e);
      alert(`切换声源失败：${e}`);
    }
  }
}
