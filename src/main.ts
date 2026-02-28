import "./styles.css";
import i18n from "./i18n";
import { PrinterList } from "./components/PrinterList";
import { LogPanel } from "./components/LogPanel";
import { invoke } from "@tauri-apps/api/core"; // 确保已导入

/**
 * 更新页面上所有标记了 data-i18n 的元素
 */
const updatePageTranslations = () => {
  document.querySelectorAll<HTMLElement>('[data-i18n]').forEach((el) => {
    const key = el.getAttribute('data-i18n');
    if (key) {
      const translation = i18n.t(key);
      if (translation && translation !== key) {
        el.textContent = translation;
      }
    }
  });

  const appTitle = i18n.t('app.title');
  if (appTitle) {
    document.title = appTitle.replace(/🖨️\s*/, '');
  }
};

// 👇 新增：专门用于同步语言到后端的函数
const syncLanguageToBackend = async (lang: string) => {
  // 👇 防御性检查：如果 lang 为空，默认为 'en'
  if (!lang || lang.trim() === '') {
    console.warn('⚠️ Language is empty, defaulting to "en"');
    lang = 'en';
  }
  
  try {
    await invoke("set_language", { lang });
    console.log(`✅ Backend language synced to: ${lang}`);
  } catch (err) {
    console.error(`❌ Failed to sync backend language: ${err}`);
  }
};

document.addEventListener("DOMContentLoaded", async () => {
  if (!i18n.isInitialized) {
    await new Promise<void>((resolve) => {
      i18n.on('initialized', () => resolve());
    });
  }

  new PrinterList("printer-list-container");
  new LogPanel("log-panel-container");
  updatePageTranslations();

  const langSelect = document.getElementById("lang-select") as HTMLSelectElement;
  
  if (langSelect) {
    // 👇 确保初始值不为空
    const currentLang = i18n.language || 'en';
    langSelect.value = currentLang; 

    // 初始化同步
    try {
      await invoke("set_language", { lang: currentLang });
    } catch (e) {
      console.warn("Backend sync failed on init", e);
    }

    langSelect.addEventListener("change", async (e) => {
      const newLang = (e.target as HTMLSelectElement).value;
        
        // 👇 再次检查
      if (!newLang) return; 

      try {
        await i18n.changeLanguage(newLang);
        await syncLanguageToBackend(newLang);
        updatePageTranslations();
        document.documentElement.lang = newLang;
      } catch (err) {
        console.error("Failed to change language:", err);
      }
    });
  }

  // 全局监听 (防止其他代码调用 i18n.changeLanguage)
  i18n.on('languageChanged', (lng) => {
    if (langSelect) langSelect.value = lng;
    updatePageTranslations();
    // 👇 如果是通过代码触发的变化，也需要同步后端
    // 注意避免死循环，通常上面的 change 事件已经处理了用户交互
    // 这里可以加一个标志位，或者确信只有用户操作才会触发 change 事件
  });
});