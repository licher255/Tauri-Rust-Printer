// src-tauri/src/commands/system.rs
use rust_i18n::t;

#[cfg(target_os = "windows")]
fn firewall_script(program: &std::path::Path) -> String {
    let literal = program.to_string_lossy().replace('\'', "''");
    include_str!("../../../scripts/enable_firewall.ps1").replace(
        "param([Parameter(Mandatory=$true)][string]$Program)",
        &format!("$Program = '{literal}'"),
    )
}

#[tauri::command]
pub async fn enable_lan_access() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            use std::os::windows::process::CommandExt;
            let program = std::env::current_exe().map_err(|e| e.to_string())?;
            let elevate = r#"$ErrorActionPreference='Stop'; $encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($env:AIRPRINTER_FIREWALL_SOURCE)); $process=Start-Process -FilePath powershell.exe -ArgumentList ('-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand ' + $encoded) -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $process.ExitCode"#;
            let output = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", elevate])
                .env("AIRPRINTER_FIREWALL_SOURCE", firewall_script(&program))
                .creation_flags(0x08000000)
                .output()
                .map_err(|e| e.to_string())?;
            if output.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "Windows did not enable LAN access (administrator approval may be required): {}",
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Windows firewall setup requires Windows".into())
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn elevated_script_quotes_executable_without_a_writable_script_file() {
        let source = firewall_script(std::path::Path::new(r"C:\O'Reilly\AirPrinter.exe"));
        assert!(source.contains(r"$Program = 'C:\O''Reilly\AirPrinter.exe'"));
        assert!(!source.contains("[Parameter(Mandatory=$true)][string]$Program"));
    }
}

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
