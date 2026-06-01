// Overlay 行为：锁定(点击穿透)/解锁(可交互)、显隐、全局热键、透明度。
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { register } from "@tauri-apps/plugin-global-shortcut";

const win = getCurrentWindow();

const HOTKEY_LOCK = "CommandOrControl+Shift+L";
const HOTKEY_SHOW = "CommandOrControl+Shift+H";

export class Overlay {
  private locked = true;

  constructor(
    private hintEl: HTMLElement,
    lockBtn: HTMLElement,
    opacity: HTMLInputElement,
    private appEl: HTMLElement,
  ) {
    // 解锁状态下点「锁定」按钮回到 overlay。
    lockBtn.addEventListener("click", () => void this.setLocked(true));
    opacity.addEventListener("input", () => {
      this.appEl.style.opacity = opacity.value;
    });
    void this.init();
  }

  private async init() {
    await this.setLocked(true); // 默认即 overlay：穿透 + 极简
    try {
      await register(HOTKEY_LOCK, (e) => {
        if (e.state === "Pressed") void this.toggleLock();
      });
      await register(HOTKEY_SHOW, (e) => {
        if (e.state === "Pressed") void this.toggleShow();
      });
    } catch (e) {
      console.error("注册全局热键失败", e);
    }

    // 托盘菜单事件，复用同一套切换逻辑以保持状态一致。
    await listen("menu-toggle-lock", () => void this.toggleLock());
    await listen("menu-toggle-show", () => void this.toggleShow());
  }

  /** 锁定=点击穿透+隐藏控件（纯 overlay）；解锁=可交互+显示控件。 */
  private async setLocked(v: boolean) {
    this.locked = v;
    try {
      await win.setIgnoreCursorEvents(v);
    } catch (e) {
      console.error("setIgnoreCursorEvents 失败", e);
    }
    document.body.classList.toggle("locked", v);
    this.hintEl.textContent = v
      ? "⌘⇧L 解锁 · ⌘⇧H 显隐"
      : "拖顶栏移动 · ⌘⇧L 锁回穿透 overlay";
  }

  private toggleLock() {
    return this.setLocked(!this.locked);
  }

  private async toggleShow() {
    const vis = await win.isVisible();
    if (vis) {
      await win.hide();
    } else {
      await win.show();
      await win.setFocus();
    }
  }
}
