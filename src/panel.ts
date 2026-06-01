// 控制面板：把滑杆接到后端 set_params 命令（控制通道，低频），并持久化。
import { invoke } from "@tauri-apps/api/core";
import { getSettings, updateSettings, type Settings } from "./settings";

type ParamKey = "sensitivity" | "smoothing" | "gate";

interface ParamDef {
  key: ParamKey;
  label: string;
  min: number;
  max: number;
  step: number;
}

const DEFS: ParamDef[] = [
  { key: "sensitivity", label: "灵敏度", min: 0.2, max: 4, step: 0.05 },
  { key: "smoothing", label: "平滑", min: 0, max: 0.95, step: 0.01 },
  { key: "gate", label: "门限", min: 0, max: 0.3, step: 0.005 },
];

export class ControlPanel {
  private params: Pick<Settings, ParamKey>;
  private pushTimer: number | undefined;

  constructor(container: HTMLElement) {
    const s = getSettings();
    this.params = { gate: s.gate, smoothing: s.smoothing, sensitivity: s.sensitivity };

    for (const def of DEFS) {
      const row = document.createElement("div");
      row.className = "ctl-row";

      const label = document.createElement("label");
      label.textContent = def.label;

      const input = document.createElement("input");
      input.type = "range";
      input.min = String(def.min);
      input.max = String(def.max);
      input.step = String(def.step);
      input.value = String(this.params[def.key]);

      const val = document.createElement("span");
      val.className = "ctl-val";
      val.textContent = this.params[def.key].toFixed(2);

      input.addEventListener("input", () => {
        const v = Number(input.value);
        this.params[def.key] = v;
        val.textContent = v.toFixed(2);
        updateSettings({ [def.key]: v });
        this.schedulePush();
      });

      row.append(label, input, val);
      container.appendChild(row);
    }

    // 用恢复的设置覆盖后端默认值。
    void this.push();
  }

  /** 节流推送，避免拖动滑杆时高频 IPC。 */
  private schedulePush() {
    if (this.pushTimer !== undefined) return;
    this.pushTimer = window.setTimeout(() => {
      this.pushTimer = undefined;
      void this.push();
    }, 60);
  }

  private push() {
    return invoke("set_params", { ...this.params }).catch((e) =>
      console.error("set_params 失败", e),
    );
  }
}
