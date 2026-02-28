// src-tauri/src/main.rs

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use tauri::Manager;

use airprinter::*;
use airprinter::services::{PrinterDetector, AirPrintServer};

// 导入命令
use airprinter::commands::{
    get_printers, 
    share_printer, 
    stop_printer, 
    get_shared_printers, 
    unshare_printer, 
    set_language, // 确保这里引入了
    AppState
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        
        .setup(|app| {
            app.manage(AppState {
                detector: Mutex::new(PrinterDetector::new()),
                server: Mutex::new(AirPrintServer::new()),
            });
            
            // 👇 修复：使用 .to_string() 或 {:?}
            // 方法 A: 转为 String (推荐)
            println!("Backend initialized with locale: {}", rust_i18n::locale().to_string());
            
            // 或者 方法 B: 使用调试格式
            // println!("Backend initialized with locale: {:?}", rust_i18n::locale());
            
            Ok(())
        })
        
        .invoke_handler(tauri::generate_handler![
            get_printers,
            share_printer,
            stop_printer,
            get_shared_printers,
            unshare_printer,
            set_language,
        ])
        
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}