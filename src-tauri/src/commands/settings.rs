use super::AppState;
use crate::models::AppSettings;
use crate::services::settings::apply_autostart;
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<AppSettings, String> {
    let store = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(store.get())
}

#[tauri::command]
pub fn set_close_to_tray(enabled: bool, state: State<AppState>) -> Result<(), String> {
    let mut store = state.settings.lock().map_err(|e| e.to_string())?;
    store.update(|s| s.close_to_tray = enabled)?;
    Ok(())
}

#[tauri::command]
pub fn set_autostart(enabled: bool, state: State<AppState>) -> Result<(), String> {
    apply_autostart(enabled)?;
    let mut store = state.settings.lock().map_err(|e| e.to_string())?;
    store.update(|s| s.autostart = enabled)?;
    Ok(())
}
