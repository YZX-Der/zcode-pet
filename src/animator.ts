import type { PetManifest, PetStateName } from "./protocol";

/** Canvas 精灵表帧动画播放器。 */
export class SpriteAnimator {
  private ctx: CanvasRenderingContext2D;
  private sheet: HTMLImageElement | null = null;
  private manifest: PetManifest | null = null;
  private state: PetStateName = "idle";
  private frame = 0;
  private lastFrameTime = 0;
  private rafId = 0;
  private scale: number;

  constructor(canvas: HTMLCanvasElement, scale = 1) {
    this.ctx = canvas.getContext("2d")!;
    this.ctx.imageSmoothingEnabled = false;
    this.scale = scale;
  }

  async load(manifest: PetManifest, sheetUrl: string): Promise<void> {
    this.manifest = manifest;
    const [fw, fh] = manifest.frame;
    // Canvas 内部分辨率保持精灵表原始尺寸（清晰像素）
    this.ctx.canvas.width = fw;
    this.ctx.canvas.height = fh;
    // 通过 CSS 缩放到窗口实际尺寸
    this.ctx.canvas.style.width = `${fw * this.scale}px`;
    this.ctx.canvas.style.height = `${fh * this.scale}px`;
    this.sheet = await this.loadImage(sheetUrl);
    this.start();
  }

  setState(state: PetStateName): void {
    if (this.state !== state) {
      this.state = state;
      this.frame = 0;
    }
  }

  private start(): void {
    const loop = (time: number) => {
      this.tick(time);
      this.rafId = requestAnimationFrame(loop);
    };
    this.rafId = requestAnimationFrame(loop);
  }

  private tick(time: number): void {
    if (!this.sheet || !this.manifest) return;
    const spec = this.manifest.states[this.state];
    if (!spec) return;

    const [fw, fh] = this.manifest.frame;
    const interval = 1000 / spec.fps;
    if (time - this.lastFrameTime >= interval) {
      this.frame = (this.frame + 1) % spec.frames;
      this.lastFrameTime = time;
    }

    const col = this.frame % this.manifest.cols;
    const row = spec.row;
    this.ctx.clearRect(0, 0, fw, fh);
    this.ctx.drawImage(
      this.sheet,
      col * fw, row * fh, fw, fh,
      0, 0, fw, fh,
    );
  }

  private loadImage(url: string): Promise<HTMLImageElement> {
    return new Promise((resolve, reject) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = () => reject(new Error(`failed to load ${url}`));
      img.src = url;
    });
  }

  destroy(): void {
    cancelAnimationFrame(this.rafId);
  }
}
