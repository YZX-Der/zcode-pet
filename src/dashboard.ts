/** 主窗口 Dashboard 逻辑 */

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
  activate_targets: string[];
}

interface SessionInfo {
  session_id: string;
  state: string;
  effective_state: string;
  project: string | null;
  ts: number;
}

// ── Tab 切换 ────────────────────────────────────────────

function setupTabs(): void {
  const items = document.querySelectorAll<HTMLButtonElement>(".nav-item");
  const panes = document.querySelectorAll<HTMLElement>(".tab-pane");
  items.forEach((btn) => {
    btn.addEventListener("click", () => {
      items.forEach((b) => b.classList.remove("active"));
      panes.forEach((p) => p.classList.remove("active"));
      btn.classList.add("active");
      const tab = btn.dataset.tab!;
      document.getElementById(`tab-${tab}`)!.classList.add("active");
      if (tab === "sessions") refreshSessions();
    });
  });
}

// ── 设置面板 ────────────────────────────────────────────

let previewAnimator: SpriteAnimator | null = null;
let debounceTimer: number | null = null;

async function setupSettings(): Promise<void> {
  const settings = await invoke<Settings>("get_settings");
  const pets = await invoke<string[]>("list_pets");

  const scaleEl = document.getElementById("scale") as HTMLInputElement;
  const opacityEl = document.getElementById("opacity") as HTMLInputElement;
  const petSelect = document.getElementById("pet-select") as HTMLSelectElement;
  const topEl = document.getElementById("always-on-top") as HTMLInputElement;
  const targetsEl = document.getElementById("activate-targets") as HTMLInputElement;

  // 填充宠物下拉
  for (const name of pets) {
    const opt = document.createElement("option");
    opt.value = name;
    opt.textContent = name;
    if (name === settings.pet) opt.selected = true;
    petSelect.appendChild(opt);
  }

  scaleEl.value = String(settings.scale);
  opacityEl.value = String(settings.opacity);
  topEl.checked = settings.always_on_top;
  targetsEl.value = settings.activate_targets.join(", ");

  updateLabels();
  initPreview(settings.pet, settings.scale);

  // 滑块实时更新标签 + 预览 + 防抖保存
  scaleEl.addEventListener("input", () => {
    updateLabels();
    updatePreviewSize(parseFloat(scaleEl.value));
    debouncedSave();
  });
  opacityEl.addEventListener("input", () => {
    updateLabels();
    debouncedSave();
  });

  // 其他控件变更立即保存
  petSelect.addEventListener("change", async () => {
    await save();
    // 切换预览宠物
    previewAnimator?.destroy();
    previewAnimator = null;
    initPreview(petSelect.value, parseFloat(scaleEl.value));
  });
  topEl.addEventListener("change", () => debouncedSave());
  targetsEl.addEventListener("change", () => debouncedSave());
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

async function save(): Promise<void> {
  const scaleEl = document.getElementById("scale") as HTMLInputElement;
  const opacityEl = document.getElementById("opacity") as HTMLInputElement;
  const petSelect = document.getElementById("pet-select") as HTMLSelectElement;
  const topEl = document.getElementById("always-on-top") as HTMLInputElement;
  const targetsEl = document.getElementById("activate-targets") as HTMLInputElement;

  const newSettings: Settings = {
    scale: parseFloat(scaleEl.value),
    opacity: parseFloat(opacityEl.value),
    pet: petSelect.value,
    always_on_top: topEl.checked,
    activate_targets: targetsEl.value.split(",").map((s) => s.trim()).filter(Boolean),
  };

  try {
    await invoke("save_settings", { settings: newSettings });
  } catch (e) {
    console.error("save settings failed:", e);
  }
}

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
    previewAnimator = new SpriteAnimator(canvas, scale * 0.5);
    await previewAnimator.load(manifest, convertFileSrc(sheet_path));
  } catch (e) {
    console.error("preview init failed:", e);
  }
}

function updatePreviewSize(scale: number): void {
  const canvas = document.getElementById("preview-canvas") as HTMLCanvasElement;
  canvas.style.width = `${192 * scale * 0.5}px`;
  canvas.style.height = `${208 * scale * 0.5}px`;
}

// ── 会话列表 ────────────────────────────────────────────

const STATE_COLORS: Record<string, string> = {
  running: "#4a9eff",
  needs_input: "#ff9f1c",
  ready: "#40c057",
  blocked: "#e03131",
  idle: "#94a3b8",
  sleep: "#9775fa",
};

const STATE_LABELS: Record<string, string> = {
  running: "🔵 Running",
  needs_input: "🟡 Needs Input",
  ready: "🟢 Ready",
  blocked: "🔴 Blocked",
  idle: "⚪ Idle",
  sleep: "😴 Sleep",
};

async function refreshSessions(): Promise<void> {
  const container = document.getElementById("session-list")!;
  try {
    const sessions = await invoke<SessionInfo[]>("list_sessions");
    if (sessions.length === 0) {
      container.innerHTML = '<p class="empty-hint">暂无活跃会话。打开一个 ZCode 会话发条消息试试。</p>';
      return;
    }
    container.innerHTML = sessions.map((s) => {
      const state = s.effective_state || s.state;
      const color = STATE_COLORS[state] || "#94a3b8";
      const label = STATE_LABELS[state] || state;
      const shortId = s.session_id.length > 24
        ? s.session_id.slice(0, 12) + "…" + s.session_id.slice(-6)
        : s.session_id;
      const project = s.project || "（未指定项目）";
      return `
        <div class="session-card">
          <div class="session-state-dot" style="background:${color}"></div>
          <div class="session-info">
            <div class="session-id">${shortId}</div>
            <div class="session-meta">${project}</div>
          </div>
          <span class="session-state-tag" style="background:${color}22;color:${color}">${label}</span>
        </div>`;
    }).join("");
  } catch (e) {
    container.innerHTML = `<p class="empty-hint">加载失败: ${e}</p>`;
  }
}

// ── 启动 ────────────────────────────────────────────────

async function main(): Promise<void> {
  setupTabs();
  await setupSettings();

  // 监听状态变化，会话页打开时自动刷新
  await listen("pet://state-changed", () => {
    if (document.getElementById("tab-sessions")?.classList.contains("active")) {
      void refreshSessions();
    }
  });

  // 前端加载完成，通知后端激活并显示主窗口
  await invoke("frontend_ready").catch(() => {});
}

main().catch((err) => console.error("dashboard error:", err));
