// Windows queue bridge: DNS-SD discovery and IPP transport.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::Manager;

use airprinter::services::{AirPrintServer, PrinterDetector};
use airprinter::*;

// 导入命令
use airprinter::commands::{
    enable_lan_access, get_printers, get_shared_printers, set_language, share_printer,
    stop_printer, unshare_printer, AppState,
};

fn main() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║              🖨️  AirPrinter 启动中...                    ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let preferences = app.path().app_config_dir()?.join("shared-printers.json");
            app.manage(AppState {
                detector: Mutex::new(PrinterDetector::new()),
                server: Mutex::new(
                    AirPrintServer::with_preferences(preferences).map_err(std::io::Error::other)?,
                ),
            });

            let handle = app.handle().clone();
            thread::spawn(move || loop {
                let state = handle.state::<AppState>();
                let detected = match state.detector.lock() {
                    Ok(detector) => detector.detect(),
                    Err(error) => Err(error.to_string()),
                };
                match detected {
                    Ok(printers) => {
                        if let Ok(mut server) = state.server.lock() {
                            for error in server.reconcile(&printers) {
                                eprintln!("[AirPrinter] 共享同步失败: {error}");
                            }
                        }
                    }
                    Err(error) => eprintln!("[AirPrinter] 检测 Windows 打印机失败: {error}"),
                }
                thread::sleep(Duration::from_secs(30));
            });

            println!(
                "✅ 后端初始化完成，当前语言: {}",
                rust_i18n::locale().to_string()
            );
            println!("");
            println!("📋 AirPrint 服务发现机制：");
            println!("   • _ipp._tcp (端口 631)              - 基础 IPP 服务");
            println!("   • _printer._tcp (端口 0)            - RFC 6763 Flagship Naming");
            println!("   • _universal._sub._ipp._tcp        - AirPrint 发现子类型");
            println!("");
            println!("⚠️  使用提示：");
            println!("   1. 确保手机和电脑在可互通的同一局域网，电脑可使用网线");
            println!("   2. 点击“允许局域网访问”，为本程序放行 mDNS 和 IPP");
            println!("   3. 路由器不能开启 'AP隔离' / '客户端隔离'");
            println!("");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_printers,
            share_printer,
            stop_printer,
            get_shared_printers,
            unshare_printer,
            enable_lan_access,
            set_language,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
