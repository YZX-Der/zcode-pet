/** 主窗口 Dashboard 逻辑 — macOS 26 Liquid Glass */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { SpriteAnimator } from "./animator";
import { loadManifest } from "./compat";
import type { PetManifest } from "./protocol";

interface Settings {
  scale: number;
  opacity: number;
  pet: string;
  always_on_top: boolean;
  pet_hidden: boolean;
  bubble_enabled: boolean;
  bubble_seconds: number;
}

interface SessionInfo {
  session_id: string;
  state: string;
  effective_state: string;
  project: string | null;
  title: string;
  is_current: boolean;
  ts: number;
}

// ── Tab 切换（带淡入淡出动画）─────────────────────────────

function setupTabs(): void {
  const items = document.querySelectorAll<HTMLButtonElement>(".nav-item");
  const panes = document.querySelectorAll<HTMLElement>(".tab-pane");
  items.forEach((btn) => {
    btn.addEventListener("click", () => {
      items.forEach((b) => b.classList.remove("active"));
      panes.forEach((p) => p.classList.remove("active"));
      btn.classList.add("active");
      const tab = btn.dataset.tab!;
      const pane = document.getElementById(`tab-${tab}`)!;
      // 强制 reflow 以重新触发入场动画
      void pane.offsetWidth;
      pane.classList.add("active");
      if (tab === "sessions") void refreshSessions();
    });
  });
}

// ── 滑块进度条同步 ────────────────────────────────────────

function updateSliderProgress(input: HTMLInputElement): void {
  const wrap = input.closest(".slider-wrap") as HTMLElement;
  if (!wrap) return;
  const min = parseFloat(input.min);
  const max = parseFloat(input.max);
  const val = parseFloat(input.value);
  const pct = ((val - min) / (max - min)) * 100;
  wrap.style.setProperty("--progress", `${pct}%`);
}

// ── 设置面板 ────────────────────────────────────────────

let previewAnimator: SpriteAnimator | null = null;
let debounceTimer: number | null = null;
// 当前选中的宠物名（网格切换时更新）
let currentPet = "zbuddy";

async function setupSettings(): Promise<void> {
  const settings = await invoke<Settings>("get_settings");
  const pets = await invoke<string[]>("list_pets");

  const scaleEl = document.getElementById("scale") as HTMLInputElement;
  const opacityEl = document.getElementById("opacity") as HTMLInputElement;
  const petGrid = document.getElementById("pet-grid")!;
  const topEl = document.getElementById("always-on-top") as HTMLInputElement;
  const petVisibleEl = document.getElementById("pet-visible") as HTMLInputElement;
  const bubbleEnabledEl = document.getElementById("bubble-enabled") as HTMLInputElement;
  const bubbleSecondsWrap = document.getElementById("bubble-seconds-wrap")!;

  // 当前选中宠物（空则回退 zbuddy）
  let selectedPet = settings.pet || "zbuddy";
  if (!pets.includes(selectedPet)) selectedPet = pets[0] || "zbuddy";
  currentPet = selectedPet;

  // 构建宠物图片网格（当前选中排第一）
  await renderPetGrid(petGrid, pets, selectedPet);

  // 点击网格项切换宠物
  petGrid.addEventListener("click", async (e) => {
    const item = (e.target as HTMLElement).closest<HTMLElement>("[data-pet]");
    if (!item) return;
    const value = item.dataset.pet!;
    if (value === selectedPet) return;
    selectedPet = value;
    currentPet = value;
    // 重渲染网格（选中项排第一）
    await renderPetGrid(petGrid, pets, selectedPet);
    // 保存并更新预览
    previewAnimator?.destroy();
    previewAnimator = null;
    await initPreview(value, parseFloat(scaleEl.value));
    await save();
  });

  scaleEl.value = String(settings.scale);
  opacityEl.value = String(settings.opacity);
  topEl.checked = settings.always_on_top;
  petVisibleEl.checked = !settings.pet_hidden;
  bubbleEnabledEl.checked = settings.bubble_enabled;

  // 自定义下拉：气泡消失时长
  initCustomSelect(
    bubbleSecondsWrap,
    [
      { value: "2", label: "2 秒" },
      { value: "3", label: "3 秒" },
      { value: "5", label: "5 秒" },
      { value: "10", label: "10 秒" },
    ],
    String(settings.bubble_seconds),
    () => debouncedSave(),
  );

  updateLabels();
  updateSliderProgress(scaleEl);
  updateSliderProgress(opacityEl);
  initPreview(selectedPet, settings.scale);

  // 滑块实时更新
  scaleEl.addEventListener("input", () => {
    updateLabels();
    updateSliderProgress(scaleEl);
    updatePreviewSize(parseFloat(scaleEl.value));
    debouncedSave();
  });
  opacityEl.addEventListener("input", () => {
    updateLabels();
    updateSliderProgress(opacityEl);
    // 实时预览不透明度
    const previewCanvas = document.getElementById("preview-canvas") as HTMLCanvasElement;
    previewCanvas.style.opacity = opacityEl.value;
    debouncedSave();
  });

  topEl.addEventListener("change", () => debouncedSave());
  petVisibleEl.addEventListener("change", () => {
    void invoke("set_pet_visible", { visible: petVisibleEl.checked });
  });
  bubbleEnabledEl.addEventListener("change", () => debouncedSave());
}

