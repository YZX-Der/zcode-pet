#!/usr/bin/env python3
"""petgen — zcode-pet 像素宠物生成器

用字符网格 + 调色板定义 24×26 逻辑像素的宠物造型，
经程序化帧变换（呼吸浮动 / 眨眼 / 状态徽标）生成
Codex 兼容的 1536×1872（8 列 × 9 行，帧 192×208）精灵表。

用法：
    python3 tools/petgen.py build [--pet NAME] [--out assets/pets]
    python3 tools/petgen.py preview [--pet NAME] [--out /tmp/petgen-preview]
    python3 tools/petgen.py validate <pet_dir>...

精灵表行约定（v1）：
    row0 idle / row1 running / row2 needs_input /
    row3 ready / row4 blocked / row5 sleep / row6-8 保留（透明）
"""
from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path

from PIL import Image

# ---------------------------------------------------------------- 常量

LOGICAL_W, LOGICAL_H = 24, 26          # 逻辑网格
SCALE = 8                              # 放大倍数（最近邻）
FRAME_W, FRAME_H = LOGICAL_W * SCALE, LOGICAL_H * SCALE   # 192×208
COLS, ROWS = 8, 9
SHEET_W, SHEET_H = FRAME_W * COLS, FRAME_H * ROWS         # 1536×1872

STATES = ["idle", "running", "needs_input", "ready", "blocked", "sleep"]
STATE_FPS = {
    "idle": 6, "running": 10, "needs_input": 6,
    "ready": 6, "blocked": 4, "sleep": 3,
}
PROTOCOL_VERSION = 1

# ---------------------------------------------------------------- 宠物定义


@dataclass(frozen=True)
class Eye:
    """眨眼覆盖区域（逻辑坐标），bg 为闭眼时填充的底色（调色板键）。"""
    x: int
    y: int
    w: int
    h: int
    bg: str


@dataclass(frozen=True)
class PetDef:
    key: str
    display_name: str
    description: str
    palette: dict[str, tuple[int, int, int, int]]
    grid: tuple[str, ...]          # 26 行 × 24 列
    eyes: tuple[Eye, ...]

# 调色板通用键：
#   O 描边  B 主体  b 主体阴影  L 浅色区（脸/肚皮）
#   E 眼白  P 瞳孔/深色五官  C 腮红  A 强调色
#   '.' 透明

# ---------------------------------------------------------------- 绘制原语
# 用代码原语生成 24×26 网格，行宽由代码保证，杜绝手工字符画的错位问题。


class Grid:
    """24×26 字符画布，后画的覆盖先画的。"""

    W, H = LOGICAL_W, LOGICAL_H

    def __init__(self) -> None:
        self.px: list[list[str]] = [["."] * self.W for _ in range(self.H)]

    def pt(self, x: int, y: int, ch: str) -> "Grid":
        if 0 <= x < self.W and 0 <= y < self.H:
            self.px[y][x] = ch
        return self

    def rect(self, x0: int, y0: int, x1: int, y1: int, ch: str) -> "Grid":
        """实心矩形（含边界）。"""
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.pt(x, y, ch)
        return self

    def box(self, x0: int, y0: int, x1: int, y1: int, outline: str, fill: str) -> "Grid":
        """带描边的实心矩形。"""
        self.rect(x0, y0, x1, y1, outline)
        if x1 - x0 >= 2 and y1 - y0 >= 2:
            self.rect(x0 + 1, y0 + 1, x1 - 1, y1 - 1, fill)
        return self

    def hline(self, x0: int, x1: int, y: int, ch: str) -> "Grid":
        for x in range(x0, x1 + 1):
            self.pt(x, y, ch)
        return self

    def vline(self, x: int, y0: int, y1: int, ch: str) -> "Grid":
        for y in range(y0, y1 + 1):
            self.pt(x, y, ch)
        return self

    def ellipse(self, cx: int, cy: int, rx: int, ry: int, ch: str,
                outline: str | None = None) -> "Grid":
        """椭圆填充；给 outline 时先画稍大的描边椭圆。"""
        if outline is not None:
            self._fill_ellipse(cx, cy, rx + 1, ry + 1, outline)
        self._fill_ellipse(cx, cy, rx, ry, ch)
        return self

    def _fill_ellipse(self, cx: int, cy: int, rx: int, ry: int, ch: str) -> None:
        for y in range(cy - ry, cy + ry + 1):
            for x in range(cx - rx, cx + rx + 1):
                if rx > 0 and ry > 0 and ((x - cx) ** 2) * (ry ** 2) + ((y - cy) ** 2) * (rx ** 2) <= (rx ** 2) * (ry ** 2):
                    self.pt(x, y, ch)

    def rows(self) -> tuple[str, ...]:
        return tuple("".join(r) for r in self.px)


