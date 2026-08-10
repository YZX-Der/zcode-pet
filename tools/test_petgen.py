#!/usr/bin/env python3
"""petgen 自测：全量构建到临时目录并校验全部宠物。

用法：python3 tools/test_petgen.py
"""
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PETGEN = ROOT / "tools" / "petgen.py"
EXPECTED_PETS = 5


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        build = subprocess.run(
            [sys.executable, str(PETGEN), "build", "--out", tmp],
            capture_output=True, text=True,
        )
        if build.returncode != 0:
            print(build.stdout)
            print(build.stderr, file=sys.stderr)
            return 1

        pet_dirs = sorted(p for p in Path(tmp).iterdir() if p.is_dir())
        if len(pet_dirs) != EXPECTED_PETS:
            print(f"期望 {EXPECTED_PETS} 只宠物，实际 {len(pet_dirs)}", file=sys.stderr)
            return 1

        validate = subprocess.run(
            [sys.executable, str(PETGEN), "validate", *(str(d) for d in pet_dirs)],
            capture_output=True, text=True,
        )
        print(validate.stdout, end="")
        if validate.returncode != 0:
            print(validate.stderr, file=sys.stderr)
            return 1

    print("test_petgen: 全部通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
