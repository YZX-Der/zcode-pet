/** 宠物浮窗逻辑 -- 加载 pet.html 时运行 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { convertFileSrc } from "@tauri-apps/api/core";
import { SpriteAnimator } from "./animator";
import { loadManifest } from "./compat";
import type { PetManifest, PetStateName, StatePayload } from "./protocol";

const DRAG_THRESHOLD = 2;
let animator: SpriteAnimator | null = null;
let unsubListener: (() => void) | null = null;
let interactionBound = false;
// 当前最新状态：pet://state 事件更新它；
// 热更新（updatePet）时优先用它，避免回退到 __PET_INIT__ 的初始快照
let currentState: PetStateName | null = null;
// 用户设置的基础不透明度（sleep 时在此基础上变淡）
let baseOpacity = 1;

// ── 状态气泡 ────────────────────────────────────────
const BUBBLE_TEXT: Record<string, string> = {
  idle: "空闲",
  running: "执行中",
  needs_input: "等待确认",
  ready: "已完成",
  blocked: "执行出错",
  sleep: "休眠中",
};
const BUBBLE_COLOR: Record<string, string> = {
  idle: "#8e8e93",
  running: "#5b7cfa",
  needs_input: "#f5a623",
  ready: "#34c759",
  blocked: "#ff3b30",
  sleep: "#af52de",
};
let lastBubbleState: string | null = null;
let bubbleTimer: number | null = null;
// 气泡配置（从设置读取）
let bubbleEnabled = true;
let bubbleSeconds = 3;

// 立即设置指针样式（不等异步加载完成，避免首次移入时仍是箭头）
(function setPetCursor() {
  const canvas = document.getElementById("pet") as HTMLCanvasElement | null;
  if (canvas) canvas.style.cursor = "pointer";
})();

function getInit() {
  return (window as any).__PET_INIT__ as {
    pet: string;
    state: PetStateName;
    sheet: string;
    scale?: number;
    opacity?: number;
  } | undefined;
}

/** 根据状态计算实际不透明度：sleep 变淡，其他恢复正常 */
function effectiveOpacity(state: PetStateName, opacity: number): number {
  return state === "sleep" ? opacity * 0.35 : opacity;
}

/** 读取气泡配置（设置页保存时 recreate_all 会重载窗口，配置自动刷新） */
async function loadBubbleConfig(): Promise<void> {
  try {
    const s = await invoke<{ bubble_enabled: boolean; bubble_seconds: number }>("get_settings");
    bubbleEnabled = s.bubble_enabled;
    bubbleSeconds = s.bubble_seconds;
  } catch {
    // 保持默认
  }
}

/** 显示气泡并按时长自动消失。
 *  force=true（鼠标移入触发）时无视状态去重，强制重新显示 + 重新计时 */
function showBubble(state: PetStateName, force = false): void {
  if (!bubbleEnabled) return;
  const bubble = document.getElementById("state-bubble");
  const text = document.getElementById("bubble-text");
  const dot = document.getElementById("bubble-dot");
  if (!bubble || !text || !dot) return;
  text.textContent = BUBBLE_TEXT[state] || state;
  dot.style.background = BUBBLE_COLOR[state] || "#8e8e93";

  // 状态变化去重（避免衰减定时器重复触发）；force 时无视去重
  if (!force && state === lastBubbleState) return;
  lastBubbleState = state;
  bubble.classList.add("visible");
  if (bubbleTimer) clearTimeout(bubbleTimer);
  bubbleTimer = window.setTimeout(
    () => bubble.classList.remove("visible"),
    bubbleSeconds * 1000,
  );
}