# ---------------------------------------------------------------- 五只宠物


def _build_zbuddy() -> Grid:
    g = Grid()
    # 天线
    g.vline(11, 2, 4, "O").vline(12, 2, 4, "O").rect(11, 1, 12, 1, "A")
    # 头（14×12，x5-18，y4-15）
    g.box(5, 4, 18, 15, "O", "B")
    # 面部面板（x7-16，y7-12）
    g.rect(7, 7, 16, 12, "L")
    # 眼白 3×3：左 x8-10，右 x13-15，y8-10
    g.rect(8, 8, 10, 10, "E").rect(13, 8, 15, 10, "E")
    g.pt(9, 9, "P").pt(14, 9, "P")          # 瞳孔
    g.hline(10, 13, 12, "P")                # 嘴
    # 身体（x8-15，y16-22）
    g.box(8, 16, 15, 22, "O", "B")
    g.rect(9, 18, 14, 18, "A")              # 胸口灯带
    g.hline(9, 10, 21, "b").hline(13, 14, 21, "b")   # 底部阴影
    # 脚
    g.rect(9, 23, 10, 24, "O").rect(13, 23, 14, 24, "O")
    return g


def _build_shiba() -> Grid:
    g = Grid()
    # 耳朵（方块耳 + 耳尖 + 耳内，贴住头顶 y7）
    g.rect(5, 4, 7, 7, "O").pt(6, 3, "O").rect(6, 5, 6, 6, "b")
    g.rect(16, 4, 18, 7, "O").pt(17, 3, "O").rect(17, 5, 17, 6, "b")
    # 头（圆脸）
    g.ellipse(12, 13, 7, 6, "B", outline="O")
    # 吻部（浅色）
    g.ellipse(12, 17, 4, 2, "L")
    # 眼白
    g.rect(6, 10, 8, 12, "E").rect(15, 10, 17, 12, "E")
    g.pt(7, 11, "P").pt(16, 11, "P")
    # 鼻 + 嘴线
    g.rect(11, 14, 12, 14, "P").pt(12, 15, "P")
    # 腮红
    g.pt(5, 14, "C").pt(18, 14, "C")
    # 项圈
    g.hline(8, 15, 19, "A")
    return g


def _build_ducky() -> Grid:
    g = Grid()
    # 身体（大圆）
    g.ellipse(12, 14, 7, 7, "B", outline="O")
    # 肚皮
    g.ellipse(12, 17, 4, 3, "L")
    # 眼白
    g.rect(7, 8, 9, 10, "E").rect(14, 8, 16, 10, "E")
    g.pt(8, 9, "P").pt(15, 9, "P")
    # 喙（橙色，前视）
    g.rect(10, 12, 13, 13, "A").rect(11, 14, 12, 14, "A")
    # 腮红
    g.pt(5, 12, "C").pt(18, 12, "C")
    # 脚蹼
    g.rect(8, 22, 9, 23, "A").rect(14, 22, 15, 23, "A")
    return g


def _build_slime() -> Grid:
    g = Grid()
    # 身体（ dome ）
    g.ellipse(12, 14, 7, 6, "B", outline="O")
    # 高光
    g.ellipse(9, 10, 2, 1, "L")
    # 眼白（大而低）
    g.rect(6, 12, 8, 14, "E").rect(15, 12, 17, 14, "E")
    g.pt(7, 13, "P").pt(16, 13, "P")
    # 嘴
    g.hline(10, 13, 16, "P")
    # 腮红
    g.pt(4, 15, "C").pt(19, 15, "C")
    # 底部阴影
    g.hline(7, 16, 19, "b")
    return g


