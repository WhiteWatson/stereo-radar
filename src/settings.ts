// 设置持久化（localStorage；webview 数据目录跨重启保留）。
// 窗口位置/大小由 tauri-plugin-window-state 单独负责。

export interface Settings {
  gate: number;
  smoothing: number;
  sensitivity: number;
  deviceId: string | null;
  opacity: number;
}

// 与后端 AnalyzerParams 默认值保持一致。
const DEFAULTS: Settings = {
  gate: 0.05,
  smoothing: 0.5,
  sensitivity: 1.4,
  deviceId: null,
  opacity: 1,
};

const KEY = "stereo-radar-settings";

let current: Settings = load();

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? { ...DEFAULTS, ...JSON.parse(raw) } : { ...DEFAULTS };
  } catch {
    return { ...DEFAULTS };
  }
}

export function getSettings(): Readonly<Settings> {
  return current;
}

export function updateSettings(patch: Partial<Settings>): void {
  current = { ...current, ...patch };
  try {
    localStorage.setItem(KEY, JSON.stringify(current));
  } catch (e) {
    console.error("保存设置失败", e);
  }
}
