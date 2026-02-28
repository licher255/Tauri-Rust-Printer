use crate::models::{Printer, PrinterStatus};
use std::process::Command;
// 引入翻译宏
use rust_i18n::t;

pub struct PrinterDetector;

impl PrinterDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn detect(&self) -> Vec<Printer> {
        // 翻译日志
        println!("{}", t!("logs.detector_scanning"));

        #[cfg(target_os = "windows")]
        return self.detect_windows();

        #[cfg(target_os = "macos")]
        return self.detect_macos();

        #[cfg(target_os = "linux")]
        return self.detect_linux();
    }

    /// Windows: 使用 PowerShell 获取打印机列表
    #[cfg(target_os = "windows")]
    fn detect_windows(&self) -> Vec<Printer> {
        let mut printers = Vec::new();

        let output = Command::new("powershell")
            .args([
                "-Command",
                "Get-Printer | Select-Object Name, PortName, PrinterStatus | ConvertTo-Json -Compress"
            ])
            .output();

        match output {
            Ok(result) if result.status.success() => {
                let json_str = String::from_utf8_lossy(&result.stdout);
                
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    let printer_list = if json_val.is_array() {
                        json_val.as_array().unwrap().clone()
                    } else {
                        vec![json_val]
                    };

                    for (i, p) in printer_list.iter().enumerate() {
                        if let Some(name) = p.get("Name").and_then(|v| v.as_str()) {
                            let port = p.get("PortName").and_then(|v| v.as_str()).unwrap_or("Unknown");
                            let status_code = p.get("PrinterStatus").and_then(|v| v.as_i64()).unwrap_or(0);
                            
                            let status = if status_code == 7 || status_code == 9 || status_code == 8 {
                                PrinterStatus::Offline
                            } else {
                                PrinterStatus::Online
                            };
                            
                            // 翻译发现打印机的日志
                            println!("{}", t!(
                                "logs.detector_found_printer", 
                                name = name, 
                                port = port, 
                                code = status_code, 
                                status = format!("{:?}", status)
                            ));
                            
                            printers.push(Printer {
                                name: name.to_string(),
                                id: format!("printer-{}-{}", i, name.replace(" ", "-")),
                                status,
                            });
                        }
                    }
                }
            }
            _ => {
                // 翻译备用方案日志
                println!("{}", t!("logs.detector_fallback_wmic"));
                return self.detect_windows_wmic();
            }
        }

        printers
    }

    /// Windows 备用方案：wmic
    #[cfg(target_os = "windows")]
    fn detect_windows_wmic(&self) -> Vec<Printer> {
        let mut printers = Vec::new();

        let output = Command::new("wmic")
            .args(["printer", "get", "Name", "/format:csv"])
            .output();

        if let Ok(result) = output {
            let text = String::from_utf8_lossy(&result.stdout);
            for (i, line) in text.lines().skip(1).enumerate() {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    let name = parts.last().unwrap_or(&"Unknown").trim();
                    if !name.is_empty() && name != "Name" {
                        printers.push(Printer {
                            name: name.to_string(),
                            id: format!("printer-{}", i),
                            status: PrinterStatus::Online,
                        });
                    }
                }
            }
        }

        printers
    }

    /// macOS: 使用 lpstat
    #[cfg(target_os = "macos")]
    fn detect_macos(&self) -> Vec<Printer> {
        let mut printers = Vec::new();

        if let Ok(output) = Command::new("lpstat").arg("-p").output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for (i, line) in text.lines().enumerate() {
                if line.starts_with("printer ") {
                    let name = line.split_whitespace().nth(1).unwrap_or("Unknown");
                    let status = if line.contains("idle") || line.contains("ready") {
                        PrinterStatus::Online
                    } else {
                        PrinterStatus::Offline
                    };

                    printers.push(Printer {
                        name: name.to_string(),
                        id: format!("mac-printer-{}", i),
                        status,
                    });
                }
            }
        }

        printers
    }

    /// Linux: 使用 lpstat
    #[cfg(target_os = "linux")]
    fn detect_linux(&self) -> Vec<Printer> {
        self.detect_macos()
    }

    pub fn detect_one(&self, id: &str) -> Option<Printer> {
        let printers = self.detect();
        
        // 翻译查找日志
        println!("{}", t!("logs.detector_searching_id", id = id));
        let ids: Vec<String> = printers.iter().map(|p| p.id.clone()).collect();
        println!("{}", t!("logs.detector_available_printers", list = format!("{:?}", ids)));
        
        printers.into_iter().find(|p| p.id == id)
    }
}