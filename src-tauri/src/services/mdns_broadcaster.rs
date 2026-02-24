use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
//use std::net::IpAddr;
use local_ip_address::local_ip;

pub struct MdnsBroadcaster {
    daemon: ServiceDaemon,
    service_name: String,
}

impl MdnsBroadcaster {
    pub fn new() -> Result<Self, String> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| format!("创建 mDNS 守护进程失败: {}", e))?;
        
        Ok(Self {
            daemon,
            service_name: String::new(),
        })
    }

    /// 广播 AirPrint 服务
    pub fn broadcast_airprint(
        &mut self,
        printer_name: &str,
        port: u16,
    ) -> Result<(), String> {
        // 获取本机 IP
        let ip = local_ip()
            .map_err(|e| format!("获取本机 IP 失败: {}", e))?;
        
        println!("本机 IP: {}", ip);

        // 服务实例名称（带 air- 前缀）
        self.service_name = format!("air-{}", printer_name.replace(" ", "-"));
        
        // 关键：TXT 记录，包含 AirPrint 必需字段
        let mut txt_records = HashMap::new();
        txt_records.insert("txtvers".to_string(), "1".to_string());
        txt_records.insert("qtotal".to_string(), "1".to_string());
        txt_records.insert("rp".to_string(), "ipp/print".to_string());
        txt_records.insert("ty".to_string(), printer_name.to_string());
        txt_records.insert("product".to_string(), format!("({})", printer_name));
        txt_records.insert("pdl".to_string(), "image/urf,application/pdf,image/jpeg".to_string());
        txt_records.insert("Color".to_string(), "T".to_string());
        txt_records.insert("Duplex".to_string(), "T".to_string());
        txt_records.insert("Scan".to_string(), "F".to_string());
        txt_records.insert("Fax".to_string(), "F".to_string());
        txt_records.insert("Copies".to_string(), "T".to_string());
        txt_records.insert("Collate".to_string(), "T".to_string());
        txt_records.insert("kind".to_string(), "document".to_string());
        txt_records.insert("PaperMax".to_string(), "<legal-A4".to_string());
        
        // 关键：URF 支持（Universal Raster Format），AirPrint 必需！
        txt_records.insert("URF".to_string(), 
            "V1.4,CP1,PQ3,RS300-600,MT1-2-3-4-5,W8,SRGB24,ADOBERGB24,IS1".to_string()
        );
        
        // UUID
        let uuid = format!("b15525c7-8885-4279-a0a2-2ec669b9f{:04}", rand::random::<u16>());
        txt_records.insert("UUID".to_string(), uuid);

        // 创建服务信息 - 修正：不要手动加 .local.，让库自动处理
        let service_info = ServiceInfo::new(
            "_ipp._tcp.local.",             // 服务类型（加 .local.）
            &self.service_name,             // 实例名称（不加后缀）
            &format!("{}._ipp._tcp.local.", self.service_name), // 完整名称
            &ip.to_string(),                // IP 地址
            port,                           // 端口
            txt_records,                    // TXT 记录
        ).map_err(|e| format!("创建服务信息失败: {}", e))?;

        // 注册服务
        self.daemon.register(service_info)
            .map_err(|e| format!("注册 mDNS 服务失败: {}", e))?;

        println!("AirPrint 服务已广播: {} 在 {}:{}", self.service_name, ip, port);
        
        // 同时注册 _universal._sub._ipp._tcp
        self.register_universal_sub_service(&ip.to_string(), port)?;

        Ok(())
    }

    /// 注册 AirPrint 特定的 _universal._sub._ipp._tcp 服务
    fn register_universal_sub_service(
        &self,
        ip: &str,
        port: u16,
    ) -> Result<(), String> {
        let txt_records = HashMap::new();
        
        let service_info = ServiceInfo::new(
            "_universal._sub._ipp._tcp.local.",  // 子类型也要加 .local.
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

    /// 停止广播
    pub fn stop(&self) {
        if !self.service_name.is_empty() {
            let _ = self.daemon.unregister(&format!("{}._ipp._tcp.local.", self.service_name));
            let _ = self.daemon.unregister(&format!("{}._universal._sub._ipp._tcp.local.", self.service_name));
            println!("mDNS 广播已停止");
        }
    }
}

impl Drop for MdnsBroadcaster {
    fn drop(&mut self) {
        self.stop();
    }
}