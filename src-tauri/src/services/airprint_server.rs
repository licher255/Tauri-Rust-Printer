use std::collections::HashMap;
use crate::models::Printer;
use crate::services::MdnsBroadcaster;
use crate::services::ipp::IppServer;
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
        
        if self.shared_printers.contains_key(&printer_id) {
            // 使用 t! 宏，传入 key 和参数
            return Err(t!("messages.printer_already_shared", id = printer_id).to_string());
        }
        
        // 替换 println!
        println!("{}", t!("messages.start_sharing", name = printer.name));

        // 启动 IPP 服务器
        if self.ipp_server.is_none() {
            let ipp = IppServer::new("0.0.0.0", 631);
            ipp.start();
            self.ipp_server = Some(ipp);
            println!("{}", t!("messages.ipp_started"));
        }
        
        // 初始化 mDNS 广播
        if self.mdns.is_none() {
            self.mdns = Some(MdnsBroadcaster::new().map_err(|e| {
                t!("messages.mdns_error", error = e.to_string()).to_string()
            })?);
        }
        
        // 广播 AirPrint 服务
        if let Some(ref mut mdns) = self.mdns {
            mdns.broadcast_airprint(&printer.name, 631).map_err(|e| {
                t!("messages.mdns_error", error = e.to_string()).to_string()
            })?;
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