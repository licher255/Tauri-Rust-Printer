import "./styles.css";
import i18n from "./i18n";
import { PrinterList } from "./components/PrinterList";
import { LogPanel } from "./components/LogPanel";
import { invoke } from "@tauri-apps/api/core";

/**
 * ============================================================================
 * AirPrinter 初始化注意事项
 * ============================================================================
 * 
 * 1. 网络环境
 *    - 手机和电脑必须在同一 Wi-Fi 网络下
 *    - 路由器不能开启 "AP隔离" / "客户端隔离"
 * 
 * 2. 防火墙设置（Windows）
 *    以管理员身份运行 PowerShell 执行：
 *    
 *    netsh advfirewall firewall add rule name="mDNS AirPrint" dir=in action=allow protocol=udp localport=5353
 *    netsh advfirewall firewall add rule name="IPP Server" dir=in action=allow protocol=tcp localport=631
 * 
 * 3. AirPrint 服务发现
 *    本应用注册 3 个 mDNS 服务（必须使用相同实例名称）：
 *    - _ipp._tcp (端口 631)
 *    - _printer._tcp (端口 0, RFC 6763)
 *    - _print._sub._ipp._tcp (端口 631, IPP Everywhere™)
 * 
 * 4. 故障排除
 *    - Discovery App 能发现但 iOS 系统打印无法发现：
 *      检查是否包含 ipp-features-supported = ipp-everywhere
 *    - 完全无法发现：检查防火墙和路由器 AP 隔离设置
 * 
 * 参考文档：README.md 中的 "使用注意事项" 章节
 * ============================================================================
 */

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

// 专门用于同步语言到后端的函数
const syncLanguageToBackend = async (lang: string) => {
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

// 打印初始化提示信息
const printInitNotes = () => {
  console.log('%c🖨️ AirPrinter 初始化完成', 'color: #4CAF50; font-size: 16px; font-weight: bold;');
  console.log('%c═══════════════════════════════════════════════════════════', 'color: #2196F3;');
  console.log('%c使用注意事项：', 'color: #FF9800; font-weight: bold;');
  console.log('  1. 确保手机和电脑在同一 Wi-Fi 网络下');
  console.log('  2. 检查 Windows 防火墙是否放行 UDP 5353 和 TCP 631');
  console.log('  3. 路由器不能开启 "AP隔离" / "客户端隔离"');
  console.log('  4. 共享打印机后，iOS 应在 5-10 秒内发现打印机');
  console.log('%c═══════════════════════════════════════════════════════════', 'color: #2196F3;');
  console.log('详细文档请查看 README.md 中的 "使用注意事项" 章节');
  console.log('故障排查指南: https://github.com/yourusername/airprinter#故障排除');
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

  // 打印初始化提示
  printInitNotes();

  const langSelect = document.getElementById("lang-select") as HTMLSelectElement;
  
  if (langSelect) {
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
  });
});