function updateLabels(): void {
  const scaleEl = document.getElementById("scale") as HTMLInputElement;
  const opacityEl = document.getElementById("opacity") as HTMLInputElement;
  document.getElementById("scale-val")!.textContent =
    `${Math.round(parseFloat(scaleEl.value) * 100)}%`;
  document.getElementById("opacity-val")!.textContent =
    `${Math.round(parseFloat(opacityEl.value) * 100)}%`;
}

function debouncedSave(): void {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => { void save(); }, 300);
}

/** 初始化自定义下拉（液态玻璃风格）：选中值存 wrap.dataset.value */
function initCustomSelect(
  wrap: HTMLElement,
  options: { value: string; label: string }[],
  initial: string,
  onChange: (value: string) => void,
): void {
  const trigger = wrap.querySelector(".cs-trigger") as HTMLButtonElement;
  const valueEl = wrap.querySelector(".cs-value") as HTMLElement;
  const dropdown = wrap.querySelector(".cs-dropdown") as HTMLElement;

  const current = options.find((o) => o.value === initial) ?? options[0];
  wrap.dataset.value = current.value;
  valueEl.textContent = current.label;

  dropdown.innerHTML = options
    .map((o) => `<div class="cs-option${o.value === current.value ? " selected" : ""}" data-value="${o.value}">${o.label}</div>`)
    .join("");

  trigger.addEventListener("click", (e) => {
    e.stopPropagation();
    wrap.classList.toggle("open");
  });
  dropdown.addEventListener("click", (e) => {
    const opt = (e.target as HTMLElement).closest<HTMLElement>(".cs-option");
    if (!opt) return;
    const value = opt.dataset.value!;
    wrap.dataset.value = value;
    valueEl.textContent = opt.textContent ?? value;
    dropdown.querySelectorAll(".cs-option").forEach((o) => o.classList.remove("selected"));
    opt.classList.add("selected");
    wrap.classList.remove("open");
    onChange(value);
  });
  // 点击外部关闭
  document.addEventListener("click", () => wrap.classList.remove("open"));
}

