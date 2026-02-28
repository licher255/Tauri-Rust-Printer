use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use local_ip_address::local_ip;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
// 引入翻译宏
use rust_i18n::t;
pub struct MdnsBroadcaster {
    daemon: ServiceDaemon,
    service_name: String,
    ip: String,
    port: u16,
    txt_records: HashMap<String, String>,
    _heartbeat: Option<thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl MdnsBroadcaster {
    pub fn new() -> Result<Self, String> {
        let daemon = ServiceDaemon::new()
            // 使用 t! 宏替换硬编码中文
            .map_err(|e| t!("errors.mdns_daemon_create_failed", error = e.to_string()).to_string())?;
        
        Ok(Self {
            daemon,
            service_name: String::new(),
            ip: String::new(),
            port: 0,
            txt_records: HashMap::new(),
            _heartbeat: None,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn broadcast_airprint(
        &mut self,
        printer_name: &str,
        port: u16,
    ) -> Result<(), String> {
        let ip = local_ip()
            .map_err(|e| t!("errors.mdns_get_ip_failed", error = e.to_string()).to_string())?;
        
        // 日志也使用翻译
        println!("{}", t!("logs.mdns_local_ip", ip = ip.to_string()));
        
        self.service_name = format!("air-{}", printer_name.replace(" ", "-"));
        self.ip = ip.to_string();
        self.port = port;

        // 完整的 TXT 记录 (协议关键字保持英文，不要翻译)
        let mut txt_records = HashMap::new();
        txt_records.insert("txtvers".to_string(), "1".to_string());
        txt_records.insert("qtotal".to_string(), "1".to_string());
        txt_records.insert("rp".to_string(), "ipp/print".to_string());
        txt_records.insert("ty".to_string(), printer_name.to_string());
        txt_records.insert("product".to_string(), format!("({})", printer_name));
        // "note" 字段是给用户看的，可以考虑翻译，但通常 AirPrint 客户端显示有限，建议保持英文或简短
        txt_records.insert("note".to_string(), t!("mdns.note_content").to_string()); 
        txt_records.insert("adminurl".to_string(), format!("http://{}:631/", ip));
        txt_records.insert("pdl".to_string(), "application/pdf,image/urf,image/jpeg".to_string());
        txt_records.insert("Color".to_string(), "T".to_string());
        txt_records.insert("Duplex".to_string(), "T".to_string());
        txt_records.insert("Scan".to_string(), "F".to_string());
        txt_records.insert("Fax".to_string(), "F".to_string());
        txt_records.insert("Copies".to_string(), "T".to_string());
        txt_records.insert("Collate".to_string(), "T".to_string());
        txt_records.insert("kind".to_string(), "document".to_string());
        txt_records.insert("PaperMax".to_string(), "legal-A4".to_string());
        
        txt_records.insert("URF".to_string(), 
            "V1.4,CP1,DM1,IS1,W8,RS300,SRGB24,ADOBERGB24".to_string()
        );
        
        txt_records.insert("universal".to_string(), "true".to_string());
        txt_records.insert("priority".to_string(), "0".to_string());
        
        let uuid = format!("b15525c7-8885-4279-a0a2-2ec669b9f{:04}", 
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() % 10000) as u16
        );
        txt_records.insert("UUID".to_string(), uuid);

        let service_info = ServiceInfo::new(
            "_ipp._tcp.local.",
            &self.service_name,
            &format!("{}._ipp._tcp.local.", self.service_name),
            &ip.to_string(),
            port,
            txt_records.clone(),
        ).map_err(|e| t!("errors.mdns_service_info_create_failed", error = e.to_string()).to_string())?;

        self.daemon.register(service_info)
            .map_err(|e| t!("errors.mdns_register_failed", error = e.to_string()).to_string())?;

        self.txt_records = txt_records;

        // 成功日志
        println!("{}", t!("logs.mdns_broadcast_success", name = self.service_name, ip = ip, port = port));

        self.start_heartbeat(); 

        Ok(())
    }

    fn start_heartbeat(&mut self) {
        self.running.store(true, Ordering::Relaxed);
        let running = self.running.clone();
        let daemon = self.daemon.clone();
        let service_name = self.service_name.clone();
        let ip = self.ip.clone();
        let port = self.port;
        let txt_records = self.txt_records.clone();

        self._heartbeat = Some(thread::spawn(move || {
            let mut count = 0;
            while running.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(10));
                count += 1;
                
                if count % 6 == 0 {
                    // 心跳日志
                    println!("{}", t!("logs.mdns_heartbeat_renewing"));
                    
                    let _ = daemon.unregister(&format!("{}._ipp._tcp.local.", service_name));
                    
                    if let Ok(main_info) = ServiceInfo::new(
                        "_ipp._tcp.local.",
                        &service_name,
                        &format!("{}._ipp._tcp.local.", service_name),
                        &ip,
                        port,
                        txt_records.clone(),
                    ) {
                        let _ = daemon.register(main_info);
                    }
                }
            }
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if !self.service_name.is_empty() {
            let _ = self.daemon.unregister(&format!("{}._ipp._tcp.local.", self.service_name));
            println!("{}", t!("logs.mdns_broadcast_stopped"));
        }
    }
}

impl Drop for MdnsBroadcaster {
    fn drop(&mut self) {
        self.stop();
    }
}