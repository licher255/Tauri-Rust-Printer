use std::collections::HashMap;
use crate::models::Printer;
use crate::services::MdnsBroadcaster;
use crate::services::ipp::{IppServer, set_shared_printer_name};
// 引入 t! 宏用于翻译
use rust_i18n::t;

pub struct AirPrintServer {
    shared_printers: HashMap<String, Printer>,
    mdns: Option<MdnsBroadcaster>,
    ipp_server: Option<IppServer>,
}

impl AirPrintServer {
    pub fn new() -> Self {
        Self {
            shared_printers: HashMap::new(),
            mdns: None,
            ipp_server: None,
        }
    }

    pub fn share(&mut self, printer: Printer) -> Result<String, String> {
        let printer_id = printer.id.clone();
        
        println!("[AirPrintServer] 开始共享打印机: id={}, name={}", printer_id, printer.name);
        
        if self.shared_printers.contains_key(&printer_id) {
            println!("[AirPrintServer] 打印机已在共享中");
            return Err(t!("messages.printer_already_shared", id = printer_id).to_string());
        }
        
        println!("[AirPrintServer] 当前共享打印机数: {}", self.shared_printers.len());
        println!("{}", t!("messages.start_sharing", name = printer.name));

        // 启动 IPP 服务器
        if self.ipp_server.is_none() {
            println!("[AirPrintServer] 正在启动 IPP 服务器...");
            let ipp = IppServer::new("0.0.0.0", 631);
            match ipp.start() {
                Ok(()) => {
                    self.ipp_server = Some(ipp);
                    println!("{}", t!("messages.ipp_started"));
                }
                Err(e) => {
                    println!("[AirPrintServer] IPP 服务器启动失败: {}", e);
                    return Err(t!("messages.ipp_start_failed", error = e).to_string());
                }
            }
        } else {
            println!("[AirPrintServer] IPP 服务器已在运行");
        }
        
        // 初始化 mDNS 广播
        if self.mdns.is_none() {
            println!("[AirPrintServer] 正在创建 mDNS 广播器...");
            match MdnsBroadcaster::new() {
                Ok(mdns) => {
                    self.mdns = Some(mdns);
                    println!("[AirPrintServer] mDNS 广播器创建成功");
                }
                Err(e) => {
                    println!("[AirPrintServer] mDNS 广播器创建失败: {}", e);
                    return Err(t!("messages.mdns_error", error = e.to_string()).to_string());
                }
            }
        } else {
            println!("[AirPrintServer] mDNS 广播器已存在");
        }
        
        // 设置 IPP 服务器使用的打印机名称（确保 Get-Printer-Attributes 返回正确的名称）
        println!("[AirPrintServer] 设置共享打印机名称: {}", printer.name);
        set_shared_printer_name(&printer.name);
        
        // 广播 AirPrint 服务
        if let Some(ref mut mdns) = self.mdns {
            println!("[AirPrintServer] 开始广播 AirPrint 服务...");
            match mdns.broadcast_airprint(&printer.name, 631) {
                Ok(()) => {
                    println!("[AirPrintServer] mDNS 广播成功");
                }
                Err(e) => {
                    println!("[AirPrintServer] mDNS 广播失败: {}", e);
                    return Err(t!("messages.mdns_error", error = e.to_string()).to_string());
                }
            }
        } else {
            println!("[AirPrintServer] 错误: mDNS 广播器为 None");
            return Err("mDNS 广播器未初始化".to_string());
        }
        
        self.shared_printers.insert(printer_id.clone(), printer);
        
        // 返回成功消息也使用翻译
        Ok(t!("messages.share_success", id = printer_id).to_string())
    }

    pub fn stop(&mut self, printer_id: &str) -> Result<(), String> {
        match self.shared_printers.remove(printer_id) {
            Some(_) => {
                println!("{}", t!("messages.stop_sharing", id = printer_id));
                
                if self.shared_printers.is_empty() {
                    self.mdns = None; 
                }
                
                Ok(())
            }
            None => Err(t!("messages.printer_not_shared", id = printer_id).to_string()),
        }
    }

    pub fn is_shared(&self, printer_id: &str) -> bool {
        self.shared_printers.contains_key(printer_id)
    }

    pub fn get_shared_printers(&self) -> Vec<&Printer> {
        self.shared_printers.values().collect()
    }
}