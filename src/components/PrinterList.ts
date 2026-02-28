import { printerApi, Printer } from "../services/printerService";
import i18n from '../i18n';

export class PrinterList {
  private container: HTMLElement;
  private listContainer: HTMLElement;
  private refreshBtn: HTMLButtonElement;
  private printers: Printer[] = [];
  private sharedPrinterIds: Set<string> = new Set();
  
  // 定义事件处理函数，以便后续移除监听
  private handleLanguageChange = () => {
    this.renderStaticLabels();
    this.renderList();
  };

  constructor(containerId: string) {
    const container = document.getElementById(containerId);
    if (!container) throw new Error(`找不到元素: ${containerId}`);
    this.container = container;

    this.render();
    this.listContainer = this.container.querySelector("#printer-items")!;
    this.refreshBtn = this.container.querySelector("#refresh-btn")!;

    this.bindEvents();
    
    // ✅ 修复：直接绑定，不需要接收返回值
    i18n.on('languageChanged', this.handleLanguageChange);

    this.load();
  }

  // ✅ 修复：使用 off 方法移除监听
  public destroy() {
    i18n.off('languageChanged', this.handleLanguageChange);
  }

  private render() {
    this.container.innerHTML = `
      <div class="card bg-base-100 shadow-xl mb-6">
        <div class="card-body">
          <div class="flex justify-between items-center mb-4">
            <h2 id="pl-title" class="card-title">打印机列表</h2>
            <button id="refresh-btn" class="btn btn-primary btn-sm">
              <span class="loading loading-spinner loading-xs hidden" id="loading"></span>
              <span id="pl-refresh-text">刷新</span>
            </button>
          </div>
          <div id="printer-items" class="space-y-2">
            <div class="text-gray-500" id="pl-loading-text">加载中...</div>
          </div>
        </div>
      </div>
    `;
    
    this.renderStaticLabels();
  }

  private renderStaticLabels() {
    const titleEl = document.getElementById('pl-title');
    const refreshTextEl = document.getElementById('pl-refresh-text');
    const loadingTextEl = document.getElementById('pl-loading-text');

    if (titleEl) titleEl.textContent = i18n.t('printers.title');
    if (refreshTextEl) refreshTextEl.textContent = i18n.t('actions.refresh');
    if (loadingTextEl) loadingTextEl.textContent = i18n.t('common.loading');
  }

  private bindEvents() {
    this.refreshBtn.addEventListener("click", () => this.load());
  }

  async load() {
    this.setLoading(true);
    const loadingTextEl = document.getElementById('pl-loading-text');
    if(loadingTextEl) loadingTextEl.textContent = i18n.t('common.loading');

    try {
      const [printers, shared] = await Promise.all([
        printerApi.getList(),
        printerApi.getSharedList()
      ]);
      
      this.printers = printers;
      this.sharedPrinterIds = new Set(shared.map(p => p.id));
      this.renderList();
    } catch (error) {
      const errorMsg = i18n.t('errors.load_failed', { error: String(error) });
      this.listContainer.innerHTML = `<div class="text-error">${errorMsg}</div>`;
    } finally {
      this.setLoading(false);
    }
  }

  private renderList() {
    if (this.printers.length === 0) {
      this.listContainer.innerHTML = `<div class="text-gray-500">${i18n.t('printers.no_printers')}</div>`;
      return;
    }

    this.listContainer.innerHTML = this.printers.map(p => {
      const statusStr = (p.status || '').toString().toLowerCase();
      const isOnline = statusStr === 'online';
      const isShared = this.sharedPrinterIds.has(p.id);
      
      const statusKey = isOnline ? 'status.online' : 'status.offline';
      const statusText = i18n.t(statusKey);
      
      const badgeClass = isOnline ? 'badge-success' : 'badge-error';
      
      const shareKey = isShared ? 'actions.stop_sharing' : 'actions.share';
      const btnText = i18n.t(shareKey);
      
      const btnClass = isShared ? 'btn-error' : 'btn-primary';
      const btnDisabled = !isOnline && !isShared;
      
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

    this.listContainer.querySelectorAll(".share-btn").forEach(btn => {
      btn.addEventListener("click", (e) => {
        const target = e.target as HTMLButtonElement;
        const id = target.dataset.id!;
        const isShared = target.dataset.shared === "true";
        this.handleShare(id, isShared, target);
      });
    });
  }

  private async handleShare(printerId: string, isShared: boolean, btn: HTMLButtonElement) {
    btn.disabled = true;
    
    const processingKey = isShared ? 'actions.stopping' : 'actions.sharing';
    btn.textContent = i18n.t(processingKey);

    try {
      if (isShared) {
        await printerApi.unshare(printerId);
        this.sharedPrinterIds.delete(printerId);
        alert(`✅ ${i18n.t('messages.stop_success')}`);
      } else {
        await printerApi.share(printerId);
        this.sharedPrinterIds.add(printerId);
        alert(`✅ ${i18n.t('messages.share_success')}`);
      }
      
      this.renderList();
      
    } catch (error) {
      const errorMsg = i18n.t('errors.operation_failed', { error: String(error) });
      alert(`❌ ${errorMsg}`);
      
      const restoreKey = isShared ? 'actions.stop_sharing' : 'actions.share';
      btn.textContent = i18n.t(restoreKey);
      btn.disabled = false;
    }
  }

  private setLoading(loading: boolean) {
    const spinner = this.container.querySelector("#loading");
    if (spinner) {
      spinner.classList.toggle("hidden", !loading);
    }
    this.refreshBtn.disabled = loading;
    
    if (loading && this.printers.length === 0) {
       const loadingTextEl = document.getElementById('pl-loading-text');
       if(loadingTextEl) loadingTextEl.textContent = i18n.t('common.loading');
    }
  }
}