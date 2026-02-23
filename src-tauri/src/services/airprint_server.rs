use std::collections::HashMap;
use crate::models::Printer;

pub struct AirPrintServer {
    shared_printers: HashMap<String, Printer>,
}

impl AirPrintServer {
    pub fn new() -> Self {
        Self {
            shared_printers: HashMap::new(),
        }
    }

    pub fn share(&mut self, printer: Printer) -> Result<String, String> {
        let printer_id = printer.id.clone();  // 先克隆 id
        
        if self.shared_printers.contains_key(&printer_id) {
            return Err(format!("打印机 {} 已经在共享中", printer_id));
        }
        
        println!("开始共享打印机: {}", printer.name);
        
        self.shared_printers.insert(printer_id.clone(), printer);  // 这里 move printer
        Ok(format!("打印机 {} 已共享", printer_id))  // 用保存的 id
    }

    pub fn stop(&mut self, printer_id: &str) -> Result<(), String> {
        match self.shared_printers.remove(printer_id) {
            Some(_) => {
                println!("停止共享打印机: {}", printer_id);
                Ok(())
            }
            None => Err(format!("打印机 {} 未在共享中", printer_id)),
        }
    }

    pub fn is_shared(&self, printer_id: &str) -> bool {
        self.shared_printers.contains_key(printer_id)
    }
}