// src/services/mdns/broadcaster.rs
use std::collections::HashMap;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use uuid::Uuid;


/// AirPrint 所需的 mDNS TXT 记录键
pub struct AirPrintTxtRecords {
    /// 必需: 文本记录版本
    pub txtvers: String,
    /// 必需: 队列总数
    pub qtotal: String,
    /// 必需: 支持的文档格式 (关键: 必须包含 image/urf)
    pub pdl: String,
    /// 必需: 资源路径 (通常是 ipp/print 或 printers/name)
    pub rp: String,
    /// 必需: 打印机类型/型号
    pub ty: String,
    /// 必需: 产品名称
    pub product: String,
    /// 可选: 位置信息
    pub note: Option<String>,
    /// 必需: AirPrint 能力字符串 (URF 格式)
    pub urf: String,
    /// 必需: 是否支持彩色
    pub color: String,
    /// 必需: 是否支持双面打印
    pub duplex: String,
    /// 必需: 是否支持复印
    pub copies: String,
    /// 必需: 打印机 UUID (关键: iOS 用此识别打印机)
    pub uuid: String,
    /// 可选: 管理界面 URL
    pub adminurl: Option<String>,
    /// 可选: 优先级
    pub priority: String,
    /// 可选: 支持的功能类型
    pub kind: String,
    /// 可选: 最大纸张尺寸
    pub paper_max: String,
    /// 可选: USB 制造商
    pub usb_mfg: Option<String>,
    /// 可选: USB 型号
    pub usb_mdl: Option<String>,
    /// 可选: USB 命令集
    pub usb_cmd: Option<String>,
    /// 可选: 打印机状态
    pub printer_state: String,
    /// 可选: 打印机类型位掩码
    pub printer_type: String,
    /// 可选: AirPrint 版本
    pub air: String,
    /// 可选: 扫描支持
    pub scan: String,
    /// 可选: 传真支持
    pub fax: String,
}

impl AirPrintTxtRecords {
    pub fn new(printer_name: &str, hostname: &str, port: u16) -> Self {
        let uuid = Uuid::new_v4().to_string();
        let _safe_name = printer_name.replace(" ", "_");
        
        Self {
            txtvers: "1".to_string(),
            qtotal: "1".to_string(),
            // 关键: pdl 必须包含 image/urf，否则 iOS 不会识别为 AirPrint
            pdl: "application/pdf,image/urf,image/jpeg,image/pwg-raster".to_string(),
            rp: "ipp/print".to_string(),
            ty: printer_name.to_string(),
            product: format!("({})", printer_name),
            note: Some("Virtual AirPrint Printer".to_string()),
            // URF 格式: V1.x=版本, DM1=双面模式, CP1=彩色, W8=支持A4宽度, RS300=分辨率等
            urf: "V1.4,W8,DM1,CP1,IS1,MT1-2-3-4-5,RS300,SRGB24".to_string(),
            color: "T".to_string(),
            duplex: "T".to_string(),
            copies: "T".to_string(),
            uuid,
            adminurl: Some(format!("http://{}:{}/", hostname, port)),
            priority: "25".to_string(),
            kind: "document".to_string(),
            paper_max: "legal-A4".to_string(),
            usb_mfg: Some("Generic".to_string()),
            usb_mdl: Some("AirPrint Virtual".to_string()),
            usb_cmd: Some("URF".to_string()),
            printer_state: "3".to_string(), // 3 = idle
            printer_type: "0x0480FFFC".to_string(),
            air: "none".to_string(),
            scan: "F".to_string(),
            fax: "F".to_string(),
        }
    }

