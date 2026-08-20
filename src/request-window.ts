/** 权限确认弹窗逻辑 -- 加载 request.html 时运行 */

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** 10 分钟无操作自动关闭 */
const AUTO_CLOSE_MS = 10 * 60 * 1000;

function render(req: {
  request_id: string;
  tool: string;
  input_summary: string;
  risk: string;
  reason: string;
}): void {
  const riskEl = document.getElementById("req-risk")!;
  const toolEl = document.getElementById("req-tool")!;
  const detailEl = document.getElementById("req-detail")!;
  const reasonEl = document.getElementById("req-reason")!;

  const risk = (req.risk || "medium").toLowerCase();
  riskEl.textContent = risk;
  riskEl.className = `req-risk ${risk}`;
  toolEl.textContent = req.tool || "未知工具";
  detailEl.textContent = req.input_summary || "（无参数）";
  reasonEl.textContent = req.reason || "";
}

async function initRequest(): Promise<void> {
  // 读取最新待确认请求详情
  let req: { request_id: string; tool: string; input_summary: string; risk: string; reason: string } | null = null;
  try {
    req = await invoke("get_pending_request");
  } catch {
    // ignore
  }

  if (req) {
    render(req);
  } else {
    document.getElementById("req-tool")!.textContent = "无待确认请求";
    (document.getElementById("btn-go") as HTMLButtonElement).disabled = true;
  }

  // 「前往 ZCode 确认」：激活 ZCode + 清理请求记录 + 关闭
  document.getElementById("btn-go")!.addEventListener("click", () => {
    if (req) {
      void invoke("clear_pending_request", { requestId: req.request_id }).catch(() => {});
    }
    void invoke("activate_target").catch(() => {});
    void getCurrentWindow().close();
  });

  // 「知道了」：关闭
  document.getElementById("btn-dismiss")!.addEventListener("click", () => {
    void getCurrentWindow().close();
  });

  // 超时自动关闭
  window.setTimeout(() => void getCurrentWindow().close(), AUTO_CLOSE_MS);

  // 用户切回 ZCode 窗口时自动关闭（ZCode 前台可见权限框，弹窗不再需要）
  window.setInterval(async () => {
    try {
      if (await invoke<boolean>("is_zcode_frontmost")) {
        void getCurrentWindow().close();
      }
    } catch {
      // ignore
    }
  }, 2000);
}

(window as any).initRequest = initRequest;

// 自动初始化
function autoInit(): void {
  if ((window as any).__REQUEST_READY__) {
    initRequest().catch((err) => console.error("[zcode-pet] request init error:", err));
  } else {
    setTimeout(autoInit, 200);
  }
}
autoInit();
