// 声源选择：列出可选设备、可手动刷新、切换时调用后端 start_capture，并持久化。
import { invoke } from "@tauri-apps/api/core";
import { getSettings, updateSettings } from "./settings";
import { showToast } from "./toast";

interface DeviceInfo {
  id: string;
  name: string;
  channels: number;
}

export class DeviceSelector {
  constructor(
    private select: HTMLSelectElement,
    private statusEl: HTMLElement,
    refreshBtn: HTMLElement,
  ) {
    void this.refresh(true);
    this.select.addEventListener("change", () => void this.onChange());
    refreshBtn.addEventListener("click", () => void this.refresh(false));
  }

  /** 重新枚举设备。initial=true 时恢复上次所选并启动采集；手动刷新则保留当前选择、不重启采集。 */
  private async refresh(initial: boolean) {
    const keep = initial ? getSettings().deviceId : this.select.value;
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

      const found = keep && devices.some((d) => d.id === keep);
      if (found) {
        this.select.value = keep!;
        if (initial) await this.onChange(); // 初次：切到上次设备
      }
      this.renderStatus();

      if (!initial) {
        showToast(`已刷新：发现 ${devices.length} 个声源`, "success");
        if (keep && !found) {
          showToast("上次的设备已不在，请重新选择", "info");
        }
      }
    } catch (e) {
      showToast(`获取设备列表失败：${e}`, "error");
    }
  }

  private async onChange() {
    this.renderStatus();
    const deviceId = this.select.value;
    updateSettings({ deviceId });
    try {
      await invoke("start_capture", { device_id: deviceId });
    } catch (e) {
      showToast(`切换声源失败：${e}`, "error");
    }
  }

  /** 按声道数提示方位完整度。 */
  private renderStatus() {
    const opt = this.select.selectedOptions[0];
    const ch = Number(opt?.dataset.channels ?? 0);
    const id = this.select.value;
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
      text = "立体声：可判左右，前后不可靠（受限于 2 声道）";
      cls = "info";
    } else {
      text = `⚠ ${ch}ch：方位信息不足`;
      cls = "warn";
    }
    this.statusEl.textContent = text;
    this.statusEl.className = `device-status ${cls}`;
  }
}
