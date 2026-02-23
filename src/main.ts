import "./styles.css";
import i18n from "./i18n";
import { PrinterList } from "./components/PrinterList";
import { LogPanel } from "./components/LogPanel";

// 初始化
document.addEventListener("DOMContentLoaded", () => {
  // 初始化组件
  new PrinterList("printer-list-container");
  new LogPanel("log-panel-container");

  // 语言切换
  const langSelect = document.getElementById("lang-select") as HTMLSelectElement;
  if (langSelect) {
    langSelect.addEventListener("change", (e) => {
      i18n.changeLanguage((e.target as HTMLSelectElement).value);
    });
  }
});