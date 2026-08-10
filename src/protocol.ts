/** 状态协议与宠物清单的共享类型 —— 对应 docs/03-state-protocol.md */

export type PetStateName =
  | "idle"
  | "running"
  | "needs_input"
  | "ready"
  | "blocked"
  | "sleep";

export interface SessionState {
  version: number;
  session_id: string;
  state: string;
  event: string;
  tool?: string;
  project?: string;
  project_dir?: string;
  ts: number;
}

export interface StateSpec {
  row: number;
  frames: number;
  fps: number;
}

export interface PetManifest {
  version: number;
  name: string;
  display_name: string;
  description: string;
  frame: [number, number];
  cols: number;
  rows: number;
  sheet: string;
  states: Record<string, StateSpec>;
}

export interface StatePayload {
  state: PetStateName;
}