def _build_rocky() -> Grid:
    g = Grid()
    # 头顶小花
    g.pt(11, 1, "A").pt(10, 2, "A").pt(11, 2, "A").pt(12, 2, "A").pt(11, 3, "A")
    g.vline(11, 4, 5, "O")
    # 石头主体
    g.ellipse(12, 14, 7, 6, "B", outline="O")
    # 高光
    g.ellipse(9, 10, 2, 1, "L")
    # 眼白（纯白眼，呆萌）
    g.rect(7, 12, 9, 14, "E").rect(14, 12, 16, 14, "E")
    g.pt(8, 13, "P").pt(15, 13, "P")
    # 微笑
    g.hline(10, 13, 16, "P")
    # 腮红
    g.pt(5, 15, "C").pt(18, 15, "C")
    # 石纹
    g.hline(7, 8, 18, "b").hline(15, 16, 19, "b")
    # 底部
    g.hline(8, 15, 20, "b")
    return g


ZBUDDY = PetDef(
    key="zbuddy",
    display_name="Z Buddy",
    description="zcode-pet 默认机器人伙伴",
    palette={
        "O": (43, 45, 66, 255),
        "B": (141, 153, 174, 255),
        "b": (108, 122, 137, 255),
        "L": (237, 242, 244, 255),
        "E": (255, 255, 255, 255),
        "P": (34, 34, 34, 255),
        "C": (255, 159, 178, 255),
        "A": (239, 35, 60, 255),
    },
    grid=_build_zbuddy().rows(),
    eyes=(Eye(8, 8, 3, 3, "L"), Eye(13, 8, 3, 3, "L")),
)

SHIBA = PetDef(
    key="shiba",
    display_name="Shiba",
    description="微笑柴犬",
    palette={
        "O": (91, 58, 41, 255),
        "B": (244, 162, 97, 255),
        "b": (231, 111, 81, 255),
        "L": (254, 250, 224, 255),
        "E": (255, 255, 255, 255),
        "P": (43, 33, 24, 255),
        "C": (244, 164, 164, 255),
        "A": (42, 157, 143, 255),
    },
    grid=_build_shiba().rows(),
    eyes=(Eye(6, 10, 3, 3, "B"), Eye(15, 10, 3, 3, "B")),
)

DUCKY = PetDef(
    key="ducky",
    display_name="Ducky",
    description="淡定小黄鸭",
    palette={
        "O": (122, 92, 0, 255),
        "B": (255, 214, 10, 255),
        "b": (240, 180, 41, 255),
        "L": (255, 243, 176, 255),
        "E": (255, 255, 255, 255),
        "P": (43, 33, 24, 255),
        "C": (255, 179, 193, 255),
        "A": (255, 133, 0, 255),
    },
    grid=_build_ducky().rows(),
    eyes=(Eye(7, 8, 3, 3, "B"), Eye(14, 8, 3, 3, "B")),
)

SLIME = PetDef(
    key="slime",
    display_name="Slime",
    description="软乎乎的史莱姆",
    palette={
        "O": (27, 67, 50, 255),
        "B": (82, 183, 136, 255),
        "b": (64, 145, 108, 255),
        "L": (183, 228, 199, 255),
        "E": (255, 255, 255, 255),
        "P": (27, 42, 34, 255),
        "C": (255, 175, 204, 255),
        "A": (216, 243, 220, 255),
    },
    grid=_build_slime().rows(),
    eyes=(Eye(6, 12, 3, 3, "B"), Eye(15, 12, 3, 3, "B")),
)

ROCKY = PetDef(
    key="rocky",
    display_name="Rocky",
    description="diff 很大时的沉稳依靠",
    palette={
        "O": (52, 58, 64, 255),
        "B": (173, 181, 189, 255),
        "b": (134, 142, 150, 255),
        "L": (222, 226, 230, 255),
        "E": (255, 255, 255, 255),
        "P": (33, 37, 41, 255),
        "C": (255, 168, 168, 255),
        "A": (255, 212, 59, 255),
    },
    grid=_build_rocky().rows(),
    eyes=(Eye(7, 12, 3, 3, "B"), Eye(14, 12, 3, 3, "B")),
)

