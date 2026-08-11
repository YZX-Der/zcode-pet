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
  activate_targets: string[];
}

interface SessionInfo {
  session_id: string;
  state: string;
  effective_state: string;
  project: string | null;
  title: string;
  pet_enabled: boolean;
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

async function setupSettings(): Promise<void> {
  const settings = await invoke<Settings>("get_settings");
  const pets = await invoke<string[]>("list_pets");

  const scaleEl = document.getElementById("scale") as HTMLInputElement;
  const opacityEl = document.getElementById("opacity") as HTMLInputElement;
  const petSelect = document.getElementById("pet-select") as HTMLSelectElement;
  const csWrap = document.getElementById("pet-select-wrap")!;
  const csTrigger = csWrap.querySelector(".cs-trigger") as HTMLButtonElement;
  const csValue = csWrap.querySelector(".cs-value") as HTMLElement;
  const csDropdown = csWrap.querySelector(".cs-dropdown") as HTMLElement;
  const topEl = document.getElementById("always-on-top") as HTMLInputElement;
  const targetsEl = document.getElementById("activate-targets") as HTMLInputElement;

  // 构建自定义下拉选项
  csDropdown.innerHTML = pets.map((name) =>
    `<div class="cs-option ${name === settings.pet ? "selected" : ""}" data-value="${name}">
      <span>${name}</span>
      <span class="cs-check">✓</span>
    </div>`
  ).join("");
  csValue.textContent = settings.pet;
  petSelect.value = settings.pet;

  // 点击触发器切换下拉
  csTrigger.addEventListener("click", (e) => {
    e.stopPropagation();
    csWrap.classList.toggle("open");
  });

  // 点击选项
  csDropdown.addEventListener("click", (e) => {
    const opt = (e.target as HTMLElement).closest(".cs-option") as HTMLElement;
    if (!opt) return;
    const value = opt.dataset.value!;
    csValue.textContent = value;
    petSelect.value = value;
    csDropdown.querySelectorAll(".cs-option").forEach((o) => o.classList.remove("selected"));
    opt.classList.add("selected");
    csWrap.classList.remove("open");
    void save();
    previewAnimator?.destroy();
    previewAnimator = null;
    initPreview(value, parseFloat(scaleEl.value));
  });

  // 点击外部关闭下拉
  document.addEventListener("click", () => csWrap.classList.remove("open"));

  scaleEl.value = String(settings.scale);
  opacityEl.value = String(settings.opacity);
  topEl.checked = settings.always_on_top;
  targetsEl.value = settings.activate_targets.join(", ");

  updateLabels();
  updateSliderProgress(scaleEl);
  updateSliderProgress(opacityEl);
  initPreview(settings.pet, settings.scale);

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
    debouncedSave();
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
      const petCls = s.pet_enabled ? "on" : "off";
      const currentCls = s.is_current ? " is-current" : "";
      const currentBadge = s.is_current ? '<span class="session-current-badge">当前</span>' : "";
      return `
        <div class="session-card${currentCls}" style="animation-delay:${i * 0.06}s">
          <div class="session-state-dot" style="background:${color};box-shadow:0 0 8px ${color}"></div>
          <div class="session-info">
            <div class="session-title" title="${escapeHtml(title)}">${escapeHtml(title)}${currentBadge}</div>
            <div class="session-meta">${escapeHtml(shortId)}${project ? ` · ${escapeHtml(project)}` : ""}</div>
          </div>
          <div class="session-actions">
            <span class="session-state-tag" style="background:${color}1a;color:${color}">${label}</span>
            <button class="session-action pet-toggle ${petCls}" data-action="toggle-pet" data-session="${s.session_id}"
              title="${s.pet_enabled ? "关闭该会话的桌宠" : "打开该会话的桌宠"}">🐾</button>
            <button class="session-action session-close" data-action="close-session" data-session="${s.session_id}"
              title="关闭该会话（清理状态记录）">✕</button>
          </div>
        </div>`;
    }).join("");

    // 事件委托：桌宠开关 / 关闭会话
    container.onclick = async (e) => {
      const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-action]");
      if (!btn) return;
      const sessionId = btn.dataset.session!;
      const action = btn.dataset.action!;
      btn.disabled = true;
      try {
        if (action === "toggle-pet") {
          await invoke("set_pet_enabled", { sessionId, enabled: btn.classList.contains("off") });
        } else if (action === "close-session") {
          await invoke("close_session", { sessionId });
        }
      } catch (err) {
        console.error(`${action} failed:`, err);
      } finally {
        await refreshSessions();
      }
    };
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

  await invoke("frontend_ready").catch(() => {});
}

main().catch((err) => console.error("dashboard error:", err));
