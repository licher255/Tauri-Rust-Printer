import i18n from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import HttpBackend from 'i18next-http-backend';

i18n
  .use(HttpBackend) // 用于加载外部 json 文件
  .use(LanguageDetector) // 用于检测用户浏览器语言
  .init({
    fallbackLng: 'en', // 如果检测不到或加载失败，默认使用英文
    debug: false,      // 生产环境设为 false
    
    // 后端加载配置
    backend: {
      // Vite 开发环境和打包后，public 目录的文件都在根路径下
      loadPath: '/locales/{{lng}}.json', 
    },

    // 语言检测配置
    detection: {
      order: ['navigator', 'querystring', 'localStorage'],
      caches: ['localStorage'], // 记住用户的选择
    },

    interpolation: {
      escapeValue: false, 
    },
    
    // 重要：在资源加载完成前不要初始化完成，防止页面闪烁空文本
    // 对于原生 JS，我们通常手动处理 ready 事件或在 UI 中做加载状态
    initImmediate: true, 
  });

export default i18n;