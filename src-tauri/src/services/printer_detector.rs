use crate::models::{Printer, PrinterStatus};
use std::process::Command;

pub struct PrinterDetector;

impl PrinterDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn detect(&self) -> Vec<Printer> {
        println!("正在检测系统打印机...");

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

        // 使用 PowerShell 命令获取打印机
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
                            
                            // 获取 Windows 打印机状态码
                            let status_code = p.get("PrinterStatus").and_then(|v| v.as_i64()).unwrap_or(0);
                            
                            // 判断打印机状态
                            // 0 = 未知, 1 = 其他, 2 = 未知, 3 = 空闲, 4 = 打印中, 5 = 预热, 
                            // 6 = 停止打印, 7 = 离线, 8 = 暂停, 9 = 错误
                            let status = if status_code == 7 || status_code == 9 || status_code == 8 {
                                PrinterStatus::Offline
                            } else {
                                PrinterStatus::Online
                            };
                            
                            println!("发现打印机: {} | 端口: {} | 状态码: {} | 状态: {:?}", 
                                name, port, status_code, status);
                            
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
                println!("PowerShell 命令失败，使用备用方案");
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
        println!("查找打印机 ID: {}", id);
        println!("可用打印机: {:?}", printers.iter().map(|p| &p.id).collect::<Vec<_>>());
        
        printers.into_iter().find(|p| p.id == id)
    }
}