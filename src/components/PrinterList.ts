import { printerApi, Printer } from "../services/printerService";

export class PrinterList {
  private container: HTMLElement;
  private listContainer: HTMLElement;
  private refreshBtn: HTMLButtonElement;
  private printers: Printer[] = [];

  constructor(containerId: string) {
    const container = document.getElementById(containerId);
    if (!container) throw new Error(`找不到元素: ${containerId}`);
    this.container = container;

    this.render();
    this.listContainer = this.container.querySelector("#printer-items")!;
    this.refreshBtn = this.container.querySelector("#refresh-btn")!;

    this.bindEvents();
    this.load(); // 自动加载
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
      this.printers = await printerApi.getList();
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

    this.listContainer.innerHTML = this.printers.map(p => `
      <div class="flex items-center justify-between p-3 bg-base-200 rounded-lg">
        <div class="flex items-center gap-3">
          <span class="text-2xl">🖨️</span>
          <div>
            <div class="font-bold">${p.name}</div>
            <div class="text-xs text-gray-500">ID: ${p.id}</div>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <span class="badge ${p.status === 'online' ? 'badge-success' : 'badge-error'}">
            ${p.status === 'online' ? '在线' : '离线'}
          </span>
          <button 
            class="btn btn-xs btn-primary share-btn" 
            data-id="${p.id}"
            ${p.status !== 'online' ? 'disabled' : ''}
          >
            共享
          </button>
        </div>
      </div>
    `).join("");

    // 绑定共享按钮
    this.listContainer.querySelectorAll(".share-btn").forEach(btn => {
      btn.addEventListener("click", (e) => {
        const id = (e.target as HTMLButtonElement).dataset.id!;
        this.handleShare(id);
      });
    });
  }

  private async handleShare(printerId: string) {
    const btn = this.listContainer.querySelector(`[data-id="${printerId}"]`) as HTMLButtonElement;
    btn.disabled = true;
    btn.textContent = "共享中...";

    try {
      await printerApi.share(printerId);
      btn.textContent = "已共享";
      btn.classList.remove("btn-primary");
      btn.classList.add("btn-success");
    } catch (error) {
      btn.disabled = false;
      btn.textContent = "共享";
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