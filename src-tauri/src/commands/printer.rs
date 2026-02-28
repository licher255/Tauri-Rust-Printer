// src-tauri/src/commands/printer.rs

use tauri::State;
use crate::models::Printer;
use super::AppState; // 从父模块 (mod.rs) 导入 AppState
use rust_i18n::t;    // 引入翻译宏

#[tauri::command]
pub fn get_printers(state: State<AppState>) -> Result<Vec<Printer>, String> {
    let detector = state.detector.lock().map_err(|e| e.to_string())?;
    Ok(detector.detect())
}

#[tauri::command]
pub fn share_printer(
    printer_id: String,
    state: State<AppState>
) -> Result<String, String> {
    let detector = state.detector.lock().map_err(|e| e.to_string())?;
    
    // 使用 t! 宏替换硬编码中文
    let printer = detector
        .detect_one(&printer_id)
        .ok_or_else(|| t!("errors.printer_not_found", id = printer_id).to_string())?;
    
    let mut server = state.server.lock().map_err(|e| e.to_string())?;
    server.share(printer)
}

#[tauri::command]
pub fn stop_printer(
    printer_id: String,
    state: State<AppState>
) -> Result<(), String> {
    let mut server = state.server.lock().map_err(|e| e.to_string())?;
    server.stop(&printer_id)
}

#[tauri::command]
pub fn get_shared_printers(state: State<AppState>) -> Result<Vec<Printer>, String> {
    let server = state.server.lock().map_err(|e| e.to_string())?;
    Ok(server.get_shared_printers().into_iter().cloned().collect())
}

#[tauri::command]
pub fn unshare_printer(
    printer_id: String,
    state: State<AppState>
) -> Result<(), String> {
    let mut server = state.server.lock().map_err(|e| e.to_string())?;
    server.stop(&printer_id)
}