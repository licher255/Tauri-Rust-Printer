use tiny_http::{Server, Response, Header};
use std::thread;
use std::io::{Read, Cursor, Write};
use std::fs::{self, File};
use std::path::{Path};
use std::process::Command;
use std::time::Duration;

// 👇 1. 导入 prelude 以获取 FromPrimitive trait
use ipp::prelude::*;
use ipp::model::{StatusCode, Operation, DelimiterTag, IppVersion};
use ipp::request::IppRequestResponse;
use ipp::attribute::IppAttribute;
use ipp::value::IppValue;
use ipp::parser::IppParser;
use ipp::reader::IppReader;

// 引入翻译宏
use rust_i18n::t;

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
                // 使用 t! 宏翻译错误日志
                eprintln!("{}", t!("errors.ipp_server_start_failed", error = e.to_string()));
                return;
            }
        };

        let server_address = self.address.clone();
        // 翻译启动日志
        println!("{}", t!("logs.ipp_server_listening", address = self.address));
        println!("{}", t!("logs.ipp_temp_dir_usage"));

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
            eprintln!("{}", t!("errors.ipp_read_body_failed", error = e.to_string()));
            return;
        }

        if body.len() < 9 {
            eprintln!("{}", t!("errors.ipp_packet_too_small"));
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
                
                // 日志可以使用翻译，但操作名通常保留英文以便调试
                println!("{}", t!("logs.ipp_request_parsed", op = op_name, id = request_id));

                // 👇 【关键步骤 1】在消耗 payload 之前，先提取打印属性
                let print_options = Self::extract_print_options(&ipp_request);
                // 这里可以打印选项日志，如果需要的话
                // println!("{}", t!("logs.ipp_print_options", options = format!("{:?}", print_options)));

                // 👇 【关键步骤 2】提取 Payload
                let mut payload_reader = ipp_request.into_payload();
                let mut document_data = Vec::new();
                if let Err(e) = payload_reader.read_to_end(&mut document_data) {
                    eprintln!("{}", t!("errors.ipp_read_payload_failed", error = e.to_string()));
                }

                let response_body = match Operation::from_u16(op_code) {
                    Some(Operation::GetPrinterAttributes) => {
                        Self::handle_get_printer_attributes(request_id, server_address)
                    },
                    Some(Operation::PrintJob) => {
                        Self::handle_print_job(request_id, server_address, document_data, print_options)
                    },
                    Some(Operation::ValidateJob) => {
                        Self::handle_validate_job(request_id)
                    },
                    _ => {
                        eprintln!("{}", t!("errors.ipp_unsupported_operation", op = op_code));
                        Self::create_error_response(request_id, StatusCode::ClientErrorBadRequest)
                    }
                };

                let _ = request.respond(Response::from_data(response_body)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/ipp"[..]).unwrap()));
            },
            Err(e) => {
                eprintln!("{}", t!("errors.ipp_parse_failed", error = format!("{:?}", e)));
                let err_resp = Self::create_error_response(1, StatusCode::ClientErrorBadRequest);
                let _ = request.respond(Response::from_data(err_resp)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/ipp"[..]).unwrap()));
            }
        }
    }

    fn extract_print_options(req: &IppRequestResponse) -> PrintOptions {
        let mut options = PrintOptions::default();
        
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
        
        // 协议属性值保持英文
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

    fn handle_print_job(request_id: u32, server_address: &str, document_data: Vec<u8>, options: PrintOptions) -> Vec<u8> {
        // 翻译日志
        println!("{}", t!("logs.ipp_job_received", id = request_id, size = document_data.len(), copies = options.copies));

        if document_data.is_empty() {
            return Self::create_error_response(request_id, StatusCode::ClientErrorBadRequest);
        }

        let temp_dir = std::env::temp_dir();
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let filename = format!("airprint_{}_{}.pdf", timestamp, request_id);
        let filepath = temp_dir.join(filename);

        let write_result = (|| -> std::io::Result<()> {
            let mut file = File::create(&filepath)?;
            file.write_all(&document_data)?;
            file.sync_all()?; 
            Ok(())
        })();

        if let Err(e) = write_result {
            eprintln!("{}", t!("errors.ipp_write_temp_failed", error = e.to_string(), path = format!("{:?}", filepath)));
            return Self::create_error_response(request_id, StatusCode::ServerErrorInternalError);
        }

        println!("{}", t!("logs.ipp_temp_file_created", path = format!("{:?}", filepath)));

        let filepath_clone = filepath.clone();
        let options_clone = options.clone();
        
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));

            if !filepath_clone.exists() {
                eprintln!("{}", t!("errors.ipp_file_missing_before_print", path = format!("{:?}", filepath_clone)));
                return;
            }

            let print_success = Self::print_document(&filepath_clone, &options_clone);

            if print_success {
                thread::sleep(Duration::from_secs(3));
                if let Err(e) = fs::remove_file(&filepath_clone) {
                    eprintln!("{}", t!("errors.ipp_cleanup_failed", error = e.to_string(), path = format!("{:?}", filepath_clone)));
                } else {
                    println!("{}", t!("logs.ipp_temp_file_cleaned"));
                }
            } else {
                eprintln!("{}", t!("errors.ipp_print_failed_keep_file", path = format!("{:?}", filepath_clone)));
            }
        });

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

    fn print_document(filepath: &Path, options: &PrintOptions) -> bool {
        let file_name = filepath.file_name().unwrap_or_default().to_string_lossy();
        println!("{}", t!("logs.ipp_printing_start", file = file_name, copies = options.copies, sides = options.sides));

        #[cfg(target_os = "windows")]
        {
            let path_str = filepath.to_string_lossy();
            
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
                        println!("{}", t!("logs.ipp_print_success_ps"));
                        return true;
                    } else {
                        eprintln!("{}", t!("errors.ipp_ps_failed", error = String::from_utf8_lossy(&out.stderr)));
                    }
                },
                Err(e) => eprintln!("{}", t!("errors.ipp_ps_start_failed", error = e.to_string())),
            }

            Self::fallback_windows_print(filepath, options);
            return true;
        }

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
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
                        println!("{}", t!("logs.ipp_print_success_lp"));
                        return true;
                    } else {
                        eprintln!("{}", t!("errors.ipp_lp_failed", error = String::from_utf8_lossy(&out.stderr)));
                        return false;
                    }
                },
                Err(e) => {
                    eprintln!("{}", t!("errors.ipp_lp_start_failed", error = e.to_string()));
                    return false;
                }
            }
        }
    }

    fn fallback_windows_print(filepath: &Path, _options: &PrintOptions) {
        let path_str = filepath.to_string_lossy();
        println!("{}", t!("logs.ipp_fallback_print_start"));
        
        match Command::new("cmd")
            .args(&["/C", "start", "", &path_str])
            .spawn()
        {
            Ok(_) => println!("{}", t!("logs.ipp_fallback_sent")),
            Err(e) => eprintln!("{}", t!("errors.ipp_fallback_failed", error = e.to_string())),
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