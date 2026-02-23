import { invoke } from "@tauri-apps/api/core";
import "./styles.css";
import i18n from "./i18n";

// 简单的 i18n 实现
const t = (key: string): string => {
  return i18n.t(key) as string;
};

// 更新页面文本
const updateContent = () => {
  document.querySelectorAll("[data-i18n]").forEach((el) => {
    const key = el.getAttribute("data-i18n");
    if (key) {
      el.textContent = t(key);
    }
  });
};

// 语言切换
const setupLanguageSwitch = () => {
  const select = document.getElementById("lang-select") as HTMLSelectElement;
  if (select) {
    select.addEventListener("change", (e) => {
      const lang = (e.target as HTMLSelectElement).value;
      i18n.changeLanguage(lang).then(() => {
        updateContent();
      });
    });
  }
};

// 测试调用 Rust
const testRustCall = async () => {
  try {
    const response = await invoke("greet", { name: "AirPrinter" });
    console.log("Rust says:", response);
    
    const resultEl = document.getElementById("rust-result");
    if (resultEl) {
      resultEl.textContent = response as string;
    }
  } catch (error) {
    console.error("Error calling Rust:", error);
  }
};

// 初始化
document.addEventListener("DOMContentLoaded", () => {
  updateContent();
  setupLanguageSwitch();
  testRustCall();
});