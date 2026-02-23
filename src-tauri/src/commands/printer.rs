use tauri::State;
use std::sync::Mutex;
use crate::models::Printer;
use crate::services::{PrinterDetector, AirPrintServer};

pub struct AppState {
    pub detector: Mutex<PrinterDetector>,
    pub server: Mutex<AirPrintServer>,
}

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
    
    let printer = detector
        .detect_one(&printer_id)
        .ok_or_else(|| "打印机不存在".to_string())?;
    
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