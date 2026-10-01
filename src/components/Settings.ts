import i18n from "../i18n";
import { logService } from "../services/logService";
import { settingsApi } from "../services/settingsService";

export class Settings {
  private container: HTMLElement;

  private handleLanguageChange = () => {
    this.renderStaticLabels();
  };

  constructor(containerId: string) {
    const container = document.getElementById(containerId);
    if (!container) throw new Error(`找不到元素: ${containerId}`);
    this.container = container;

    this.render();
    i18n.on("languageChanged", this.handleLanguageChange);
    this.bindEvents();
    this.load();
  }

  public destroy() {
    i18n.off("languageChanged", this.handleLanguageChange);
  }

  private render() {
    this.container.innerHTML = `
      <section class="panel">
        <div class="panel-header">
          <div class="panel-title-group">
            <span class="panel-icon"><i class="fa-solid fa-gear"></i></span>
            <h2 id="settings-title" class="panel-title"></h2>
          </div>
        </div>
        <div class="settings-group">
          <div class="settings-row">
            <div class="settings-label">
              <i class="fa-solid fa-globe"></i>
              <span id="settings-language-label" class="settings-label-text"></span>
            </div>
            <select id="settings-language" class="select-apple">
              <option value="en">English</option>
              <option value="zh">中文</option>
            </select>
          </div>
          <div class="settings-row">
            <div class="settings-label">
              <i class="fa-solid fa-window-restore"></i>
              <span>
                <span id="settings-tray-label" class="settings-label-text"></span>
                <span id="settings-tray-hint" class="hint block"></span>
              </span>
            </div>
            <input id="settings-close-to-tray" type="checkbox" class="toggle toggle-primary" />
          </div>
          <div class="settings-row">
            <div class="settings-label">
              <i class="fa-solid fa-power-off"></i>
              <span>
                <span id="settings-autostart-label" class="settings-label-text"></span>
                <span id="settings-autostart-hint" class="hint block"></span>
              </span>
            </div>
            <input id="settings-autostart" type="checkbox" class="toggle toggle-primary" />
          </div>
        </div>
      </section>
    `;
    this.renderStaticLabels();
  }

  private renderStaticLabels() {
    const set = (id: string, key: string) => {
      const el = this.container.querySelector<HTMLElement>(`#${id}`);
      if (el) el.textContent = i18n.t(key);
    };
    set("settings-title", "settings.title");
    set("settings-language-label", "settings.language");
    set("settings-tray-label", "settings.close_to_tray");
    set("settings-tray-hint", "settings.close_to_tray_hint");
    set("settings-autostart-label", "settings.autostart");
    set("settings-autostart-hint", "settings.autostart_hint");

    const select = this.container.querySelector<HTMLSelectElement>("#settings-language");
    if (select) select.value = (i18n.language || "en").split("-")[0];
  }

  private bindEvents() {
    const language = this.container.querySelector<HTMLSelectElement>("#settings-language")!;
    const closeToTray = this.container.querySelector<HTMLInputElement>("#settings-close-to-tray")!;
    const autostart = this.container.querySelector<HTMLInputElement>("#settings-autostart")!;

    language.addEventListener("change", async () => {
      const lang = language.value;
      try {
        await i18n.changeLanguage(lang);
        document.documentElement.lang = lang;
        await settingsApi.setLanguage(lang);
      } catch (error) {
        logService.add(i18n.t("errors.settings_failed", { error: String(error) }), "error");
      }
    });

    closeToTray.addEventListener("change", async () => {
      closeToTray.disabled = true;
      try {
        await settingsApi.setCloseToTray(closeToTray.checked);
      } catch (error) {
        closeToTray.checked = !closeToTray.checked;
        logService.add(i18n.t("errors.settings_failed", { error: String(error) }), "error");
      } finally {
        closeToTray.disabled = false;
      }
    });

    autostart.addEventListener("change", async () => {
      autostart.disabled = true;
      try {
        await settingsApi.setAutostart(autostart.checked);
      } catch (error) {
        autostart.checked = !autostart.checked;
        logService.add(i18n.t("errors.settings_failed", { error: String(error) }), "error");
      } finally {
        autostart.disabled = false;
      }
    });
  }

  private async load() {
    try {
      const settings = await settingsApi.getSettings();
      this.container.querySelector<HTMLInputElement>("#settings-close-to-tray")!.checked =
        settings.close_to_tray;
      this.container.querySelector<HTMLInputElement>("#settings-autostart")!.checked =
        settings.autostart;
      // 后端保存的语言优先于浏览器检测
      if (settings.language && settings.language !== (i18n.language || "").split("-")[0]) {
        await i18n.changeLanguage(settings.language);
        document.documentElement.lang = settings.language;
      }
    } catch (error) {
      logService.add(i18n.t("errors.settings_failed", { error: String(error) }), "error");
    }
  }
}