PETS: tuple[PetDef, ...] = (ZBUDDY, SHIBA, DUCKY, SLIME, ROCKY)

# ---------------------------------------------------------------- 状态徽标字形

# 小尺寸逻辑像素字形，缩放 8× 后清晰可读
GLYPH_CHECK = (
    ".....X",
    "....XX",
    "...XX.",
    "X.XX..",
    ".XX...",
    ".X....",
)
GLYPH_CROSS = (
    "X....X",
    ".X..X.",
    "..XX..",
    "..XX..",
    ".X..X.",
    "X....X",
)
GLYPH_BANG = (
    "XX",
    "XX",
    "XX",
    "XX",
    "..",
    "XX",
    "XX",
)
GLYPH_Z = (
    "XXXXX",
    "...X.",
    "..X..",
    ".X...",
    "XXXXX",
)

BADGE_COLORS = {
    "running": (80, 200, 255, 255),     # 蓝：三点循环
    "needs_input": (255, 159, 28, 255),  # 橙：!
    "ready": (64, 192, 87, 255),        # 绿：✓
    "blocked": (224, 49, 48, 255),      # 红：✗
    "sleep": (151, 117, 250, 255),      # 紫：z
}

# 每状态的身体位移模式（逻辑像素，长度 = COLS）
# 设计约束：任意相邻两帧（含首尾相接）必须有可见差异
DY_IDLE = [0, -1, 0, -1, 0, 1, 0, -1]
DY_RUNNING = [0, -2, 0, -2, 0, -2, 0, -2]
DX_RUNNING = [0, 1, 0, -1, 0, 1, 0, -1]
DY_NEEDS_INPUT = [0, -1, 0, 0, 0, -1, 0, 0]
DY_READY = [0, -2, 0, -2, 0, -1, 0, -1]
DX_BLOCKED = [0, -1, 0, 1, 0, -1, 0, 1]

BLINK_FRAMES = {"idle": {4}, "needs_input": {6}, "ready": set(), "running": set(), "blocked": set()}

# ---------------------------------------------------------------- 渲染引擎


def _logical_image() -> Image.Image:
    return Image.new("RGBA", (LOGICAL_W, LOGICAL_H), (0, 0, 0, 0))


def _draw_grid(img: Image.Image, pet: PetDef) -> None:
    px = img.load()
    for y, row in enumerate(pet.grid):
        if len(row) != LOGICAL_W:
            raise ValueError(f"{pet.key} 第 {y} 行宽度 {len(row)} != {LOGICAL_W}")
        for x, ch in enumerate(row):
            if ch == ".":
                continue
            try:
                px[x, y] = pet.palette[ch]
            except KeyError as exc:
                raise ValueError(f"{pet.key} 第 {y} 行未知调色板键 {ch!r}") from exc


def _apply_blink(img: Image.Image, pet: PetDef) -> None:
    """闭眼：眼睛区域填底色，中间画一条瞳孔色横线。"""
    px = img.load()
    pupil = pet.palette["P"]
    for eye in pet.eyes:
        bg = pet.palette[eye.bg]
        mid_y = eye.y + eye.h // 2
        for yy in range(eye.y, eye.y + eye.h):
            for xx in range(eye.x, eye.x + eye.w):
                px[xx, yy] = pupil if yy == mid_y else bg


def _draw_glyph(img: Image.Image, glyph: tuple[str, ...], ox: int, oy: int,
                color: tuple[int, int, int, int]) -> None:
    px = img.load()
    for dy, row in enumerate(glyph):
        for dx, ch in enumerate(row):
            if ch == "X":
                x, y = ox + dx, oy + dy
                if 0 <= x < LOGICAL_W and 0 <= y < LOGICAL_H:
                    px[x, y] = color


