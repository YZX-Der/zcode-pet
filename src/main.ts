import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { convertFileSrc } from "@tauri-apps/api/core";
import { SpriteAnimator } from "./animator";
import { loadManifest } from "./compat";
import type { PetStateName, StatePayload } from "./protocol";

const DRAG_THRESHOLD = 5; // 像素，小于此位移视为点击

async function main(): Promise<void> {
  // 参数由后端通过 window.__PET_INIT__ 注入（避免 URL 查询字符串的路径转义问题）
  const init = (window as any).__PET_INIT__ as {
    pet: string;
    state: PetStateName;
    sheet: string;
  } | undefined;

  const sheetPath = init?.sheet || "";
  const initialState = (init?.state || "idle") as PetStateName;

  // 加载宠物清单（自动检测 zcode-pet 原生格式或 Codex 兼容格式）
  const manifestUrl = convertFileSrc(
    sheetPath.replace(/spritesheet\.\w+$/, "pet.json"),
  );
  const res = await fetch(manifestUrl);
  const raw = await res.json() as Record<string, unknown>;
  const sheetFileName = sheetPath.split("/").pop() || "spritesheet.webp";
  const manifest = loadManifest(raw, sheetFileName);

  const canvas = document.getElementById("pet") as HTMLCanvasElement;
  const animator = new SpriteAnimator(canvas);
  await animator.load(manifest, convertFileSrc(sheetPath));
  animator.setState(initialState);

  // 监听后端状态更新
  await listen<StatePayload>("pet://state", (event) => {
    animator.setState(event.payload.state);
  });

  setupInteraction(canvas);
}

/** 区分点击（激活目标应用）与拖拽（移动窗口）。 */
function setupInteraction(canvas: HTMLCanvasElement): void {
  let downPos: { x: number; y: number } | null = null;
  let dragging = false;

  canvas.addEventListener("mousedown", (e) => {
    downPos = { x: e.screenX, y: e.screenY };
    dragging = false;
  });

  canvas.addEventListener("mousemove", async (e) => {
    if (!downPos) return;
    const dx = Math.abs(e.screenX - downPos.x);
    const dy = Math.abs(e.screenY - downPos.y);
    if (!dragging && (dx > DRAG_THRESHOLD || dy > DRAG_THRESHOLD)) {
      dragging = true;
      await getCurrentWindow().startDragging();
    }
  });

  canvas.addEventListener("mouseup", async () => {
    if (downPos && !dragging) {
      await invoke("activate_target").catch(() => {});
    }
    downPos = null;
    dragging = false;
  });
}

main().catch((err) => console.error("zcode-pet frontend error:", err));
