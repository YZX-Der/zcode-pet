#!/bin/sh
# zcode-hook 自测：临时 HOME 隔离，断言各事件写入正确状态
set -eu

HOOK="$(cd "$(dirname "$0")" && pwd)/zcode-hook"
TMP_HOME="$(mktemp -d)"
trap 'rm -rf "$TMP_HOME"' EXIT

STATE_FILE="$TMP_HOME/.zcode-pet/state/test-sid.json"
FAIL=0

check() {  # <事件> <期望状态>
  CLAUDE_SESSION_ID="test-sid" CLAUDE_PROJECT_DIR="/tmp/我的项目" \
    HOME="$TMP_HOME" "$HOOK" "$1"
  if [ ! -f "$STATE_FILE" ]; then
    echo "✗ $1: 状态文件未生成"; FAIL=1; return
  fi
  if grep -q "\"state\":\"$2\"" "$STATE_FILE"; then
    echo "✓ $1 → $2"
  else
    echo "✗ $1: 期望 $2，实际 $(cat "$STATE_FILE")"; FAIL=1
  fi
}

check SessionStart idle
check UserPromptSubmit running
check PreToolUse running
check PostToolUse running
check PermissionRequest needs_input
check PostToolUseFailure blocked
check Stop ready

# 项目名提取（含中文路径）
if grep -q '"project":"我的项目"' "$STATE_FILE"; then
  echo "✓ 项目名提取（中文路径）"
else
  echo "✗ 项目名提取失败: $(cat "$STATE_FILE")"; FAIL=1
fi

# 未知事件：静默退出 0，不产生新文件
rm -f "$STATE_FILE"
CLAUDE_SESSION_ID="test-sid" HOME="$TMP_HOME" "$HOOK" BogusEvent
if [ ! -f "$STATE_FILE" ]; then echo "✓ 未知事件静默忽略"; else echo "✗ 未知事件不应写文件"; FAIL=1; fi

# 无 session id：退出 0 且不写文件
if CLAUDE_SESSION_ID="" HOME="$TMP_HOME" "$HOOK" Stop && [ ! -f "$STATE_FILE" ]; then
  echo "✓ 缺少 session id 静默通过"
else
  echo "✗ 缺少 session id 行为异常"; FAIL=1
fi

# JSON 合法性（python 校验最后一次写入）
check Stop ready
if python3 -c "import json,sys; json.load(open('$STATE_FILE'))" 2>/dev/null; then
  echo "✓ JSON 格式合法"
else
  echo "✗ JSON 格式非法"; FAIL=1
fi

if [ "$FAIL" -eq 0 ]; then echo "test_hook: 全部通过"; else echo "test_hook: 存在失败"; exit 1; fi
