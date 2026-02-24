#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use tauri::Manager;

use airprinter::*;
use airprinter::services::{PrinterDetector, AirPrintServer};

// 从 lib.rs 导入命令
use airprinter::commands::{get_printers, share_printer, stop_printer, get_shared_printers, unshare_printer,AppState};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        
        .setup(|app| {
            app.manage(AppState {
                detector: Mutex::new(PrinterDetector::new()),
                server: Mutex::new(AirPrintServer::new()),
            });
            Ok(())
        })
        
        .invoke_handler(tauri::generate_handler![
            get_printers,
            share_printer,
            stop_printer,
            get_shared_printers,
            unshare_printer,
        ])
        
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}