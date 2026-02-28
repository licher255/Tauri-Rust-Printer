use tiny_http::{Server, Response, Header};
use std::thread;
use std::io::{Read, Cursor, Write};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use std::collections::HashMap;

// 👇 1. 导入 prelude 以获取 FromPrimitive trait
use ipp::prelude::*;
use ipp::model::{StatusCode, Operation, DelimiterTag, IppVersion};
use ipp::request::IppRequestResponse;
use ipp::attribute::IppAttribute;
use ipp::value::IppValue;
use ipp::parser::IppParser;
use ipp::reader::IppReader;

// 定义一个结构体来存储解析出的打印选项
#[derive(Debug, Clone)]
struct PrintOptions {
    copies: i32,
    sides: String,
    color_mode: String,
    media: String,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            copies: 1,
            sides: "one-sided".to_string(),
            color_mode: "auto".to_string(),
            media: "A4".to_string(),
        }
    }
}

pub struct IppServer {
    address: String,
}

impl IppServer {
    pub fn new(bind_address: &str, port: u16) -> Self {
        Self {
            address: format!("{}:{}", bind_address, port),
        }
    }

    pub fn start(&self) {
        let server = match Server::http(&self.address) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("IPP 服务器启动失败：{}", e);
                return;
            }
        };

        let server_address = self.address.clone();
        println!("✅ IPP 服务器监听于：http://{}", self.address);
        println!("📂 打印文件将使用系统临时目录 (自动清理)");

        thread::spawn(move || {
            for request in server.incoming_requests() {
                let addr_clone = server_address.clone();
                thread::spawn(move || {
                    Self::handle_request(request, &addr_clone);
                });
            }
        });
    }

    fn handle_request(mut request: tiny_http::Request, server_address: &str) {
        // Content-Type 检查
        let is_ipp = request.headers().iter().any(|h| {
            let field_lower = h.field.as_str().to_ascii_lowercase();
            let value_lower = h.value.as_str().to_ascii_lowercase();
            field_lower == "content-type" && value_lower.contains("application/ipp")
        });

        if !is_ipp {
            let html = r#"<!DOCTYPE html><html><body><h1>IPP Everywhere Printer</h1></body></html>"#;
            let _ = request.respond(Response::from_string(html)
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()));
            return;
        }

        // 读取 Body
        let mut body = Vec::new();
        if let Err(e) = request.as_reader().read_to_end(&mut body) {
            eprintln!("读取请求体失败：{}", e);
            return;
        }

        if body.len() < 9 {
            eprintln!("数据包太小");
            return;
        }

        let cursor = Cursor::new(body);
        let reader = IppReader::new(cursor);
        let parser = IppParser::new(reader);
        
        match parser.parse() {
            Ok(ipp_request) => {
                let op_code = ipp_request.header().operation_or_status;
                let request_id = ipp_request.header().request_id;
                
                let op_name = Operation::from_u16(op_code)
                    .map(|o| format!("{:?}", o))
                    .unwrap_or_else(|| format!("Unknown({})", op_code));
                
                println!("📦 解析成功：Op={}, ID={}", op_name, request_id);

                // 👇 【关键步骤 1】在消耗 payload 之前，先提取打印属性
                let print_options = Self::extract_print_options(&ipp_request);
                println!("⚙️ 打印选项：{:?}", print_options);

                // 👇 【关键步骤 2】提取 Payload
                let mut payload_reader = ipp_request.into_payload();
                let mut document_data = Vec::new();
                if let Err(e) = payload_reader.read_to_end(&mut document_data) {
                    eprintln!("读取 Payload 失败：{}", e);
                }

                let response_body = match Operation::from_u16(op_code) {
                    Some(Operation::GetPrinterAttributes) => {
                        Self::handle_get_printer_attributes(request_id, server_address)
                    },
                    Some(Operation::PrintJob) => {
                        // 传递 options 给处理函数
                        Self::handle_print_job(request_id, server_address, document_data, print_options)
                    },
                    Some(Operation::ValidateJob) => {
                        Self::handle_validate_job(request_id)
                    },
                    _ => {
                        eprintln!("未支持的操作：{}", op_code);
                        Self::create_error_response(request_id, StatusCode::ClientErrorBadRequest)
                    }
                };

                let _ = request.respond(Response::from_data(response_body)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/ipp"[..]).unwrap()));
            },
            Err(e) => {
                eprintln!("❌ IPP 解析失败：{:?}", e);
                let err_resp = Self::create_error_response(1, StatusCode::ClientErrorBadRequest);
                let _ = request.respond(Response::from_data(err_resp)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/ipp"[..]).unwrap()));
            }
        }
    }

    // 👇 【新功能】提取打印属性
    fn extract_print_options(req: &IppRequestResponse) -> PrintOptions {
        let mut options = PrintOptions::default();
        
        // 遍历所有属性组
        for group in req.attributes().groups() {
            for (_, attr) in group.attributes() {
                match attr.name() {
                    "copies" => {
                        if let IppValue::Integer(val) = attr.value() {
                            options.copies = *val;
                        }
                    },
                    "sides" => {
                        if let IppValue::Keyword(val) | IppValue::NameWithoutLanguage(val) = attr.value() {
                            options.sides = val.clone();
                        }
                    },
                    "print-color-mode" | "color-mode" => {
                        if let IppValue::Keyword(val) | IppValue::NameWithoutLanguage(val) = attr.value() {
                            options.color_mode = val.clone();
                        }
                    },
                    "media" | "media-size" => {
                        if let IppValue::Keyword(val) | IppValue::NameWithoutLanguage(val) = attr.value() {
                            options.media = val.clone();
                        }
                    },
                    _ => {}
                }
            }
        }
        options
    }

    fn handle_get_printer_attributes(request_id: u32, server_address: &str) -> Vec<u8> {
        let printer_uri_str = format!("ipp://{}/ipp/print", server_address);
        let version = IppVersion::v2_0();
        let mut response = IppRequestResponse::new_response(version, StatusCode::SuccessfulOk, request_id);
        let attrs = response.attributes_mut();
        
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-name", IppValue::NameWithoutLanguage("AirPrinter".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-make-and-model", IppValue::TextWithoutLanguage("AirPrinter Model A".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-state", IppValue::Enum(3)));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-is-accepting-jobs", IppValue::Boolean(true)));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-state-reasons", IppValue::Keyword("none".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-uri-supported", IppValue::Uri(printer_uri_str)));
        
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("operations-supported", IppValue::Array(vec![
            IppValue::Enum(Operation::PrintJob as i32),
            IppValue::Enum(Operation::GetPrinterAttributes as i32),
            IppValue::Enum(Operation::ValidateJob as i32),
        ])));
        
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("document-format-supported", IppValue::Array(vec![
            IppValue::MimeMediaType("application/pdf".to_string()),
            IppValue::MimeMediaType("image/urf".to_string()),
            IppValue::MimeMediaType("image/jpeg".to_string()),
        ])));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("document-format-default", IppValue::MimeMediaType("application/pdf".to_string())));
        
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("urf-supported", IppValue::Array(vec![
            IppValue::Keyword("V1.4".to_string()),
            IppValue::Keyword("CP1".to_string()),
            IppValue::Keyword("DM1".to_string()),
            IppValue::Keyword("IS1".to_string()),
            IppValue::Keyword("W8".to_string()),
            IppValue::Keyword("RS300".to_string()),
            IppValue::Keyword("SRGB24".to_string()),
        ])));

        response.to_bytes().to_vec()
    }

    // 👇 【核心修改】手动管理临时文件，解决占用问题
    fn handle_print_job(request_id: u32, server_address: &str, document_data: Vec<u8>, options: PrintOptions) -> Vec<u8> {
        println!("🖨️ 收到打印任务 #{} (大小: {} bytes, 份数: {})", request_id, document_data.len(), options.copies);

        if document_data.is_empty() {
            return Self::create_error_response(request_id, StatusCode::ClientErrorBadRequest);
        }

        // 1. 构建临时文件路径 (使用 .pdf 后缀，非隐藏文件)
        let temp_dir = std::env::temp_dir();
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let filename = format!("airprint_{}_{}.pdf", timestamp, request_id);
        let filepath = temp_dir.join(filename);

        // 2. 显式创建并写入文件
        let write_result = (|| -> std::io::Result<()> {
            let mut file = File::create(&filepath)?;
            file.write_all(&document_data)?;
            // 👇 关键：显式关闭文件句柄，确保操作系统释放锁
            file.sync_all()?; 
            Ok(())
        })();

        if let Err(e) = write_result {
            eprintln!("❌ 写入临时文件失败：{}", e);
            return Self::create_error_response(request_id, StatusCode::ServerErrorInternalError);
        }

        println!("✅ 数据已写入临时文件：{:?}", filepath);

        // 3. 异步打印
        let filepath_clone = filepath.clone();
        let options_clone = options.clone(); // 如果需要，可以把 options 也传进去
        
        thread::spawn(move || {
            // 稍微等待，确保文件系统索引更新
            thread::sleep(Duration::from_millis(500));

            if !filepath_clone.exists() {
                eprintln!("⚠️ 错误：文件在打印前已消失 {:?}", filepath_clone);
                return;
            }

            let print_success = Self::print_document(&filepath_clone, &options_clone);

            if print_success {
                // 等待 Spooler 读取完成
                thread::sleep(Duration::from_secs(3));
                if let Err(e) = fs::remove_file(&filepath_clone) {
                    eprintln!("⚠️ 清理临时文件失败：{}", e);
                } else {
                    println!("🧹 临时文件已清理");
                }
            } else {
                eprintln!("⚠️ 打印失败，保留文件供调试：{:?}", filepath_clone);
            }
        });

        // 4. 返回成功响应
        let job_uri_str = format!("ipp://{}/jobs/{}", server_address, request_id);
        let version = IppVersion::v2_0();
        let mut response = IppRequestResponse::new_response(version, StatusCode::SuccessfulOk, request_id);
        let attrs = response.attributes_mut();
        
        attrs.add(DelimiterTag::JobAttributes, IppAttribute::new("job-id", IppValue::Integer(request_id as i32)));
        attrs.add(DelimiterTag::JobAttributes, IppAttribute::new("job-uri", IppValue::Uri(job_uri_str)));
        attrs.add(DelimiterTag::JobAttributes, IppAttribute::new("job-state", IppValue::Enum(9)));
        attrs.add(DelimiterTag::JobAttributes, IppAttribute::new("job-state-reasons", IppValue::Keyword("job-completed-successfully".to_string())));

        response.to_bytes().to_vec()
    }

    // 👇 【增强版】支持传递打印选项
    fn print_document(filepath: &Path, options: &PrintOptions) -> bool {
        println!("🖨️ 正在尝试打印：{:?} (份数:{}, 双面:{})", filepath.file_name().unwrap_or_default(), options.copies, options.sides);

        #[cfg(target_os = "windows")]
        {
            let path_str = filepath.to_string_lossy();
            
            // 构造更稳健的 PowerShell 脚本
            // 使用 -LiteralPath 防止通配符问题，使用 try-catch 捕获错误
            let ps_script = format!(
                r#"
                $path = "{}"
                if (Test-Path -LiteralPath $path) {{
                    Start-Process -FilePath $path -Verb Print -Wait -ErrorAction Stop
                    Write-Host "Success"
                }} else {{
                    Write-Error "File not found"
                    exit 1
                }}
                "#,
                path_str.replace("\\", "\\\\").replace("\"", "`\"")
            );

            let output = Command::new("powershell")
                .args(&["-NoProfile", "-NonInteractive", "-Command", &ps_script])
                .output();

            match output {
                Ok(out) => {
                    if out.status.success() {
                        println!("✅ 打印命令执行成功 (PowerShell)");
                        return true;
                    } else {
                        eprintln!("⚠️ PS 执行失败：{}", String::from_utf8_lossy(&out.stderr));
                        // 降级
                    }
                },
                Err(e) => eprintln!("❌ 无法启动 PowerShell: {}", e),
            }

            // 降级方案
            Self::fallback_windows_print(filepath, options);
            return true;
        }

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            // macOS/Linux 可以使用 lp 命令传递选项
            let mut cmd = Command::new("lp");
            cmd.arg(filepath);
            if options.copies > 1 {
                cmd.arg("-n").arg(options.copies.to_string());
            }
            if options.sides == "two-sided-long-edge" {
                cmd.arg("-o").arg("sides=two-sided-long-edge");
            }
            
            match cmd.output() {
                Ok(out) => {
                    if out.status.success() {
                        println!("✅ 打印命令执行成功 (lp)");
                        return true;
                    } else {
                        eprintln!("⚠️ lp 命令失败：{}", String::from_utf8_lossy(&out.stderr));
                        return false;
                    }
                },
                Err(e) => {
                    eprintln!("❌ 无法执行 lp: {}", e);
                    return false;
                }
            }
        }
        
        false
    }

    fn fallback_windows_print(filepath: &Path, options: &PrintOptions) {
        let path_str = filepath.to_string_lossy();
        println!("🔄 尝试降级打印方案 (cmd start)...");
        
        // 注意：cmd start 无法直接传递份数和双面参数，只能打开文件
        // 如果需要高级功能，建议用户安装 SumatraPDF 并配置关联
        match Command::new("cmd")
            .args(&["/C", "start", "", &path_str])
            .spawn()
        {
            Ok(_) => println!("✅ 降级命令已发送 (将打开默认应用)"),
            Err(e) => eprintln!("❌ 降级命令失败：{}", e),
        }
    }

    fn handle_validate_job(request_id: u32) -> Vec<u8> {
        let version = IppVersion::v2_0();
        let response = IppRequestResponse::new_response(version, StatusCode::SuccessfulOk, request_id);
        response.to_bytes().to_vec()
    }

    fn create_error_response(request_id: u32, status: StatusCode) -> Vec<u8> {
        let version = IppVersion::v2_0();
        let response = IppRequestResponse::new_response(version, status, request_id);
        response.to_bytes().to_vec()
    }
}