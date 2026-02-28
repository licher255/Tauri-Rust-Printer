// src-tauri/src/commands/system.rs
use rust_i18n::t;

#[tauri::command]
pub fn set_language(lang: String) -> Result<(), String> {
    // 标准化语言代码 (例如 zh-CN -> zh)
    let base_lang = lang.split('-').next().unwrap_or(&lang);
    
    // 简单的白名单验证
    let valid_locales = ["en", "zh", "zh-CN", "zh-TW", "ja", "fr"];
    
    if !valid_locales.iter().any(|&l| l == base_lang || l == lang) {
        eprintln!("Unsupported language: {}, falling back to en", lang);
        rust_i18n::set_locale("en");
        return Ok(());
    }

    rust_i18n::set_locale(&lang);
    
    // 确保 locales 文件中有 messages.lang_switched 这个 key
    println!("{}", t!("messages.lang_switched", locale = lang));
    
    Ok(())
}