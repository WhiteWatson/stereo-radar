// 轻量 toast 提示，替代会阻塞/在窄窗口点不到的 alert。
// 非阻塞、可复制、可手动关闭；错误默认常驻直到关闭。

export type ToastType = "info" | "error" | "success";

let container: HTMLElement | null = null;

function ensureContainer(): HTMLElement {
  if (!container) {
    container = document.createElement("div");
    container.id = "toast-container";
    document.body.appendChild(container);
  }
  return container;
}

function dismiss(el: HTMLElement) {
  el.classList.add("hide");
  setTimeout(() => el.remove(), 180);
}

export function showToast(
  message: string,
  type: ToastType = "info",
  opts: { duration?: number; copyable?: boolean } = {},
): void {
  const c = ensureContainer();
  const el = document.createElement("div");
  el.className = `toast ${type}`;

  const msg = document.createElement("span");
  msg.className = "toast-msg";
  msg.textContent = message;
  el.appendChild(msg);

  // 错误默认提供「复制」按钮（解决报错无法复制的问题）。
  if (opts.copyable ?? type === "error") {
    const copy = document.createElement("button");
    copy.className = "toast-btn";
    copy.textContent = "复制";
    copy.onclick = () => {
      navigator.clipboard
        .writeText(message)
        .then(() => (copy.textContent = "已复制"))
        .catch(() => (copy.textContent = "复制失败"));
    };
    el.appendChild(copy);
  }

  const close = document.createElement("button");
  close.className = "toast-btn close";
  close.textContent = "✕";
  close.onclick = () => dismiss(el);
  el.appendChild(close);

  c.appendChild(el);

  // 错误常驻（duration=0），其余自动消失。
  const duration = opts.duration ?? (type === "error" ? 0 : 3000);
  if (duration > 0) setTimeout(() => dismiss(el), duration);
}
