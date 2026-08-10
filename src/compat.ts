/**
 * Codex 宠物格式兼容层
 *
 * Codex pet.json 只含 { id, displayName, description, spritesheetPath, spriteVersionNumber? }，
 * 不存储帧尺寸/状态/动画——几何布局和状态→行映射由 app 硬编码。
 *
 * 精灵表几何（v1，与 zcode-pet 完全一致）：
 *   1536×1872，8 列 × 9 行，192×208 每帧
 *
 * Codex 的 9 行状态映射（仅 v1 前 9 行，v2 额外的方向行不使用）：
 *   0 idle         1 running-right   2 running-left   3 waving
 *   4 jumping      5 failed          6 waiting        7 running
 *   8 review
 *
 * zcode-pet 6 状态 → Codex 行的语义映射：
 *   idle        → row 0 (idle)
 *   running     → row 7 (running，原地工作循环)
 *   needs_input → row 6 (waiting，等待用户)
 *   ready       → row 3 (waving，任务完成的招呼)
 *   blocked     → row 5 (failed，出错)
 *   sleep       → row 0 (idle，Codex 无 sleep，复用 idle)
 *
 * 参考：codex-rs/tui/src/pets/model.rs, awesome-codex-pet, Petdex
 */

import type { PetManifest, StateSpec } from "./protocol";

/** Codex pet.json 的原始格式（v1/v2） */
export interface CodexPetJson {
  id?: string;
  displayName?: string;
  name?: string;
  description?: string;
  spritesheetPath?: string;
  spriteVersionNumber?: number;
}

/** zcode-pet 状态 → Codex 行号 */
const STATE_TO_CODEX_ROW: Record<string, number> = {
  idle: 0,
  running: 7,
  needs_input: 6,
  ready: 3,
  blocked: 5,
  sleep: 0,
};

/** Codex 各行的帧数与帧率（基于 codex-rs model.rs 的 frameMs 表推导） */
const CODEX_ROW_SPECS: Record<number, { frames: number; fps: number }> = {
  0: { frames: 6, fps: 6 }, // idle: 1680ms 循环
  1: { frames: 8, fps: 8 }, // running-right
  2: { frames: 8, fps: 8 }, // running-left
  3: { frames: 4, fps: 7 }, // waving
  4: { frames: 5, fps: 7 }, // jumping
  5: { frames: 8, fps: 7 }, // failed
  6: { frames: 6, fps: 7 }, // waiting
  7: { frames: 6, fps: 8 }, // running
  8: { frames: 6, fps: 7 }, // review
};

/** 检测是否为 Codex 格式的 pet.json（缺少 zcode-pet 特有字段） */
export function isCodexFormat(raw: Record<string, unknown>): boolean {
  const hasCodexFields = "spritesheetPath" in raw || "spriteVersionNumber" in raw;
  const hasZcodeFields = "frame" in raw && "states" in raw;
  return hasCodexFields || (!hasZcodeFields && ("id" in raw || "displayName" in raw));
}

/** 将 Codex pet.json 转换为 zcode-pet PetManifest */
export function adaptCodexPet(codex: CodexPetJson, sheetFileName: string): PetManifest {
  const states: Record<string, StateSpec> = {};
  for (const [stateName, row] of Object.entries(STATE_TO_CODEX_ROW)) {
    const spec = CODEX_ROW_SPECS[row] ?? { frames: 8, fps: 6 };
    states[stateName] = { row, frames: spec.frames, fps: spec.fps };
  }

  return {
    version: codex.spriteVersionNumber ?? 1,
    name: codex.id ?? "codex-pet",
    display_name: codex.displayName ?? codex.name ?? codex.id ?? "Codex Pet",
    description: codex.description ?? "",
    frame: [192, 208],
    cols: 8,
    rows: codex.spriteVersionNumber === 2 ? 11 : 9,
    sheet: sheetFileName,
    states,
  };
}

/**
 * 统一加载宠物清单：自动检测 Codex 或 zcode-pet 格式。
 * @param raw 解析后的 pet.json 对象
 * @param sheetFileName 精灵表文件名（如 "spritesheet.webp"）
 */
export function loadManifest(raw: Record<string, unknown>, sheetFileName: string): PetManifest {
  if (isCodexFormat(raw)) {
    return adaptCodexPet(raw as CodexPetJson, sheetFileName);
  }
  // zcode-pet 原生格式，直接断言
  return raw as unknown as PetManifest;
}
