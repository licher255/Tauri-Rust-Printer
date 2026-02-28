import { logService, LogEntry } from "../services/logService";
import i18n from '../i18n';

export class LogPanel {
  private container: HTMLElement;
  private logArea: HTMLElement;
  private clearBtn: HTMLButtonElement | null;
  private titleEl: HTMLElement | null;
  
  // 定义事件处理函数，用于移除监听
  private handleLanguageChange = () => {
    this.updateStaticLabels();
    // 如果当前没有日志，需要重新渲染空状态提示
    const currentLogs = logService.getLogs(); // 假设 logService 有获取当前日志的方法，如果没有，可以通过内部变量或重新触发 update
    if (!currentLogs || currentLogs.length === 0) {
       this.update([]);
    }
  };

  constructor(containerId: string) {
    const container = document.getElementById(containerId);
    if (!container) throw new Error(`找不到元素: ${containerId}`);
    this.container = container;
    
    this.render();
    
    // 获取引用
    this.logArea = this.container.querySelector("#log-content")!;
    this.clearBtn = this.container.querySelector("#clear-log");
    this.titleEl = this.container.querySelector("#lp-title");

    // 绑定清空按钮事件
    this.clearBtn?.addEventListener("click", () => {
      logService.clear();
    });

    // 监听语言变化
    i18n.on('languageChanged', this.handleLanguageChange);
    
    // 初始化静态文本
    this.updateStaticLabels();

    // 订阅日志更新
    logService.onUpdate((logs) => this.update(logs));
  }

  // 销毁方法，用于清理事件监听
  public destroy() {
    i18n.off('languageChanged', this.handleLanguageChange);
  }

  private render() {
    // 添加 ID 以便后续通过 JS 更新文本
    this.container.innerHTML = `
      <div class="card bg-base-100 shadow-xl">
        <div class="card-body">
          <div class="flex justify-between items-center mb-4">
            <h2 id="lp-title" class="card-title">运行日志</h2>
            <button id="clear-log" class="btn btn-xs btn-ghost">清空</button>
          </div>
          <div id="log-content" class="bg-gray-100 dark:bg-gray-800 p-4 rounded-lg h-48 overflow-y-auto font-mono text-sm">
            <div class="text-gray-500">等待操作...</div>
          </div>
        </div>
      </div>
    `;
  }

  // 专门更新静态标签（标题、按钮）
  private updateStaticLabels() {
    if (this.titleEl) {
      this.titleEl.textContent = i18n.t('logs.title');
    }
    if (this.clearBtn) {
      this.clearBtn.textContent = i18n.t('logs.clear');
    }
  }

  private update(logs: LogEntry[]) {
    if (logs.length === 0) {
      // 使用翻译后的空状态提示
      this.logArea.innerHTML = `<div class="text-gray-500">${i18n.t('logs.waiting')}</div>`;
      return;
    }

    this.logArea.innerHTML = logs.map(log => {
      const colorClass = {
        info: "text-blue-600 dark:text-blue-400",
        success: "text-green-600 dark:text-green-400",
        error: "text-red-600 dark:text-red-400",
        warning: "text-yellow-600 dark:text-yellow-400"
      }[log.level] || "text-gray-600";

      // 注意：log.message 通常来自后端或系统，如果是固定错误码，最好也在后端或前端映射为 i18n key
      // 这里假设 message 已经是可读文本，或者由后端负责翻译。
      // 如果 message 是硬编码的英文错误，你可能需要一个映射表将其转换为 i18n key
      
      return `
        <div class="mb-1 break-words">
          <span class="text-gray-400 opacity-70 text-xs mr-2">[${log.time}]</span>
          <span class="${colorClass}">${this.escapeHtml(log.message)}</span>
        </div>
      `;
    }).join("");

    // 自动滚动到底部
    this.logArea.scrollTop = this.logArea.scrollHeight;
  }

  // 简单的 HTML 转义，防止日志内容包含恶意脚本
  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}