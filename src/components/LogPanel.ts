import { logService, LogEntry } from "../services/logService";

export class LogPanel {
  private container: HTMLElement;
  private logArea: HTMLElement;

  constructor(containerId: string) {
    const container = document.getElementById(containerId);
    if (!container) throw new Error(`找不到元素: ${containerId}`);
    this.container = container;
    
    this.render();
    this.logArea = this.container.querySelector("#log-content")!;
    
    // 订阅日志更新
    logService.onUpdate((logs) => this.update(logs));
  }

  private render() {
    this.container.innerHTML = `
      <div class="card bg-base-100 shadow-xl">
        <div class="card-body">
          <div class="flex justify-between items-center mb-4">
            <h2 class="card-title">运行日志</h2>
            <button id="clear-log" class="btn btn-xs btn-ghost">清空</button>
          </div>
          <div id="log-content" class="bg-gray-100 p-4 rounded-lg h-48 overflow-y-auto font-mono text-sm">
            <div class="text-gray-500">等待操作...</div>
          </div>
        </div>
      </div>
    `;

    // 绑定清空按钮
    this.container.querySelector("#clear-log")?.addEventListener("click", () => {
      logService.clear();
    });
  }

  private update(logs: LogEntry[]) {
    if (logs.length === 0) {
      this.logArea.innerHTML = '<div class="text-gray-500">等待操作...</div>';
      return;
    }

    this.logArea.innerHTML = logs.map(log => {
      const colorClass = {
        info: "text-blue-600",
        success: "text-green-600",
        error: "text-red-600",
        warning: "text-yellow-600"
      }[log.level];

      return `
        <div class="mb-1">
          <span class="text-gray-400">[${log.time}]</span>
          <span class="${colorClass}">${log.message}</span>
        </div>
      `;
    }).join("");

    // 自动滚动到底部
    this.logArea.scrollTop = this.logArea.scrollHeight;
  }
}