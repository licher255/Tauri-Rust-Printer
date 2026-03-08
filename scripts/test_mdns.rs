// 简单的 mDNS 测试程序
// 运行: cargo run --example test_mdns

use std::collections::HashMap;
use std::thread;
use std::time::Duration;

fn main() {
    println!("mDNS 测试程序");
    println!("==============");
    
    // 创建 mDNS 守护进程
    let daemon = match mdns_sd::ServiceDaemon::new() {
        Ok(d) => {
            println!("✅ mDNS 守护进程创建成功");
            d
        }
        Err(e) => {
            println!("❌ 创建 mDNS 守护进程失败: {}", e);
            return;
        }
    };
    
    // 获取本机 IP
    let ip = match local_ip_address::local_ip() {
        Ok(ip) => {
            println!("✅ 本机 IP: {}", ip);
            ip
        }
        Err(e) => {
            println!("❌ 获取本机 IP 失败: {}", e);
            return;
        }
    };
    
    // 创建服务信息
    let mut txt_records = HashMap::new();
    txt_records.insert("txtvers".to_string(), "1".to_string());
    txt_records.insert("qtotal".to_string(), "1".to_string());
    
    let service_info = match mdns_sd::ServiceInfo::new(
        "_test._tcp.local.",
        "test-service",
        "test-service._test._tcp.local.",
        ip,
        12345,
        txt_records,
    ) {
        Ok(info) => {
            println!("✅ 服务信息创建成功");
            info
        }
        Err(e) => {
            println!("❌ 创建服务信息失败: {}", e);
            return;
        }
    };
    
    // 注册服务
    match daemon.register(service_info) {
        Ok(()) => {
            println!("✅ 服务注册成功");
            println!("   服务类型: _test._tcp.local.");
            println!("   服务名称: test-service");
            println!("   IP: {}, 端口: 12345", ip);
        }
        Err(e) => {
            println!("❌ 注册服务失败: {}", e);
            return;
        }
    }
    
    println!("\n服务正在广播，按 Ctrl+C 停止...");
    println!("你可以在其他设备上运行 'dns-sd -B _test._tcp' 来查看此服务");
    
    // 保持运行
    loop {
        thread::sleep(Duration::from_secs(10));
        println!("[{}] 服务仍在广播...", 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
}
