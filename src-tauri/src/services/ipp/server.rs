// src/services/ipp/server.rs (优化版)
use tiny_http::{Server, Response, Request};
use std::thread;

use crate::services::ipp::protocol::*;
pub struct IppServer {
    bind_address: String,
    port: u16,
    hostname: String,  // 用于生成绝对 URI
    server_handle: Option<thread::JoinHandle<()>>,
}

impl IppServer {
    pub fn new(bind_ip: &str, port: u16, hostname: &str) -> Self {
        Self {
            bind_address: bind_ip.to_string(),
            port,
            hostname: hostname.to_string(),
            server_handle: None,
        }
    }

    pub fn start(&mut self) -> Result<String, String> {
        let address = format!("{}:{}", self.bind_address, self.port);
        let server = Server::http(&address)
            .map_err(|e| format!("Failed to start IPP server on {}: {}", address, e))?;

        let hostname = self.hostname.clone();
        let port = self.port;
        
        let handle = thread::spawn(move || {
            println!("IPP 服务器运行在 http://{}", address);
            
            for request in server.incoming_requests() {
                let hostname_clone = hostname.clone();
                thread::spawn(move || {
                    Self::handle_connection(request, &hostname_clone, port);
                });
            }
        });

        self.server_handle = Some(handle);
        Ok(format!("ipp://{}:{}/ipp/print", self.hostname, self.port))
    }

    fn handle_connection(mut request: Request, hostname: &str, port: u16) {
        println!("IPP 请求: {} {} from {:?}", 
            request.method(), 
            request.url(),
            request.remote_addr()
        );

        // 从 HTTP Host 头获取主机名（优先于配置的主机名）
        let host_header = request.headers().iter()
            .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("host"))
            .map(|h| h.value.as_str().to_string());
        
        // 确定使用的主机名：Host 头 > 配置的 hostname
        let effective_host = match host_header {
            Some(host) => {
                // 去除端口号（如果有）
                host.split(':').next().unwrap_or(hostname).to_string()
            }
            None => hostname.to_string(),
        };

        // 构建绝对 URI 基础
        let base_uri = format!("ipp://{}:{}/ipp/print", effective_host, port);

        let is_ipp = request.headers().iter()
            .any(|h| {
                h.field.as_str().as_str().eq_ignore_ascii_case("content-type") 
                && h.value.as_str().to_ascii_lowercase().contains("application/ipp")
            });

        let response = if !is_ipp {
            Self::handle_http_request(&base_uri)
        } else {
            Self::handle_ipp_request(&mut request, &base_uri, &effective_host, port)
        };

