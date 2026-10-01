import { invoke } from "@tauri-apps/api/core";

export interface AppSettings {
  language: string;
  close_to_tray: boolean;
  autostart: boolean;
}

export const settingsApi = {
  getSettings(): Promise<AppSettings> {
    return invoke<AppSettings>("get_settings");
  },
  setCloseToTray(enabled: boolean): Promise<void> {
    return invoke("set_close_to_tray", { enabled });
  },
  setAutostart(enabled: boolean): Promise<void> {
    return invoke("set_autostart", { enabled });
  },
  setLanguage(lang: string): Promise<void> {
    return invoke("set_language", { lang });
  },
};