async function loadAndRender(): Promise<void> {
  const init = getInit();
  if (!init) {
    console.error("[zcode-pet] __PET_INIT__ not set");
    return;
  }

  // 读取气泡配置（设置变更后 recreate_all 会重载窗口，这里自动刷新）
  await loadBubbleConfig();

  const sheetPath = init.sheet;
  const scale = init.scale ?? 1;
  const opacity = init.opacity ?? 1;
  baseOpacity = opacity;

  // 加载 manifest
  const manifestUrl = convertFileSrc(sheetPath.replace(/spritesheet\.\w+$/, "pet.json"));
  const res = await fetch(manifestUrl);
  const raw = await res.json() as Record<string, unknown>;
  const sheetFileName = sheetPath.split("/").pop() || "spritesheet.webp";
  const manifest: PetManifest = loadManifest(raw, sheetFileName);

  // 获取 canvas（始终用同一个 DOM 元素，不 clone）
  const canvas = document.getElementById("pet") as HTMLCanvasElement;
  canvas.style.opacity = String(effectiveOpacity(currentState ?? init.state, opacity));
  // 气泡左缘贴宠物右侧 6px（left 定位，避免文字长短导致距离变化）
  const bubble = document.getElementById("state-bubble");
  if (bubble) {
    bubble.style.left = `${canvas.offsetWidth + 6}px`;
    bubble.style.right = "auto";
  }

  // 销毁旧 animator
  if (animator) {
    animator.destroy();
    animator = null;
  }

  // 创建新 animator（会重新设置 canvas 尺寸和 context）
  animator = new SpriteAnimator(canvas, scale);
  await animator.load(manifest, convertFileSrc(sheetPath));
  animator.setState(currentState ?? init.state);

  // 重新绑定状态监听
  if (unsubListener) {
    unsubListener();
    unsubListener = null;
  }
  unsubListener = await listen<StatePayload>("pet://state", (event) => {
    currentState = event.payload.state;
    animator?.setState(event.payload.state);
    // sleep 状态变淡，其他状态恢复用户设置的不透明度
    canvas.style.opacity = String(effectiveOpacity(event.payload.state, baseOpacity));
    // 状态变化闪现气泡
    showBubble(event.payload.state);
  });

  // 只绑一次交互事件
  if (!interactionBound) {
    setupInteraction(canvas);
    interactionBound = true;
  }
}

async function initPet(): Promise<void> {
  await loadAndRender();
  // 点击聚焦后立即显示气泡（macOS 聚焦前无 hover，聚焦后 mouseenter 不触发）
  await listen("pet://focused", () => {
    if (currentState) showBubble(currentState, true);
  }).catch(() => {});
}

/** 设置变更后热更新（不重新加载页面） */
async function updatePet(): Promise<void> {
  await loadAndRender();
}

function setupInteraction(canvas: HTMLCanvasElement): void {
  let downPos: { x: number; y: number } | null = null;
  let dragging = false;

  canvas.addEventListener("mousedown", (e) => {
    // 只处理左键（右键走 contextmenu 弹菜单，不触发拖拽/激活）
    if (e.button !== 0) return;
    downPos = { x: e.screenX, y: e.screenY };
    dragging = false;
  });

  canvas.addEventListener("mousemove", async (e) => {
    if (!downPos || dragging) return;
    const dx = Math.abs(e.screenX - downPos.x);
    const dy = Math.abs(e.screenY - downPos.y);
    if (dx <= DRAG_THRESHOLD && dy <= DRAG_THRESHOLD) return;
    // 系统级拖拽：窗口平滑跟随鼠标，无跳变（宠物固定在窗口左缘，
    // 不会被换边逻辑挤到屏幕外）
    dragging = true;
    await getCurrentWindow().startDragging().catch(() => {});
  });

  canvas.addEventListener("mouseup", async (e) => {
    if (e.button !== 0) return;
    if (downPos && !dragging) {
      await invoke("activate_target").catch(() => {});
    }
    downPos = null;
    dragging = false;
  });

  // 鼠标移入：气泡显示当前状态并按设定时长自动消失（可反复移入重新查看）
  canvas.addEventListener("mouseenter", () => {
    if (currentState) showBubble(currentState, true);
  });

  // 右键：弹出原生菜单（切换宠物 / 显示主窗口 / 退出），
  // 拦截默认的「重新载入/检查元素」调试菜单
  canvas.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    void invoke("show_pet_menu").catch(() => {});
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
