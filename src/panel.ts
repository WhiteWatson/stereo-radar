// 控制面板：把滑杆接到后端 set_params 命令（控制通道，低频）。
import { invoke } from "@tauri-apps/api/core";

interface ParamDef {
  key: "sensitivity" | "smoothing" | "gate";
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
  // 与后端 AnalyzerParams 默认值保持一致。
  private params = { gate: 0.05, smoothing: 0.5, sensitivity: 1.4 };
  private pushTimer: number | undefined;

  constructor(container: HTMLElement) {
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
        this.schedulePush();
      });

      row.append(label, input, val);
      container.appendChild(row);
    }
  }

  /** 节流推送，避免拖动滑杆时高频 IPC。 */
  private schedulePush() {
    if (this.pushTimer !== undefined) return;
    this.pushTimer = window.setTimeout(() => {
      this.pushTimer = undefined;
      void invoke("set_params", { ...this.params }).catch((e) =>
        console.error("set_params 失败", e),
      );
    }, 60);
  }
}