        if let Err(e) = request.respond(response) {
            println!("响应发送失败: {}", e);
        }
    }

    fn handle_http_request(base_uri: &str) -> Response<std::io::Cursor<Vec<u8>>> {
        let html = format!(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>AirPrint Server</title>
</head>
<body>
    <h1>🖨️ AirPrint Virtual Printer</h1>
    <p>Status: <strong>Online</strong></p>
    <p>Printer URI: <code>{}</code></p>
    <p>This printer supports AirPrint and IPP Everywhere.</p>
</body>
</html>"#, base_uri);
        
        Response::from_string(html)
            .with_header(tiny_http::Header::from_bytes(
                &b"Content-Type"[..], 
                &b"text/html; charset=utf-8"[..]
            ).unwrap())
    }

    fn handle_ipp_request(
        request: &mut Request, 
        base_uri: &str,
        hostname: &str,
        port: u16
    ) -> Response<std::io::Cursor<Vec<u8>>> {
        let mut body = Vec::new();
        if let Err(e) = request.as_reader().read_to_end(&mut body) {
            println!("读取请求体失败: {}", e);
            return Self::create_ipp_error_response(0x0500);
        }

        let response_body = if body.len() >= 9 {
            Self::parse_and_respond(&body, base_uri, hostname, port)
        } else {
            println!("IPP 请求体太短 ({} bytes)", body.len());
            Self::create_simple_response(1, 0x0000)
        };

        Response::from_data(response_body)
            .with_header(tiny_http::Header::from_bytes(
                &b"Content-Type"[..], 
                &b"application/ipp"[..]
            ).unwrap())
    }

    fn parse_and_respond(
        body: &[u8], 
        base_uri: &str,
        hostname: &str,
        port: u16
    ) -> Vec<u8> {
        let version_major = body[0];
        let version_minor = body[1];
        let operation = u16::from_be_bytes([body[2], body[3]]);
        let request_id = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);

        println!("IPP 操作: 版本 {}.{}, 操作码 0x{:04X}, 请求ID {}", 
            version_major, version_minor, operation, request_id);

        match operation {
            0x000B => Self::handle_get_printer_attributes(request_id, base_uri, hostname, port),
            0x0002 => Self::handle_print_job(request_id, body, base_uri),
            0x0026 => Self::handle_validate_job(request_id),
            _ => {
                println!("未实现的操作码: 0x{:04X}", operation);
                Self::create_simple_response(request_id, 0x0401)
            }
        }
    }

    fn handle_get_printer_attributes(
        request_id: u32, 
        base_uri: &str,
        hostname: &str,
        port: u16
    ) -> Vec<u8> {
        let mut resp = Vec::new();
        
        // IPP 头
        resp.extend_from_slice(&[0x02, 0x00]);
        resp.extend_from_slice(&0x0000u16.to_be_bytes());
        resp.extend_from_slice(&request_id.to_be_bytes());

        // 操作属性组
        resp.push(OPERATION_ATTRIBUTES_TAG);
        Self::add_attr_charset(&mut resp, "attributes-charset", "utf-8");
        Self::add_attr_language(&mut resp, "attributes-natural-language", "en");

        // 打印机属性组
        resp.push(PRINTER_ATTRIBUTES_TAG);

        // 基本标识
        Self::add_attr_text(&mut resp, "printer-name", "AirPrinter");
        Self::add_attr_text(&mut resp, "printer-info", "Virtual AirPrint Printer");
        Self::add_attr_text(&mut resp, "printer-location", "Local Network");
        Self::add_attr_text(&mut resp, "printer-make-and-model", "Generic AirPrint Device");

        // URI 支持 - 使用绝对 URI
        Self::add_attr_uri(&mut resp, "printer-uri-supported", base_uri);
        
        // 也提供基于主机名的 URI（如果不同）
        let alt_uri = format!("ipp://{}:{}/ipp/print", hostname, port);
        if alt_uri != base_uri {
            Self::add_attr_uri(&mut resp, "printer-uri-supported", &alt_uri);
        }

        // 打印机状态
        Self::add_attr_enum(&mut resp, "printer-state", 3);
        Self::add_attr_keyword(&mut resp, "printer-state-reasons", "none");
        Self::add_attr_boolean(&mut resp, "printer-is-accepting-jobs", true);

        // 支持的操作
        Self::add_attr_integer_list(&mut resp, "operations-supported", vec![
            0x0002, 0x000B, 0x0026,
        ]);

        // 文档格式
        Self::add_attr_keyword(&mut resp, "document-format-default", "application/pdf");
        Self::add_attr_keyword(&mut resp, "document-format-supported", "application/pdf");
        Self::add_attr_keyword(&mut resp, "document-format-supported", "image/jpeg");
        Self::add_attr_keyword(&mut resp, "document-format-supported", "image/urf");
        Self::add_attr_keyword(&mut resp, "document-format-supported", "image/pwg-raster");

        // 颜色与质量
        Self::add_attr_boolean(&mut resp, "color-supported", true);
        Self::add_attr_keyword(&mut resp, "output-mode-supported", "monochrome");
        Self::add_attr_keyword(&mut resp, "output-mode-supported", "color");
        Self::add_attr_keyword(&mut resp, "output-mode-default", "color");

        // 份数支持
        Self::add_attr_integer_range(&mut resp, "copies-supported", 1, 99);
        Self::add_attr_integer(&mut resp, "copies-default", 1);

        // 纸张尺寸
        Self::add_attr_keyword(&mut resp, "media-supported", "iso_a4_210x297mm");
        Self::add_attr_keyword(&mut resp, "media-supported", "na_letter_8.5x11in");
        Self::add_attr_keyword(&mut resp, "media-default", "iso_a4_210x297mm");

        // URF 支持
        Self::add_attr_keyword(&mut resp, "urf-supported", "V1.4");
        Self::add_attr_keyword(&mut resp, "urf-supported", "W8");
        Self::add_attr_keyword(&mut resp, "urf-supported", "DM1");
        Self::add_attr_keyword(&mut resp, "urf-supported", "CP1");
        Self::add_attr_keyword(&mut resp, "urf-supported", "RS300");

        resp.push(END_OF_ATTRIBUTES_TAG);
        resp
    }

    fn handle_print_job(request_id: u32, body: &[u8], base_uri: &str) -> Vec<u8> {
        if body.len() > 100 {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let filename = format!("print_job_{}.bin", timestamp);
            
            if let Err(e) = std::fs::write(&filename, body) {
                println!("保存打印任务失败: {}", e);
            } else {
                println!("打印任务已保存: {}", filename);
            }
        }

        let mut resp = Vec::new();
        resp.extend_from_slice(&[0x02, 0x00]);
        resp.extend_from_slice(&0x0000u16.to_be_bytes());
        resp.extend_from_slice(&request_id.to_be_bytes());

        resp.push(OPERATION_ATTRIBUTES_TAG);
        Self::add_attr_charset(&mut resp, "attributes-charset", "utf-8");
        Self::add_attr_language(&mut resp, "attributes-natural-language", "en");

        resp.push(JOB_ATTRIBUTES_TAG);
        Self::add_attr_integer(&mut resp, "job-id", 1);
        
        // 作业 URI 也使用绝对路径
        let job_uri = format!("{}/jobs/1", base_uri.trim_end_matches("/ipp/print"));
        Self::add_attr_uri(&mut resp, "job-uri", &job_uri);
        
        Self::add_attr_enum(&mut resp, "job-state", 9);
        Self::add_attr_keyword(&mut resp, "job-state-reasons", "job-completed-successfully");

        resp.push(END_OF_ATTRIBUTES_TAG);
        resp
    }

    fn handle_validate_job(request_id: u32) -> Vec<u8> {
        Self::create_simple_response(request_id, 0x0000)
    }

    fn create_simple_response(request_id: u32, status: u16) -> Vec<u8> {
        let mut resp = Vec::new();
        resp.extend_from_slice(&[0x02, 0x00]);
        resp.extend_from_slice(&status.to_be_bytes());
        resp.extend_from_slice(&request_id.to_be_bytes());
        resp.push(OPERATION_ATTRIBUTES_TAG);
        Self::add_attr_charset(&mut resp, "attributes-charset", "utf-8");
        Self::add_attr_language(&mut resp, "attributes-natural-language", "en");
        resp.push(END_OF_ATTRIBUTES_TAG);
        resp
    }

    fn create_ipp_error_response(status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
        let mut resp = Vec::new();
        resp.extend_from_slice(&[0x02, 0x00]);
        resp.extend_from_slice(&status.to_be_bytes());
        resp.extend_from_slice(&1u32.to_be_bytes());
        resp.push(OPERATION_ATTRIBUTES_TAG);
        resp.push(END_OF_ATTRIBUTES_TAG);
        
        Response::from_data(resp)
            .with_header(tiny_http::Header::from_bytes(
                &b"Content-Type"[..], 
                &b"application/ipp"[..]
            ).unwrap())
    }

    // 辅助函数...
    fn add_attr_charset(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x47);
        Self::encode_name_value(buf, name, value);
    }

    fn add_attr_language(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x48);
        Self::encode_name_value(buf, name, value);
    }

    fn add_attr_text(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x41);
        Self::encode_name_value(buf, name, value);
    }

    fn add_attr_keyword(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x44);
        Self::encode_name_value(buf, name, value);
    }

    fn add_attr_uri(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x45);
        Self::encode_name_value(buf, name, value);
    }

    fn add_attr_boolean(buf: &mut Vec<u8>, name: &str, value: bool) {
        buf.push(0x22);
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&(1u16).to_be_bytes());
        buf.push(if value { 0x01 } else { 0x00 });
    }

    fn add_attr_enum(buf: &mut Vec<u8>, name: &str, value: i32) {
        buf.push(0x23);
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&(4u16).to_be_bytes());
        buf.extend_from_slice(&value.to_be_bytes());
    }

    fn add_attr_integer(buf: &mut Vec<u8>, name: &str, value: i32) {
        buf.push(0x21);
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&(4u16).to_be_bytes());
        buf.extend_from_slice(&value.to_be_bytes());
    }

    fn add_attr_integer_list(buf: &mut Vec<u8>, name: &str, values: Vec<i32>) {
        if values.is_empty() { return; }
        
        buf.push(0x21);
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&(4u16).to_be_bytes());
        buf.extend_from_slice(&values[0].to_be_bytes());

        for val in values.iter().skip(1) {
            buf.push(0x21);
            buf.extend_from_slice(&(0u16).to_be_bytes());
            buf.extend_from_slice(&(4u16).to_be_bytes());
            buf.extend_from_slice(&val.to_be_bytes());
        }
    }

    fn add_attr_integer_range(buf: &mut Vec<u8>, name: &str, min: i32, max: i32) {
        buf.push(0x33);
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&(8u16).to_be_bytes());
        buf.extend_from_slice(&min.to_be_bytes());
        buf.extend_from_slice(&max.to_be_bytes());
    }

    fn encode_name_value(buf: &mut Vec<u8>, name: &str, value: &str) {
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        
        let value_bytes = value.as_bytes();
        buf.extend_from_slice(&(value_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(value_bytes);
    }
}

impl Drop for IppServer {
    fn drop(&mut self) {
        println!("IPP 服务器正在关闭...");
        // server 在 drop 时会自动关闭，但我们需要等待线程结束
        if let Some(handle) = self.server_handle.take() {
            // 注意：这里无法强制停止线程，但 drop Server 会关闭监听
            // 给一点时间让系统释放端口
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
}