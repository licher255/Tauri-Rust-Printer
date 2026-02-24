use std::collections::HashMap;
use crate::models::Printer;
use crate::services::MdnsBroadcaster;

pub struct AirPrintServer {
    shared_printers: HashMap<String, Printer>,
    mdns: Option<MdnsBroadcaster>,
}

impl AirPrintServer {
    pub fn new() -> Self {
        Self {
            shared_printers: HashMap::new(),
            mdns: None,
        }
    }

    pub fn share(&mut self, printer: Printer) -> Result<String, String> {
        let printer_id = printer.id.clone();
        
        if self.shared_printers.contains_key(&printer_id) {
            return Err(format!("打印机 {} 已经在共享中", printer_id));
        }
        
        println!("开始共享打印机: {}", printer.name);
        
        // 初始化 mDNS 广播（如果还没有）
        if self.mdns.is_none() {
            self.mdns = Some(MdnsBroadcaster::new()?);
        }
        
        // 广播 AirPrint 服务
        if let Some(ref mut mdns) = self.mdns {
            mdns.broadcast_airprint(&printer.name, 631)?;
        }
        
        // TODO: 启动 IPP 服务器接收打印任务
        
        self.shared_printers.insert(printer_id.clone(), printer);
        Ok(format!("打印机 {} 已共享到网络（AirPrint）", printer_id))
    }

    pub fn stop(&mut self, printer_id: &str) -> Result<(), String> {
        match self.shared_printers.remove(printer_id) {
            Some(_) => {
                println!("停止共享打印机: {}", printer_id);
                
                // 如果没有任何打印机了，停止 mDNS
                if self.shared_printers.is_empty() {
                    self.mdns = None; // Drop 会自动停止广播
                }
                
                Ok(())
            }
            None => Err(format!("打印机 {} 未在共享中", printer_id)),
        }
    }

    pub fn is_shared(&self, printer_id: &str) -> bool {
        self.shared_printers.contains_key(printer_id)
    }

    pub fn get_shared_printers(&self) -> Vec<&Printer> {
        self.shared_printers.values().collect()
    }
}