// 360° 雷达渲染（纯 Canvas）。
// 角度约定：0° = 正前(上)，顺时针为正。屏幕坐标 x=cx+r·sin θ, y=cy−r·cos θ。

export interface SourcePoint {
  id: number;
  angle: number; // 度
  intensity: number; // 0~1
}

const CHANNEL_LABELS = ["FL", "FR", "C", "LFE", "RL", "RR", "SL", "SR"];

interface TrailPoint {
  x: number;
  y: number;
}

export class Radar {
  private ctx: CanvasRenderingContext2D;
  private cx: number;
  private cy: number;
  private radius: number;
  /** 每个音源 id 的近期位置历史，用于拖尾。 */
  private trails = new Map<number, TrailPoint[]>();
  private static readonly TRAIL_LEN = 18;

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext("2d")!;
    this.cx = canvas.width / 2;
    this.cy = canvas.height / 2;
    this.radius = Math.min(this.cx, this.cy) - 18;
  }

  private polar(angleDeg: number, r: number): [number, number] {
    const a = (angleDeg * Math.PI) / 180;
    return [this.cx + r * Math.sin(a), this.cy - r * Math.cos(a)];
  }

  private drawGrid() {
    const { ctx } = this;
    ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);

    // 同心圆
    ctx.strokeStyle = "#1e2638";
    ctx.lineWidth = 1;
    for (const f of [0.33, 0.66, 1]) {
      ctx.beginPath();
      ctx.arc(this.cx, this.cy, this.radius * f, 0, Math.PI * 2);
      ctx.stroke();
    }
    // 十字 + 斜向参考线
    ctx.strokeStyle = "#171f2e";
    for (let deg = 0; deg < 360; deg += 45) {
      const [x, y] = this.polar(deg, this.radius);
      ctx.beginPath();
      ctx.moveTo(this.cx, this.cy);
      ctx.lineTo(x, y);
      ctx.stroke();
    }
    // 方位文字
    ctx.fillStyle = "#3d4860";
    ctx.font = "11px system-ui";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    const marks: [string, number][] = [["前", 0], ["右", 90], ["后", 180], ["左", -90]];
    for (const [label, deg] of marks) {
      const [x, y] = this.polar(deg, this.radius + 10);
      ctx.fillText(label, x, y);
    }
    // 中心(玩家)
    ctx.fillStyle = "#4a86ff";
    ctx.beginPath();
    ctx.arc(this.cx, this.cy, 4, 0, Math.PI * 2);
    ctx.fill();
  }

  private drawSource(s: SourcePoint) {
    const { ctx } = this;
    const r = this.radius * (0.35 + 0.6 * s.intensity); // 强度→离心距离
    const [x, y] = this.polar(s.angle, r);
    const size = 6 + 14 * s.intensity;
    // 颜色：弱→黄(50°)，强→红(0°)
    const hue = 50 * (1 - s.intensity);
    const color = `hsl(${hue}, 95%, 55%)`;

    // 拖尾
    const trail = this.trails.get(s.id);
    if (trail && trail.length > 1) {
      for (let i = 1; i < trail.length; i++) {
        const a = i / trail.length;
        ctx.strokeStyle = `hsla(${hue}, 95%, 55%, ${a * 0.5})`;
        ctx.lineWidth = 1 + 2 * a;
        ctx.beginPath();
        ctx.moveTo(trail[i - 1].x, trail[i - 1].y);
        ctx.lineTo(trail[i].x, trail[i].y);
        ctx.stroke();
      }
    }

    // 光晕 + 核心
    const grad = ctx.createRadialGradient(x, y, 0, x, y, size);
    grad.addColorStop(0, color);
    grad.addColorStop(1, "transparent");
    ctx.fillStyle = grad;
    ctx.beginPath();
    ctx.arc(x, y, size, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.arc(x, y, 3, 0, Math.PI * 2);
    ctx.fill();
  }

  private updateTrails(sources: SourcePoint[]) {
    const alive = new Set<number>();
    for (const s of sources) {
      alive.add(s.id);
      const r = this.radius * (0.35 + 0.6 * s.intensity);
      const [x, y] = this.polar(s.angle, r);
      const trail = this.trails.get(s.id) ?? [];
      trail.push({ x, y });
      while (trail.length > Radar.TRAIL_LEN) trail.shift();
      this.trails.set(s.id, trail);
    }
    // 清理消失的音源轨迹
    for (const id of this.trails.keys()) {
      if (!alive.has(id)) this.trails.delete(id);
    }
  }

  render(sources: SourcePoint[]) {
    this.updateTrails(sources);
    this.drawGrid();
    for (const s of sources) this.drawSource(s);
  }
}

// 8 声道能量条（调试视图）。
export class ChannelBars {
  private bars: HTMLDivElement[] = [];

  constructor(container: HTMLElement) {
    for (const label of CHANNEL_LABELS) {
      const bar = document.createElement("div");
      bar.className = "bar";
      bar.style.height = "2px";
      const span = document.createElement("span");
      span.textContent = label;
      bar.appendChild(span);
      container.appendChild(bar);
      this.bars.push(bar);
    }
  }

  update(energies: number[]) {
    energies.forEach((e, i) => {
      const bar = this.bars[i];
      if (!bar) return;
      const h = Math.min(70, e * 600); // 经验缩放
      bar.style.height = `${Math.max(2, h)}px`;
      bar.style.background = h > 20 ? "#ff5a4a" : h > 6 ? "#e0b020" : "#1c2433";
    });
  }
}
