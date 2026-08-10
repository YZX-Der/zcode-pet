import { describe, it, expect } from "vitest";
import { isCodexFormat, adaptCodexPet, loadManifest } from "./compat";
import type { PetManifest } from "./protocol";

const codexV1 = {
  id: "my-pet",
  displayName: "My Pet",
  description: "A community pet",
  spritesheetPath: "spritesheet.webp",
};

const codexV2 = {
  id: "v2-pet",
  displayName: "V2 Pet",
  description: "A v2 community pet",
  spritesheetPath: "spritesheet.webp",
  spriteVersionNumber: 2,
};

const zcodeNative: PetManifest = {
  version: 1,
  name: "zbuddy",
  display_name: "Z Buddy",
  description: "zcode-pet 默认伙伴",
  frame: [192, 208],
  cols: 8,
  rows: 9,
  sheet: "spritesheet.webp",
  states: {
    idle: { row: 0, frames: 8, fps: 6 },
    running: { row: 1, frames: 8, fps: 10 },
    needs_input: { row: 2, frames: 8, fps: 6 },
    ready: { row: 3, frames: 8, fps: 6 },
    blocked: { row: 4, frames: 8, fps: 4 },
    sleep: { row: 5, frames: 8, fps: 3 },
  },
};

describe("isCodexFormat", () => {
  it("检测 Codex v1 格式（含 spritesheetPath）", () => {
    expect(isCodexFormat(codexV1)).toBe(true);
  });

  it("检测 Codex v2 格式（含 spriteVersionNumber）", () => {
    expect(isCodexFormat(codexV2)).toBe(true);
  });

  it("检测 zcode-pet 原生格式（含 frame + states）", () => {
    expect(isCodexFormat(zcodeNative as unknown as Record<string, unknown>)).toBe(false);
  });
});

describe("adaptCodexPet", () => {
  it("v1 转换：生成 6 状态映射", () => {
    const m = adaptCodexPet(codexV1, "spritesheet.webp");
    expect(m.name).toBe("my-pet");
    expect(m.display_name).toBe("My Pet");
    expect(m.frame).toEqual([192, 208]);
    expect(m.cols).toBe(8);
    expect(m.rows).toBe(9);
    expect(Object.keys(m.states).sort()).toEqual(
      ["blocked", "idle", "needs_input", "ready", "running", "sleep"],
    );
  });

  it("v1 状态→行映射正确", () => {
    const m = adaptCodexPet(codexV1, "spritesheet.webp");
    expect(m.states.idle.row).toBe(0);
    expect(m.states.running.row).toBe(7); // Codex running 行
    expect(m.states.needs_input.row).toBe(6); // Codex waiting 行
    expect(m.states.ready.row).toBe(3); // Codex waving 行
    expect(m.states.blocked.row).toBe(5); // Codex failed 行
    expect(m.states.sleep.row).toBe(0); // 复用 idle
  });

  it("v2 转换：rows=11", () => {
    const m = adaptCodexPet(codexV2, "spritesheet.webp");
    expect(m.version).toBe(2);
    expect(m.rows).toBe(11);
  });
});

describe("loadManifest", () => {
  it("自动检测 Codex 格式并转换", () => {
    const m = loadManifest(
      codexV1 as unknown as Record<string, unknown>,
      "spritesheet.webp",
    );
    expect(m.frame).toEqual([192, 208]);
    expect(m.states.running).toBeDefined();
  });

  it("原生格式直接透传", () => {
    const m = loadManifest(
      zcodeNative as unknown as Record<string, unknown>,
      "spritesheet.webp",
    );
    expect(m.name).toBe("zbuddy");
    expect(m.states.running.row).toBe(1); // zcode-pet running=row 1
    expect(m.states.running.fps).toBe(10); // 原生 fps 保持
  });
});
