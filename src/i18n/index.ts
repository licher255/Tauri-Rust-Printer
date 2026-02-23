import i18n from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';

// 直接嵌入翻译
const resources = {
  en: {
    translation: {
      welcome: "Welcome to AirPrinter",
      description: "Share your USB printer to AirPrint",
      status: "Status",
      ready: "Ready",
    }
  },
  zh: {
    translation: {
      welcome: "欢迎使用 AirPrinter",
      description: "将您的 USB 打印机共享为 AirPrint",
      status: "状态",
      ready: "就绪",
    }
  }
};

i18n
  .use(LanguageDetector)
  .init({
    resources,
    fallbackLng: 'en',
    interpolation: {
      escapeValue: false,
    },
  });

export default i18n;