def _draw_badge(img: Image.Image, state: str, frame: int) -> None:
    color = BADGE_COLORS[state]
    if state == "running":
        # 三点循环加载：亮度随帧轮转
        for i in range(3):
            active = (frame // 1 + i) % 3 == 0
            c = color if active else (color[0], color[1], color[2], 110)
            _draw_glyph(img, ("XX", "XX"), 17 + i * 3, 3, c)
    elif state == "needs_input":
        bounce = [0, -1, -2, -1, 0, -1, -2, -1][frame % COLS]
        _draw_glyph(img, GLYPH_BANG, 19, 2 + bounce, color)
    elif state == "ready":
        _draw_glyph(img, GLYPH_CHECK, 17, 2, color)
    elif state == "blocked":
        _draw_glyph(img, GLYPH_CROSS, 17, 2, color)
    elif state == "sleep":
        # 三颗 z 以 4 帧为周期上飘并淡出，循环复位
        rise = -(frame % 4)
        fade = (frame % 4) * 40
        for i, (ox, oy) in enumerate(((17, 5), (19, 3), (21, 1))):
            alpha = max(80, 255 - i * 50 - fade)
            _draw_glyph(img, GLYPH_Z, ox, oy + rise, (color[0], color[1], color[2], alpha))


def render_frame(pet: PetDef, state: str, frame: int) -> Image.Image:
    """渲染单帧（192×208 RGBA）。"""
    img = _logical_image()
    _draw_grid(img, pet)

    dx = dy = 0
    if state == "idle":
        dy = DY_IDLE[frame % COLS]
    elif state == "running":
        dy = DY_RUNNING[frame % COLS]
        dx = DX_RUNNING[frame % COLS]
    elif state == "needs_input":
        dy = DY_NEEDS_INPUT[frame % COLS]
    elif state == "ready":
        dy = DY_READY[frame % COLS]
    elif state == "blocked":
        dx = DX_BLOCKED[frame % COLS]

    eyes_closed = state == "sleep" or frame in BLINK_FRAMES.get(state, set())
    if eyes_closed:
        _apply_blink(img, pet)
    if state in BADGE_COLORS:
        _draw_badge(img, state, frame)

    if dx or dy:
        img = img.transform(
            (LOGICAL_W, LOGICAL_H), Image.AFFINE, (1, 0, -dx, 0, 1, -dy)
        )

    return img.resize((FRAME_W, FRAME_H), Image.NEAREST)


def build_sheet(pet: PetDef) -> Image.Image:
    """生成 1536×1872 精灵表；row6-8 保留为透明。"""
    sheet = Image.new("RGBA", (SHEET_W, SHEET_H), (0, 0, 0, 0))
    for row, state in enumerate(STATES):
        for col in range(COLS):
            frame_img = render_frame(pet, state, col)
            sheet.paste(frame_img, (col * FRAME_W, row * FRAME_H))
    return sheet


def manifest(pet: PetDef, sheet_name: str) -> dict:
    return {
        "version": PROTOCOL_VERSION,
        "name": pet.key,
        "display_name": pet.display_name,
        "description": pet.description,
        "author": "zcode-pet petgen",
        "frame": [FRAME_W, FRAME_H],
        "cols": COLS,
        "rows": ROWS,
        "sheet": sheet_name,
        "states": {
            state: {"row": i, "frames": COLS, "fps": STATE_FPS[state]}
            for i, state in enumerate(STATES)
        },
    }

# ---------------------------------------------------------------- 命令


def cmd_build(args: argparse.Namespace) -> int:
    out_root = Path(args.out)
    pets = _select_pets(args.pet)
    for pet in pets:
        pet_dir = out_root / pet.key
        pet_dir.mkdir(parents=True, exist_ok=True)
        sheet = build_sheet(pet)
        sheet_path = pet_dir / "spritesheet.webp"
        sheet.save(sheet_path, format="WEBP", lossless=True, quality=100)
        (pet_dir / "pet.json").write_text(
            json.dumps(manifest(pet, sheet_path.name), ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        print(f"[build] {pet.key}: {sheet_path} ({sheet.size[0]}x{sheet.size[1]})")
    return 0


def cmd_preview(args: argparse.Namespace) -> int:
    """每只宠物输出一张预览图：6 状态 × 帧0，横向排列，2× 放大。"""
    out_root = Path(args.out)
    out_root.mkdir(parents=True, exist_ok=True)
    for pet in _select_pets(args.pet):
        strip = Image.new("RGBA", (FRAME_W * len(STATES), FRAME_H), (40, 40, 48, 255))
        for i, state in enumerate(STATES):
            strip.paste(render_frame(pet, state, 0), (i * FRAME_W, 0))
        strip = strip.resize((strip.width * 2, strip.height * 2), Image.NEAREST)
        path = out_root / f"{pet.key}-preview.png"
        strip.save(path)
        print(f"[preview] {path}")
    return 0


def cmd_validate(args: argparse.Namespace) -> int:
    ok = True
    for raw in args.pet_dirs:
        ok &= validate_pet_dir(Path(raw))
    return 0 if ok else 1


def validate_pet_dir(pet_dir: Path) -> bool:
    """校验宠物目录：清单 schema、精灵表尺寸、每行动画有效性。"""
    errors: list[str] = []
    manifest_path = pet_dir / "pet.json"
    if not manifest_path.exists():
        print(f"[validate] {pet_dir}: 缺少 pet.json")
        return False
    data = json.loads(manifest_path.read_text(encoding="utf-8"))

    for key in ("version", "name", "frame", "cols", "rows", "sheet", "states"):
        if key not in data:
            errors.append(f"pet.json 缺少字段 {key}")
    if errors:
        print(f"[validate] {pet_dir}: " + "; ".join(errors))
        return False

    sheet_path = pet_dir / data["sheet"]
    if not sheet_path.exists():
        print(f"[validate] {pet_dir}: 缺少精灵表 {data['sheet']}")
        return False
    sheet = Image.open(sheet_path).convert("RGBA")
    fw, fh = data["frame"]
    if sheet.size != (fw * data["cols"], fh * data["rows"]):
        errors.append(f"精灵表尺寸 {sheet.size} 与清单不符")

    for state, spec in data["states"].items():
        row, frames = spec["row"], spec["frames"]
        prev = None
        row_has_pixels = False
        for col in range(frames):
            box = (col * fw, row * fh, (col + 1) * fw, (row + 1) * fh)
            region = sheet.crop(box)
            data_bytes = region.tobytes()  # 全 RGBA 比较：眨眼等纯变色也要被检测
            row_has_pixels |= any(region.getchannel("A").tobytes())
            if prev is not None and data_bytes == prev:
                errors.append(f"{state} 第 {col} 帧与前一帧完全相同（动画失效）")
            prev = data_bytes
        if not row_has_pixels:
            errors.append(f"{state} 整行透明")

    if errors:
        print(f"[validate] {pet_dir}:")
        for e in errors:
            print(f"  - {e}")
        return False
    print(f"[validate] {pet_dir}: OK")
    return True


def _select_pets(name: str | None) -> list[PetDef]:
    if not name:
        return list(PETS)
    for pet in PETS:
        if pet.key == name:
            return [pet]
    raise SystemExit(f"未知宠物 {name!r}，可选：{', '.join(p.key for p in PETS)}")


def main() -> int:
    parser = argparse.ArgumentParser(description="zcode-pet 像素宠物生成器")
    sub = parser.add_subparsers(dest="command", required=True)

    p_build = sub.add_parser("build", help="生成精灵表与清单")
    p_build.add_argument("--pet", help="只生成指定宠物")
    p_build.add_argument("--out", default="assets/pets", help="输出根目录")
    p_build.set_defaults(func=cmd_build)

    p_preview = sub.add_parser("preview", help="生成状态预览图")
    p_preview.add_argument("--pet", help="只预览指定宠物")
    p_preview.add_argument("--out", default="/tmp/petgen-preview", help="输出目录")
    p_preview.set_defaults(func=cmd_preview)

    p_validate = sub.add_parser("validate", help="校验宠物目录")
    p_validate.add_argument("pet_dirs", nargs="+", help="宠物目录")
    p_validate.set_defaults(func=cmd_validate)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