async function save(): Promise<void> {
  const scaleEl = document.getElementById("scale") as HTMLInputElement;
  const opacityEl = document.getElementById("opacity") as HTMLInputElement;
  const topEl = document.getElementById("always-on-top") as HTMLInputElement;
  const bubbleEnabledEl = document.getElementById("bubble-enabled") as HTMLInputElement;
  const bubbleSecondsWrap = document.getElementById("bubble-seconds-wrap")!;

  // pet_hidden 不在此处保存（由 set_pet_visible 命令管理，避免覆盖）
  const newSettings: Omit<Settings, "pet_hidden"> = {
    scale: parseFloat(scaleEl.value),
    opacity: parseFloat(opacityEl.value),
    pet: currentPet,
    always_on_top: topEl.checked,
    bubble_enabled: bubbleEnabledEl.checked,
    bubble_seconds: parseFloat(bubbleSecondsWrap.dataset.value || "3"),
  };

  try {
    await invoke("save_settings", { settings: newSettings });
  } catch (e) {
    console.error("save settings failed:", e);
  }
}

// ── 宠物图片网格 ────────────────────────────────────────

/** 渲染宠物图片网格：列出所有宠物，选中项有激活样式 */
async function renderPetGrid(
  container: HTMLElement,
  pets: string[],
  selectedPet: string,
): Promise<void> {
  container.innerHTML = "";

  for (const name of pets) {
    const isSelected = name === selectedPet;
    const card = await createPetCard(name, isSelected);
    container.appendChild(card);
  }
}

/** 创建单个宠物卡片 */
async function createPetCard(name: string, isSelected: boolean): Promise<HTMLElement> {
  const card = document.createElement("div");
  card.className = `pet-card${isSelected ? " selected" : ""}`;
  card.dataset.pet = name;
  card.innerHTML = `
    <canvas class="pet-card-canvas"></canvas>
    <span class="pet-card-name">${name}</span>
    ${isSelected ? '<span class="pet-card-badge">当前</span>' : ""}
  `;

  // 渲染该宠物的 idle 动画缩略图
  try {
    const { sheet_path } = await invoke<{ sheet_path: string }>("get_pet_sheet", { petName: name });
    const manifestUrl = convertFileSrc(sheet_path.replace(/spritesheet\.\w+$/, "pet.json"));
    const res = await fetch(manifestUrl);
    const raw = await res.json() as Record<string, unknown>;
    const sheetFileName = sheet_path.split("/").pop() || "spritesheet.webp";
    const manifest: PetManifest = loadManifest(raw, sheetFileName);
    const canvas = card.querySelector("canvas")!;
    const anim = new SpriteAnimator(canvas, 0.35);
    await anim.load(manifest, convertFileSrc(sheet_path));
    anim.setState("idle");
  } catch {
    card.querySelector("canvas")?.replaceWith(Object.assign(document.createElement("span"), { textContent: "🐾" }));
  }

  return card;
}

/** 保存选中的宠物并应用到桌宠（复用 save，pet 已通过 currentPet 同步） */

// ── 宠物预览 ────────────────────────────────────────────

async function initPreview(petName: string, scale: number): Promise<void> {
  try {
    const { sheet_path } = await invoke<{ sheet_path: string }>("get_pet_sheet", { petName });
    const manifestUrl = convertFileSrc(sheet_path.replace(/spritesheet\.\w+$/, "pet.json"));
    const res = await fetch(manifestUrl);
    const raw = await res.json() as Record<string, unknown>;
    const sheetFileName = sheet_path.split("/").pop() || "spritesheet.webp";
    const manifest: PetManifest = loadManifest(raw, sheetFileName);

    const canvas = document.getElementById("preview-canvas") as HTMLCanvasElement;
    previewAnimator = new SpriteAnimator(canvas, scale * 0.8);
    await previewAnimator.load(manifest, convertFileSrc(sheet_path));
  } catch (e) {
    console.error("preview init failed:", e);
  }
}

function updatePreviewSize(scale: number): void {
  const canvas = document.getElementById("preview-canvas") as HTMLCanvasElement;
  canvas.style.width = `${192 * scale * 0.8}px`;
  canvas.style.height = `${208 * scale * 0.8}px`;
}

