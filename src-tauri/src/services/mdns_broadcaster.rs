use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use local_ip_address::local_ip;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

pub struct MdnsBroadcaster {
    daemon: ServiceDaemon,
    service_name: String,
    ip: String,          // 保存 IP
    port: u16,           // 保存端口
    txt_records: HashMap<String, String>, // 👈 保存完整的 TXT 记录
    _heartbeat: Option<thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl MdnsBroadcaster {
    pub fn new() -> Result<Self, String> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| format!("创建 mDNS 守护进程失败: {}", e))?;
        
        Ok(Self {
            daemon,
            service_name: String::new(),
            ip: String::new(), // 初始化
            port: 0,
            txt_records: HashMap::new(), // 初始化
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
            .map_err(|e| format!("获取本机 IP 失败: {}", e))?;
        
        println!("本机 IP: {}", ip);
        self.service_name = format!("air-{}", printer_name.replace(" ", "-"));
        self.ip = ip.to_string();  // 👈 保存到结构体
        self.port = port;          // 👈 保存到结构体

        // 完整的 TXT 记录
        let mut txt_records = HashMap::new();
        txt_records.insert("txtvers".to_string(), "1".to_string());
        txt_records.insert("qtotal".to_string(), "1".to_string());
        txt_records.insert("rp".to_string(), "ipp/print".to_string());
        txt_records.insert("ty".to_string(), printer_name.to_string());
        txt_records.insert("product".to_string(), format!("({})", printer_name));
        txt_records.insert("note".to_string(), "Air Printer".to_string());
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
        
        // 👇 URF 必须与 server.rs 中的 urf-supported 保持一致
        txt_records.insert("URF".to_string(), 
            "V1.4,CP1,DM1,IS1,W8,RS300,SRGB24,ADOBERGB24".to_string()
        );
        
        // 👇 关键：添加 universal 关键字声明支持 IPP Everywhere
        txt_records.insert("universal".to_string(), "true".to_string());
        
        txt_records.insert("priority".to_string(), "0".to_string());
        
        // 生成 UUID
        let uuid = format!("b15525c7-8885-4279-a0a2-2ec669b9f{:04}", 
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() % 10000) as u16
        );
        txt_records.insert("UUID".to_string(), uuid);

        // 👇 只注册一个主服务，不要注册子服务！
        let service_info = ServiceInfo::new(
            "_ipp._tcp.local.",
            &self.service_name,
            &format!("{}._ipp._tcp.local.", self.service_name),
            &ip.to_string(),
            port,
            txt_records.clone(),
        ).map_err(|e| format!("创建服务信息失败: {}", e))?;

        self.daemon.register(service_info)
            .map_err(|e| format!("注册 mDNS 服务失败: {}", e))?;

        // ❌ 删除 register_universal_sub_service 调用
        // self.register_universal_sub_service(&ip.to_string(), port, txt_records)?;

        // 👇 保存到结构体供心跳使用
        self.txt_records = txt_records;

        println!("IPP Everywhere 服务已广播: {} 在 {}:{}", self.service_name, ip, port);

        // 启动心跳线程
        self.start_heartbeat(); 

        Ok(())
    }

/* 
    fn register_universal_sub_service(
        &self,
        ip: &str,
        port: u16,
        txt_records: HashMap<String, String>,
    ) -> Result<(), String> {
        let service_info = ServiceInfo::new(
            "_universal._sub._ipp._tcp.local.",
            &self.service_name,
            &format!("{}._universal._sub._ipp._tcp.local.", self.service_name),
            ip,
            port,
            txt_records,
        ).map_err(|e| format!("创建 universal 服务失败: {}", e))?;

        self.daemon.register(service_info)
            .map_err(|e| format!("注册 universal 服务失败: {}", e))?;

        println!("Universal 子服务已注册");
        Ok(())
    }
*/
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
                    println!("重新注册 mDNS 服务...");
                    
                    // 先注销再注册
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
            // 👇 只注销主服务
            let _ = self.daemon.unregister(&format!("{}._ipp._tcp.local.", self.service_name));
            println!("mDNS 广播已停止");
        }
    }
}

impl Drop for MdnsBroadcaster {
    fn drop(&mut self) {
        self.stop();
    }
}