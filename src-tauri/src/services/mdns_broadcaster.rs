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
        
        // 检查是否为链路本地地址
        let ip_str = ip.to_string();
        if ip_str.starts_with("169.254.") {
            eprintln!("[mDNS警告] 检测到链路本地地址 {}, 这可能影响服务发现", ip_str);
        }
        
        self.service_name = format!("air-{}", printer_name.replace(" ", "-"));
        self.ip = ip_str.clone();
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
        
        // AirPrint 必需的 URF 字段
        txt_records.insert("URF".to_string(), 
            "V1.4,CP1,DM1,IS1,W8,RS300,SRGB24,ADOBERGB24".to_string()
        );
        
        // 标记支持 AirPrint (某些客户端需要)
        txt_records.insert("air".to_string(), "none".to_string());
        
        txt_records.insert("universal".to_string(), "true".to_string());
        txt_records.insert("priority".to_string(), "0".to_string());
        
        // 生成稳定的 UUID
        let uuid = format!("b15525c7-8885-4279-a0a2-2ec669b9f{:04}", 
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() % 10000) as u16
        );
        txt_records.insert("UUID".to_string(), uuid.clone());
        
        // 打印调试信息
        println!("[mDNS调试] 准备注册服务:");
        println!("[mDNS调试]   服务类型: _ipp._tcp.local.");
        println!("[mDNS调试]   服务名称: {}", self.service_name);
        println!("[mDNS调试]   主机名: {}._ipp._tcp.local.", self.service_name);
        println!("[mDNS调试]   IP: {}, 端口: {}", ip_str, port);
        println!("[mDNS调试]   UUID: {}", uuid);
        println!("[mDNS调试]   TXT记录数: {}", txt_records.len());

        // 解析 IP 地址字符串为 IpAddr
        let ip_addr: std::net::IpAddr = ip_str.parse()
            .map_err(|e| format!("解析 IP 地址失败: {}", e))?;
        
        // 构造主机名（使用 local. 域）
        let host_name = format!("{}._ipp._tcp.local.", self.service_name);
        
        println!("[mDNS调试] 创建 ServiceInfo:");
        println!("[mDNS调试]   service_type: _ipp._tcp.local.");
        println!("[mDNS调试]   instance_name: {}", self.service_name);
        println!("[mDNS调试]   host_name: {}", host_name);
        println!("[mDNS调试]   ip_addr: {:?}", ip_addr);
        println!("[mDNS调试]   port: {}", port);
        
        // ========== 注册基础 _ipp._tcp 服务 ==========
        let service_info = ServiceInfo::new(
            "_ipp._tcp.local.",
            &self.service_name,     // instance name
            &host_name,             // host name
            ip_addr,                // IP address (不是字符串)
            port,
            txt_records.clone(),
        ).map_err(|e| {
            eprintln!("[mDNS错误] 创建 ServiceInfo 失败: {}", e);
            t!("errors.mdns_service_info_create_failed", error = e.to_string()).to_string()
        })?;

        match self.daemon.register(service_info) {
            Ok(()) => {
                println!("[mDNS调试] 基础 _ipp._tcp 服务注册命令已发送");
            }
            Err(e) => {
                eprintln!("[mDNS错误] 注册基础服务失败: {}", e);
                return Err(t!("errors.mdns_register_failed", error = e.to_string()).to_string());
            }
        }

        // ========== 注册 _printer._tcp 服务 (RFC 6763 Flagship Naming) ==========
        // 根据 IPP Everywhere™ 规范 4.2.2，必须注册 _printer._tcp，端口设为 0 表示不支持 LPD
        let printer_host_name = format!("{}._printer._tcp.local.", self.service_name);
        
        println!("[mDNS调试] 创建 _printer._tcp ServiceInfo (RFC 6763):");
        println!("[mDNS调试]   service_type: _printer._tcp.local.");
        println!("[mDNS调试]   instance_name: {}", self.service_name);
        println!("[mDNS调试]   host_name: {}", printer_host_name);
        
        // _printer._tcp 使用相同的 TXT 记录，但端口设为 0
        let printer_service_info = ServiceInfo::new(
            "_printer._tcp.local.",
            &self.service_name,
            &printer_host_name,
            ip_addr,
            0,  // 端口设为 0，表示 LPD 协议实际上不支持
            txt_records.clone(),
        ).map_err(|e| {
            eprintln!("[mDNS警告] 创建 _printer._tcp ServiceInfo 失败: {}", e);
            t!("errors.mdns_service_info_create_failed", error = e.to_string()).to_string()
        })?;

        match self.daemon.register(printer_service_info) {
            Ok(()) => {
                println!("[mDNS调试] _printer._tcp 服务注册命令已发送 (端口 0，RFC 6763)");
            }
            Err(e) => {
                eprintln!("[mDNS警告] 注册 _printer._tcp 服务失败: {}", e);
            }
        }
        
        // ========== 注册 IPP Everywhere 子类型 _print._sub._ipp._tcp ==========
        // 根据 IPP Everywhere™ 规范 4.2.1，子类型服务使用相同的实例名称
        // 参考: PWG 5100.14-2020
        let subtype_host_name = format!("{}._print._sub._ipp._tcp.local.", self.service_name);
        
        println!("[mDNS调试] 创建 IPP Everywhere 子类型 ServiceInfo:");
        println!("[mDNS调试]   service_type: _print._sub._ipp._tcp.local.");
        println!("[mDNS调试]   instance_name: {}", self.service_name);
        println!("[mDNS调试]   host_name: {}", subtype_host_name);
        
        let print_service_info = ServiceInfo::new(
            "_print._sub._ipp._tcp.local.",
            &self.service_name,     // 使用相同的实例名称
            &subtype_host_name,     // host name
            ip_addr,
            port,
            txt_records.clone(),
        ).map_err(|e| {
            eprintln!("[mDNS警告] 创建 IPP Everywhere 子类型 ServiceInfo 失败: {}", e);
            t!("errors.mdns_service_info_create_failed", error = e.to_string()).to_string()
        })?;

        match self.daemon.register(print_service_info) {
            Ok(()) => {
                println!("[mDNS调试] IPP Everywhere 子类型 _print._sub._ipp._tcp 服务注册命令已发送");
            }
            Err(e) => {
                eprintln!("[mDNS警告] 注册 IPP Everywhere 子类型服务失败: {}", e);
            }
        }

        self.txt_records = txt_records;

        // 成功日志
        println!("{}", t!("logs.mdns_broadcast_success", name = self.service_name, ip = ip, port = port));
        println!("[mDNS调试] 已注册 3 个服务: _ipp._tcp, _printer._tcp(0), _print._sub._ipp._tcp");

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
                    
                    // 注销所有服务（使用相同的实例名称）
                    let _ = daemon.unregister(&format!("{}._ipp._tcp.local.", service_name));
                    let _ = daemon.unregister(&format!("{}._printer._tcp.local.", service_name));
                    let _ = daemon.unregister(&format!("{}._print._sub._ipp._tcp.local.", service_name));
                    
                    // 解析 IP 地址
                    if let Ok(ip_addr) = ip.parse::<std::net::IpAddr>() {
                        // 重新注册基础 _ipp._tcp 服务
                        let host_name = format!("{}._ipp._tcp.local.", service_name);
                        if let Ok(main_info) = ServiceInfo::new(
                            "_ipp._tcp.local.",
                            &service_name,
                            &host_name,
                            ip_addr,
                            port,
                            txt_records.clone(),
                        ) {
                            let _ = daemon.register(main_info);
                        }
                        
                        // 重新注册 _printer._tcp 服务（RFC 6763，端口 0）
                        let printer_host_name = format!("{}._printer._tcp.local.", service_name);
                        if let Ok(printer_info) = ServiceInfo::new(
                            "_printer._tcp.local.",
                            &service_name,
                            &printer_host_name,
                            ip_addr,
                            0,  // 端口 0
                            txt_records.clone(),
                        ) {
                            let _ = daemon.register(printer_info);
                        }
                        
                        // 重新注册 IPP Everywhere 子类型（使用相同的实例名称）
                        let subtype_host_name = format!("{}._print._sub._ipp._tcp.local.", service_name);
                        if let Ok(print_info) = ServiceInfo::new(
                            "_print._sub._ipp._tcp.local.",
                            &service_name,
                            &subtype_host_name,
                            ip_addr,
                            port,
                            txt_records.clone(),
                        ) {
                            let _ = daemon.register(print_info);
                        }
                    }
                }
            }
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if !self.service_name.is_empty() {
            // 注销所有注册的服务（使用相同的实例名称）
            let _ = self.daemon.unregister(&format!("{}._ipp._tcp.local.", self.service_name));
            let _ = self.daemon.unregister(&format!("{}._printer._tcp.local.", self.service_name));
            let _ = self.daemon.unregister(&format!("{}._print._sub._ipp._tcp.local.", self.service_name));
            println!("{}", t!("logs.mdns_broadcast_stopped"));
        }
    }
}

impl Drop for MdnsBroadcaster {
    fn drop(&mut self) {
        self.stop();
    }
}