// ── 会话列表 ────────────────────────────────────────────

const STATE_COLORS: Record<string, string> = {
  running: "#5b7cfa",
  needs_input: "#f5a623",
  ready: "#34c759",
  blocked: "#ff3b30",
  idle: "#8e8e93",
  sleep: "#af52de",
};

const STATE_LABELS: Record<string, string> = {
  running: "Running",
  needs_input: "Needs Input",
  ready: "Ready",
  blocked: "Blocked",
  idle: "Idle",
  sleep: "Sleep",
};

async function refreshSessions(): Promise<void> {
  const container = document.getElementById("session-list")!;
  try {
    const sessions = await invoke<SessionInfo[]>("list_sessions");
    if (sessions.length === 0) {
      container.innerHTML = '<p class="empty-hint">暂无执行中的任务。提交任务后宠物会自动出现在屏幕右下角。</p>';
      return;
    }
    container.innerHTML = sessions.map((s, i) => {
      const state = s.effective_state || s.state;
      const color = STATE_COLORS[state] || "#8e8e93";
      const label = STATE_LABELS[state] || state;
      const shortId = s.session_id.length > 24
        ? s.session_id.slice(0, 12) + "…" + s.session_id.slice(-6)
        : s.session_id;
      const project = s.project || "";
      const title = s.title || shortId;
      const currentCls = s.is_current ? " is-current" : "";
      const currentBadge = s.is_current ? '<span class="session-current-badge">当前</span>' : "";
      return `
        <div class="session-card${currentCls}" style="animation-delay:${i * 0.06}s">
          <div class="session-state-dot" style="background:${color};box-shadow:0 0 8px ${color}"></div>
          <div class="session-info">
            <div class="session-title" title="${escapeHtml(title)}">${escapeHtml(title)}${currentBadge}</div>
            <div class="session-meta">${escapeHtml(shortId)}${project ? ` · ${escapeHtml(project)}` : ""}</div>
          </div>
          <span class="session-state-tag" style="background:${color}1a;color:${color}">${label}</span>
        </div>`;
    }).join("");
  } catch (e) {
    container.innerHTML = `<p class="empty-hint">加载失败: ${e}</p>`;
  }
}

/** 会话标题可能包含 markdown/特殊字符，转义后渲染 */
function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// ── 启动 ────────────────────────────────────────────────

async function main(): Promise<void> {
  setupTabs();
  await setupSettings();

  await listen("pet://state-changed", () => {
    if (document.getElementById("tab-sessions")?.classList.contains("active")) {
      void refreshSessions();
    }
  });

  // 会话页定时刷新，保持状态/当前会话标记最新
  setInterval(() => {
    if (document.getElementById("tab-sessions")?.classList.contains("active")) {
      void refreshSessions();
    }
  }, 5000);

  setupInstallButton();

  await invoke("frontend_ready").catch(() => {});
}

// ── Hooks 一键安装 ────────────────────────────────────────

async function setupInstallButton(): Promise<void> {
  const btn = document.getElementById("install-hooks-btn") as HTMLButtonElement;
  if (!btn) return;

  // 检查当前安装状态
  try {
    const installed = await invoke<boolean>("is_hooks_installed");
    if (installed) {
      btn.textContent = "✓ 已安装";
      btn.classList.add("installed");
      btn.disabled = true;
      return;
    }
  } catch {
    // 忽略，显示安装按钮
  }

  btn.addEventListener("click", async () => {
    btn.disabled = true;
    btn.textContent = "安装中...";
    try {
      await invoke("install_hooks");
      btn.textContent = "✓ 安装成功";
      btn.classList.add("installed");
    } catch (e) {
      btn.textContent = "安装失败，请重试";
      btn.disabled = false;
      console.error("install hooks failed:", e);
    }
  });
}

main().catch((err) => console.error("dashboard error:", err));
