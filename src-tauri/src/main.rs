// src-tauri/src/main.rs
// 
// AirPrinter 后端入口
// 
// 注意事项：
// 1. 本应用实现了完整的 IPP Everywhere™ v1.1 规范
// 2. 注册 3 个 mDNS 服务：_ipp._tcp、_printer._tcp(端口0)、_print._sub._ipp._tcp
// 3. 所有服务必须使用相同的实例名称（规范要求）
// 4. IPP 响应必须包含 ipp-features-supported = ipp-everywhere（iOS 必需）
// 
// 故障排除：
// - Discovery App 能发现但 iOS 系统打印无法发现：检查 IPP 属性是否完整
// - 完全无法发现：检查防火墙（UDP 5353, TCP 631）和路由器 AP 隔离

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
    set_language,
    AppState
};

fn main() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║              🖨️  AirPrinter 启动中...                    ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        
        .setup(|app| {
            app.manage(AppState {
                detector: Mutex::new(PrinterDetector::new()),
                server: Mutex::new(AirPrintServer::new()),
            });
            
            println!("✅ 后端初始化完成，当前语言: {}", rust_i18n::locale().to_string());
            println!("");
            println!("📋 AirPrint 服务发现机制：");
            println!("   • _ipp._tcp (端口 631)              - 基础 IPP 服务");
            println!("   • _printer._tcp (端口 0)            - RFC 6763 Flagship Naming");
            println!("   • _print._sub._ipp._tcp (端口 631)  - IPP Everywhere™ 子类型");
            println!("");
            println!("⚠️  使用提示：");
            println!("   1. 确保手机和电脑在同一 Wi-Fi 网络");
            println!("   2. 检查 Windows 防火墙是否放行 UDP 5353 和 TCP 631");
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
            set_language,
        ])
        
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
