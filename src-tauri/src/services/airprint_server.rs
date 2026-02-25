// src/services/airprint_server.rs
use std::collections::HashMap;
use std::net::IpAddr;

use crate::models::Printer;
use crate::services::mdns::broadcaster::MdnsBroadcaster;
use crate::services::ipp::IppServer;

pub struct AirPrintServer {
    shared_printers: HashMap<String, Printer>,
    mdns: Option<MdnsBroadcaster>,
    ipp_server: Option<IppServer>,
    bind_ip: String,
    port: u16,
    hostname: String,
}

impl AirPrintServer {
    pub fn new() -> Self {
        Self {
            shared_printers: HashMap::new(),
            mdns: None,
            ipp_server: None,
            bind_ip: "0.0.0.0".to_string(),
            port: 631,  // IPP Everywhere 标准端口
            hostname: "airprinter.local.".to_string(),
        }
    }

    pub fn with_hostname(mut self, hostname: &str) -> Result<Self, String> {
        let normalized = if hostname.ends_with(".local.") {
            hostname.to_string()
        } else if hostname.ends_with(".local") {
            format!("{}.", hostname)
        } else {
            return Err("Hostname must end with '.local' or '.local.'".to_string());
        };
        self.hostname = normalized;
        Ok(self)
    }

    pub fn share(&mut self, printer: Printer) -> Result<String, String> {
        let printer_id = printer.id.clone();
        
        if self.shared_printers.contains_key(&printer_id) {
            return Err(format!("打印机 {} 已在共享中", printer_id));
        }

        println!("正在共享打印机: {}", printer.name);

        // 启动 IPP 服务器
        if self.ipp_server.is_none() {
            let mut ipp = IppServer::new(&self.bind_ip, self.port, &self.hostname);
            let server_url = ipp.start()
                .map_err(|e| format!("启动 IPP 服务器失败: {}", e))?;
            self.ipp_server = Some(ipp);
            println!("IPP 服务器已启动: {}", server_url);
        }

        // 获取本机 IP（过滤掉链路本地地址）
        let local_ip = self.get_local_ip()?;
        
        // 初始化 mDNS
        if self.mdns.is_none() {
            self.mdns = Some(MdnsBroadcaster::new()?);
        }

        // 注册 AirPrint 服务
        if let Some(ref mut mdns) = self.mdns {
            mdns.broadcast_airprint(
                &printer.name,
                &self.hostname,
                &local_ip.to_string(),
                self.port
            )?;
        }

        self.shared_printers.insert(printer_id.clone(), printer);
        
        println!("✅ 打印机 '{}' 已成功共享到 AirPrint", printer_id);
        
        Ok(printer_id)
    }

    pub fn stop(&mut self, printer_id: &str) -> Result<(), String> {
        match self.shared_printers.remove(printer_id) {
            Some(printer) => {
                println!("停止共享打印机: {}", printer.name);
                
                if self.shared_printers.is_empty() {
                    println!("没有更多共享打印机，停止服务...");
                    // 显式 drop 确保资源释放
                    self.mdns = None;
                    self.ipp_server = None;
                    // 给系统一点时间释放端口
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                
                Ok(())
            }
            None => Err(format!("打印机 {} 未在共享中", printer_id)),
        }
    }

    /// 获取非链路本地的 IP 地址
    fn get_local_ip(&self) -> Result<IpAddr, String> {
        use std::net::{Ipv4Addr, Ipv6Addr};
        
        let ip = local_ip_address::local_ip()
            .map_err(|e| format!("无法获取本地 IP 地址: {}", e))?;
        
        // 检查是否为链路本地地址
        let is_link_local = match ip {
            IpAddr::V4(ipv4) => {
                // 169.254.0.0/16 是 IPv4 链路本地地址
                ipv4.octets()[0] == 169 && ipv4.octets()[1] == 254
            }
            IpAddr::V6(ipv6) => {
                // fe80::/10 是 IPv6 链路本地地址
                (ipv6.segments()[0] & 0xffc0) == 0xfe80
            }
        };
        
        if is_link_local {
            // 尝试获取其他网络接口的 IP
            match local_ip_address::local_ip() {
                Ok(ip) if !is_link_local_ip(&ip) => Ok(ip),
                _ => Err("获取到的 IP 是链路本地地址(169.254.x.x)，请确保设备已连接到网络".to_string())
            }
        } else {
            Ok(ip)
        }
    }

    pub fn is_shared(&self, printer_id: &str) -> bool {
        self.shared_printers.contains_key(printer_id)
    }

    pub fn get_shared_printers(&self) -> Vec<&Printer> {
        self.shared_printers.values().collect()
    }
    
    pub fn generate_hostname(printer_name: &str) -> String {
        let sanitized = printer_name.to_lowercase()
            .replace(" ", "-")
            .replace("_", "-")
            .replace(".", "-");
        format!("{}.local.", sanitized)
    }
}

fn is_link_local_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => ipv4.octets()[0] == 169 && ipv4.octets()[1] == 254,
        IpAddr::V6(ipv6) => (ipv6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

impl Drop for AirPrintServer {
    fn drop(&mut self) {
        if !self.shared_printers.is_empty() {
            println!("AirPrintServer 被 drop，清理资源...");
            self.shared_printers.clear();
            self.mdns = None;
            self.ipp_server = None;
        }
    }
}