    /// 转换为 HashMap 用于 mDNS 注册
    pub fn to_hashmap(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("txtvers".to_string(), self.txtvers.clone());
        map.insert("qtotal".to_string(), self.qtotal.clone());
        map.insert("pdl".to_string(), self.pdl.clone());
        map.insert("rp".to_string(), self.rp.clone());
        map.insert("ty".to_string(), self.ty.clone());
        map.insert("product".to_string(), self.product.clone());
        map.insert("URF".to_string(), self.urf.clone());
        map.insert("Color".to_string(), self.color.clone());
        map.insert("Duplex".to_string(), self.duplex.clone());
        map.insert("Copies".to_string(), self.copies.clone());
        map.insert("UUID".to_string(), self.uuid.clone());
        map.insert("priority".to_string(), self.priority.clone());
        map.insert("kind".to_string(), self.kind.clone());
        map.insert("PaperMax".to_string(), self.paper_max.clone());
        map.insert("printer-state".to_string(), self.printer_state.clone());
        map.insert("printer-type".to_string(), self.printer_type.clone());
        map.insert("air".to_string(), self.air.clone());
        map.insert("Scan".to_string(), self.scan.clone());
        map.insert("Fax".to_string(), self.fax.clone());
        
        if let Some(ref note) = self.note {
            map.insert("note".to_string(), note.clone());
        }
        if let Some(ref adminurl) = self.adminurl {
            map.insert("adminurl".to_string(), adminurl.clone());
        }
        if let Some(ref usb_mfg) = self.usb_mfg {
            map.insert("usb_MFG".to_string(), usb_mfg.clone());
        }
        if let Some(ref usb_mdl) = self.usb_mdl {
            map.insert("usb_MDL".to_string(), usb_mdl.clone());
        }
        if let Some(ref usb_cmd) = self.usb_cmd {
            map.insert("usb_CMD".to_string(), usb_cmd.clone());
        }
        
        map
    }
}

pub struct MdnsBroadcaster {
    daemon: ServiceDaemon,
    registered_services: Vec<String>, // 存储已注册的服务全名
}

impl MdnsBroadcaster {
    pub fn new() -> Result<Self, String> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| format!("Failed to create mDNS daemon: {}", e))?;
            
        Ok(Self {
            daemon,
            registered_services: Vec::new(),
        })
    }

    /// 广播 AirPrint 服务
    /// 
    /// # Arguments
    /// * `service_name` - 服务显示名称 (如 "My Printer")
    /// * `hostname` - 主机名，**必须以 .local 结尾** (如 "myprinter.local")
    /// * `ip` - IP 地址 (如 "192.168.1.100")
    /// * `port` - 端口号 (通常是 631)
    pub fn broadcast_airprint(
        &mut self, 
        service_name: &str, 
        hostname: &str,
        ip: &str,
        port: u16
    ) -> Result<(), String> {
        // 验证主机名以 .local 结尾
        if !hostname.ends_with(".local.") {
            return Err(format!(
                "Invalid hostname '{}'. Hostname must end with '.local' (e.g., 'printer.local')", 
                hostname
            ));
        }

        let txt_records = AirPrintTxtRecords::new(service_name, hostname, port);
        let properties = txt_records.to_hashmap();

        // 服务类型: _ipp._tcp (AirPrint 基于 IPP)
        // 子类型: _universal._sub._ipp._tcp (表示支持 IPP Everywhere)
        let service_type = "_ipp._tcp.local.";
        let sub_type = "_universal._sub._ipp._tcp.local.";
        
        // 构建服务全名: <Instance Name>._ipp._tcp.<Domain>
        // 注意: 这里的服务名是用户可见的打印机名称
        let instance_name = format!("{} {}", service_name, "AirPrint");
        
        let service_info = ServiceInfo::new(
            service_type,
            &instance_name,
            hostname,  // 关键: 主机名必须指向正确的 .local 名称
            ip,
            port,
           properties.clone(),
        )
        .map_err(|e| format!("Failed to create service info: {}", e))?
        .enable_addr_auto();
        
        // 注册服务
        self.daemon.register(service_info)
            .map_err(|e| format!("Failed to register service: {}", e))?;

            // 创建子类型服务 (IPP Everywhere)
        let sub_service_info = ServiceInfo::new(
            sub_type,               // "_universal._sub._ipp._tcp.local."
            &instance_name,
            hostname,
            ip,
            port,
            HashMap::new(),         // 子类型通常不需要额外的 TXT 记录
        )
        .map_err(|e| format!("Failed to create sub service info: {}", e))?
        .enable_addr_auto();
        
        self.daemon.register(sub_service_info)
            .map_err(|e| format!("Failed to register sub service: {}", e))?;

        self.registered_services.push(instance_name.clone());
            
        println!("mDNS 广播已启动:");
        println!("  服务: {}._ipp._tcp.local", service_name);
        println!("  主机: {} ({}:{})", hostname, ip, port);
        println!("  UUID: {}", txt_records.uuid);
        
        Ok(())
    }

    pub fn stop(&self) {
        // ServiceDaemon 在 drop 时会自动注销所有服务
        println!("mDNS 广播已停止");
    }
}

impl Drop for MdnsBroadcaster {
    fn drop(&mut self) {
        self.stop();
    }
}