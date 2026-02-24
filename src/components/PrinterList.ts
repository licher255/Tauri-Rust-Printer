import { printerApi, Printer } from "../services/printerService";

export class PrinterList {
  private container: HTMLElement;
  private listContainer: HTMLElement;
  private refreshBtn: HTMLButtonElement;
  private printers: Printer[] = [];
  private sharedPrinterIds: Set<string> = new Set();

  constructor(containerId: string) {
    const container = document.getElementById(containerId);
    if (!container) throw new Error(`找不到元素: ${containerId}`);
    this.container = container;

    this.render();
    this.listContainer = this.container.querySelector("#printer-items")!;
    this.refreshBtn = this.container.querySelector("#refresh-btn")!;

    this.bindEvents();
    this.load(); // 自动加载（里面已经调用 loadSharedPrinters）
  }

  private render() {
    this.container.innerHTML = `
      <div class="card bg-base-100 shadow-xl mb-6">
        <div class="card-body">
          <div class="flex justify-between items-center mb-4">
            <h2 class="card-title">打印机列表</h2>
            <button id="refresh-btn" class="btn btn-primary btn-sm">
              <span class="loading loading-spinner loading-xs hidden" id="loading"></span>
              刷新
            </button>
          </div>
          <div id="printer-items" class="space-y-2">
            <div class="text-gray-500">加载中...</div>
          </div>
        </div>
      </div>
    `;
  }

  private bindEvents() {
    this.refreshBtn.addEventListener("click", () => this.load());
  }

  async load() {
    this.setLoading(true);
    try {
      // 同时获取打印机列表和共享状态
      const [printers, shared] = await Promise.all([
        printerApi.getList(),
        printerApi.getSharedList()
      ]);
      
      this.printers = printers;
      this.sharedPrinterIds = new Set(shared.map(p => p.id));
      this.renderList();
    } catch (error) {
      this.listContainer.innerHTML = `<div class="text-error">加载失败: ${error}</div>`;
    } finally {
      this.setLoading(false);
    }
  }

  private renderList() {
    if (this.printers.length === 0) {
      this.listContainer.innerHTML = '<div class="text-gray-500">未发现打印机</div>';
      return;
    }

    this.listContainer.innerHTML = this.printers.map(p => {
      const statusStr = (p.status || '').toString().toLowerCase();
      const isOnline = statusStr === 'online';
      const isShared = this.sharedPrinterIds.has(p.id);  // 检查是否已共享
      
      const statusText = isOnline ? '在线' : '离线';
      const badgeClass = isOnline ? 'badge-success' : 'badge-error';
      
      // 根据共享状态显示不同按钮
      const btnClass = isShared ? 'btn-error' : 'btn-primary';
      const btnText = isShared ? '停止共享' : '共享';
      const btnDisabled = !isOnline && !isShared;  // 离线且未共享时禁用
      
      return `
        <div class="flex items-center justify-between p-3 bg-base-200 rounded-lg">
          <div class="flex items-center gap-3">
            <span class="text-2xl">🖨️</span>
            <div>
              <div class="font-bold">${p.name}</div>
              <div class="text-xs text-gray-500">ID: ${p.id}</div>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <span class="badge ${badgeClass}">${statusText}</span>
            <button 
              class="btn btn-xs ${btnClass} share-btn" 
              data-id="${p.id}"
              data-shared="${isShared}"
              ${btnDisabled ? 'disabled' : ''}
            >
              ${btnText}
            </button>
          </div>
        </div>
      `;
    }).join("");

    // 绑定共享按钮事件
    this.listContainer.querySelectorAll(".share-btn").forEach(btn => {
      btn.addEventListener("click", (e) => {
        const target = e.target as HTMLButtonElement;
        const id = target.dataset.id!;
        const isShared = target.dataset.shared === "true";
        this.handleShare(id, isShared, target);
      });
    });
  }

  // 修改：处理共享/取消共享
  private async handleShare(printerId: string, isShared: boolean, btn: HTMLButtonElement) {
    btn.disabled = true;
    btn.textContent = isShared ? "停止中..." : "共享中...";

    try {
      if (isShared) {
        // 取消共享
        await printerApi.unshare(printerId);
        this.sharedPrinterIds.delete(printerId);
        alert("✅ 已停止共享");
      } else {
        // 开始共享
        const result = await printerApi.share(printerId);
        this.sharedPrinterIds.add(printerId);
        alert(`✅ ${result}`);
      }
      
      // 重新渲染列表更新按钮状态
      this.renderList();
      
    } catch (error) {
      alert(`❌ 操作失败: ${error}`);
      // 恢复原按钮文字
      btn.textContent = isShared ? "停止共享" : "共享";
      btn.disabled = false;
    }
  }

  private setLoading(loading: boolean) {
    const spinner = this.container.querySelector("#loading");
    if (spinner) {
      spinner.classList.toggle("hidden", !loading);
    }
    this.refreshBtn.disabled = loading;
  }
}