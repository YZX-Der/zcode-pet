#!/bin/sh
# zcode-pet 安装脚本：部署 hook 脚本并合并 ZCode hooks 配置
# 幂等，可重复执行；原配置自动备份。
set -eu

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
PET_HOME="$HOME/.zcode-pet"
ZCODE_CONFIG="$HOME/.zcode/cli/config.json"

echo "==> deploy hook script to $PET_HOME/bin/"
mkdir -p "$PET_HOME/bin" "$PET_HOME/state" "$PET_HOME/pets"
cp "$REPO_DIR/scripts/zcode-hook" "$PET_HOME/bin/zcode-hook"
chmod +x "$PET_HOME/bin/zcode-hook"

echo "==> merge ZCode hooks config: $ZCODE_CONFIG"
python3 - "$ZCODE_CONFIG" "$PET_HOME/bin/zcode-hook" <<'PY'
import json
import os
import shutil
import sys
import time
from pathlib import Path

EVENTS = [
    "SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest",
    "PostToolUse", "PostToolUseFailure", "Stop",
]

config_path = Path(sys.argv[1])
hook_bin = sys.argv[2]

if config_path.exists():
    backup = config_path.with_name(
        config_path.name + f".bak-{time.strftime('%Y%m%d-%H%M%S')}"
    )
    shutil.copy2(config_path, backup)
    print(f"  backup: {backup}")
    config = json.loads(config_path.read_text(encoding="utf-8"))
else:
    config_path.parent.mkdir(parents=True, exist_ok=True)
    config = {}

hooks = config.setdefault("hooks", {})
hooks["enabled"] = True
events = hooks.setdefault("events", {})
for ev in EVENTS:
    events[ev] = [{
        "hooks": [{
            "type": "process",
            "command": hook_bin,
            "args": [ev],
            "timeoutMs": 2000,
        }]
    }]

config_path.write_text(
    json.dumps(config, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
)
os.chmod(config_path, 0o600)
print(f"  wrote {len(EVENTS)} event hooks (enabled=true)")
PY

cat <<'EOF'

install done. notes:
  1. restart your ZCode session for hooks to take effect
  2. uninstall: remove the "hooks" field from ~/.zcode/cli/config.json
     and run: rm -rf ~/.zcode-pet
EOF
