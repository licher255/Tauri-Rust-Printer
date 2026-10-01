import "@fortawesome/fontawesome-free/css/all.min.css";
import "./styles.css";
import i18n from "./i18n";
import { PrinterList } from "./components/PrinterList";
import { LogPanel } from "./components/LogPanel";
import { Settings } from "./components/Settings";
import { setupTitlebar } from "./components/Titlebar";

/**
 * 更新页面上所有标记了 data-i18n / data-i18n-title 的元素
 */
const updatePageTranslations = () => {
  document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    const key = el.getAttribute("data-i18n");
    if (key) {
      const translation = i18n.t(key);
      if (translation && translation !== key) {
        el.textContent = translation;
      }
    }
  });
  document.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((el) => {
    const key = el.getAttribute("data-i18n-title");
    if (key) {
      const translation = i18n.t(key);
      if (translation && translation !== key) {
        el.setAttribute("title", translation);
      }
    }
  });

  const appTitle = i18n.t("app.title");
  if (appTitle && appTitle !== "app.title") {
    document.title = appTitle.replace(/🖨️\s*/, "");
  }
};

document.addEventListener("DOMContentLoaded", async () => {
  if (!i18n.isInitialized) {
    await new Promise<void>((resolve) => {
      i18n.on("initialized", () => resolve());
    });
  }

  setupTitlebar();
  new PrinterList("printer-list-container");
  new Settings("settings-container");
  new LogPanel("log-panel-container");
  updatePageTranslations();

  // 全局监听（Settings 组件或浏览器检测切换语言时同步静态文案）
  i18n.on("languageChanged", (lng) => {
    updatePageTranslations();
    document.documentElement.lang = lng;
  });
});
