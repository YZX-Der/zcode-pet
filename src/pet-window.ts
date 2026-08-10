/** 宠物浮窗逻辑 -- 加载 pet.html 时运行 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { convertFileSrc } from "@tauri-apps/api/core";
import { SpriteAnimator } from "./animator";
import { loadManifest } from "./compat";
import type { PetManifest, PetStateName, StatePayload } from "./protocol";

const DRAG_THRESHOLD = 5;
let animator: SpriteAnimator | null = null;
let unsubListener: (() => void) | null = null;
let interactionBound = false;

function getInit() {
  return (window as any).__PET_INIT__ as {
    pet: string;
    state: PetStateName;
    sheet: string;
    scale?: number;
    opacity?: number;
  } | undefined;
}

async function loadAndRender(): Promise<void> {
  const init = getInit();
  if (!init) {
    console.error("[zcode-pet] __PET_INIT__ not set");
    return;
  }

  const sheetPath = init.sheet;
  const scale = init.scale ?? 1;
  const opacity = init.opacity ?? 1;

  // 加载 manifest
  const manifestUrl = convertFileSrc(sheetPath.replace(/spritesheet\.\w+$/, "pet.json"));
  const res = await fetch(manifestUrl);
  const raw = await res.json() as Record<string, unknown>;
  const sheetFileName = sheetPath.split("/").pop() || "spritesheet.webp";
  const manifest: PetManifest = loadManifest(raw, sheetFileName);

  // 获取 canvas（始终用同一个 DOM 元素，不 clone）
  const canvas = document.getElementById("pet") as HTMLCanvasElement;
  canvas.style.opacity = String(opacity);

  // 销毁旧 animator
  if (animator) {
    animator.destroy();
    animator = null;
  }

  // 创建新 animator（会重新设置 canvas 尺寸和 context）
  animator = new SpriteAnimator(canvas, scale);
  await animator.load(manifest, convertFileSrc(sheetPath));
  animator.setState(init.state);

  // 重新绑定状态监听
  if (unsubListener) {
    unsubListener();
    unsubListener = null;
  }
  unsubListener = await listen<StatePayload>("pet://state", (event) => {
    animator?.setState(event.payload.state);
  });

  // 只绑一次交互事件
  if (!interactionBound) {
    setupInteraction(canvas);
    interactionBound = true;
  }
}

async function initPet(): Promise<void> {
  await loadAndRender();
}

/** 设置变更后热更新（不重新加载页面） */
async function updatePet(): Promise<void> {
  await loadAndRender();
}

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

// 暴露给 Rust 调用
(window as any).initPet = initPet;
(window as any).updatePet = updatePet;

// 自动初始化：等待 __PET_INIT__ 可用后执行
function autoInit(): void {
  if ((window as any).__PET_INIT__) {
    initPet().catch((err) => console.error("[zcode-pet] init error:", err));
  } else {
    // Rust 的 on_page_load 回调会注入 __PET_INIT__ 并调用 initPet()
    // 这里作为后备：如果 500ms 后还没初始化，重试
    setTimeout(autoInit, 500);
  }
}
autoInit();
