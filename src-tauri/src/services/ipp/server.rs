use tiny_http::{Server, Response, Header};
use std::thread;
use std::io::{Read, Cursor, Write};
use std::fs::{self, File};
use std::path::{Path};
use std::process::Command;
use std::time::Duration;
use std::net::TcpListener;

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

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

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
    
    /// 检查端口是否被占用
    fn check_port_available(address: &str) -> Result<(), String> {
        // 尝试绑定到该端口，如果成功说明端口可用，失败说明被占用
        match TcpListener::bind(address) {
            Ok(listener) => {
                // 立即释放端口
                drop(listener);
                Ok(())
            }
            Err(e) => {
                Err(format!("端口 {} 已被占用或无法绑定: {}", address, e))
            }
        }
    }

    pub fn start(&self) -> Result<(), String> {
        // 首先检查端口是否可用
        if let Err(e) = Self::check_port_available(&self.address) {
            let err_msg = format!(
                "IPP 服务器启动失败: {}。请检查：\n\
                 1. 是否以管理员身份运行（端口631需要管理员权限）\n\
                 2. 端口是否被其他程序占用",
                e
            );
            eprintln!("[IPP错误] {}", err_msg);
            return Err(err_msg);
        }
        
        let server = match Server::http(&self.address) {
            Ok(s) => s,
            Err(e) => {
                let err_msg = t!("errors.ipp_server_start_failed", error = e.to_string()).to_string();
                eprintln!("{}", err_msg);
                return Err(err_msg);
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
        
        Ok(())
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
                    Some(Operation::GetJobs) => {
                        Self::handle_get_jobs(request_id, server_address)
                    },
                    Some(Operation::CancelJob) => {
                        Self::handle_cancel_job(request_id)
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
        
        // ===== 基础打印机信息 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-name", IppValue::NameWithoutLanguage("AirPrinter".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-make-and-model", IppValue::TextWithoutLanguage("AirPrinter Model A".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-info", IppValue::TextWithoutLanguage("AirPrint Compatible Printer".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-state", IppValue::Enum(3))); // 3 = idle
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-is-accepting-jobs", IppValue::Boolean(true)));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-state-reasons", IppValue::Keyword("none".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-up-time", IppValue::Integer(3600))); // 正常运行时间
        
        // ===== URI 支持 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-uri-supported", IppValue::Array(vec![
            IppValue::Uri(printer_uri_str.clone()),
        ])));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("uri-authentication-supported", IppValue::Array(vec![
            IppValue::Keyword("none".to_string()),
        ])));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("uri-security-supported", IppValue::Array(vec![
            IppValue::Keyword("none".to_string()),
        ])));
        
        // ===== IPP 版本和特性 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("ipp-versions-supported", IppValue::Array(vec![
            IppValue::Keyword("1.1".to_string()),
            IppValue::Keyword("2.0".to_string()),
        ])));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("ipp-features-supported", IppValue::Array(vec![
            IppValue::Keyword("ipp-everywhere".to_string()),  // 关键：声明支持 IPP Everywhere
        ])));
        
        // ===== 操作支持 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("operations-supported", IppValue::Array(vec![
            IppValue::Enum(Operation::PrintJob as i32),
            IppValue::Enum(Operation::GetPrinterAttributes as i32),
            IppValue::Enum(Operation::ValidateJob as i32),
            IppValue::Enum(Operation::GetJobs as i32),
            IppValue::Enum(Operation::CancelJob as i32),
        ])));
        
        // ===== 文档格式 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("document-format-supported", IppValue::Array(vec![
            IppValue::MimeMediaType("application/pdf".to_string()),
            IppValue::MimeMediaType("image/urf".to_string()),
            IppValue::MimeMediaType("image/jpeg".to_string()),
        ])));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("document-format-default", IppValue::MimeMediaType("application/pdf".to_string())));
        
        // ===== 纸张/介质支持 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("media-default", IppValue::Keyword("iso_a4_210x297mm".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("media-supported", IppValue::Array(vec![
            IppValue::Keyword("iso_a4_210x297mm".to_string()),
            IppValue::Keyword("iso_a5_148x210mm".to_string()),
            IppValue::Keyword("na_letter_8.5x11in".to_string()),
            IppValue::Keyword("na_legal_8.5x14in".to_string()),
        ])));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("media-ready", IppValue::Array(vec![
            IppValue::Keyword("iso_a4_210x297mm".to_string()),
        ])));
        
        // ===== 单双面打印 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("sides-default", IppValue::Keyword("one-sided".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("sides-supported", IppValue::Array(vec![
            IppValue::Keyword("one-sided".to_string()),
            IppValue::Keyword("two-sided-long-edge".to_string()),
            IppValue::Keyword("two-sided-short-edge".to_string()),
        ])));
        
        // ===== 打印份数 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("copies-default", IppValue::Integer(1)));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("copies-supported", IppValue::RangeOfInteger { min: 1, max: 99 }));
        
        // ===== 彩色/灰度 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("print-color-mode-default", IppValue::Keyword("color".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("print-color-mode-supported", IppValue::Array(vec![
            IppValue::Keyword("color".to_string()),
            IppValue::Keyword("monochrome".to_string()),
        ])));
        
        // ===== URF 支持 (AirPrint 必需) =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("urf-supported", IppValue::Array(vec![
            IppValue::Keyword("V1.4".to_string()),
            IppValue::Keyword("CP1".to_string()),
            IppValue::Keyword("DM1".to_string()),
            IppValue::Keyword("IS1".to_string()),
            IppValue::Keyword("MT1".to_string()),  // 介质类型支持
            IppValue::Keyword("MT2".to_string()),
            IppValue::Keyword("W8".to_string()),
            IppValue::Keyword("RS300".to_string()),
            IppValue::Keyword("SRGB24".to_string()),
            IppValue::Keyword("ADOBERGB24".to_string()),
        ])));
        
        // ===== 打印机设备 ID (RFC 2911) =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-device-id", 
            IppValue::TextWithoutLanguage("MFG:Generic;MDL:AirPrinter;CMD:PDF,URF;CLS:PRINTER;DES:Generic AirPrint Compatible Printer;".to_string())));
        
        // ===== PDL 覆盖支持 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("pdl-override-supported", IppValue::Keyword("not-attempted".to_string())));
        
        // ===== 参考 URI 方案 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("reference-uri-schemes-supported", IppValue::Array(vec![
            IppValue::UriScheme("http".to_string()),
            IppValue::UriScheme("https".to_string()),
            IppValue::UriScheme("ftp".to_string()),
        ])));
        
        // ===== 多文档处理 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("multiple-document-jobs-supported", IppValue::Boolean(false)));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("multiple-operation-time-out", IppValue::Integer(60)));
        
        // ===== 作业优先级 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("job-priority-default", IppValue::Integer(50)));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("job-priority-supported", IppValue::RangeOfInteger { min: 1, max: 100 }));
        
        // ===== 字符集和语言 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("charset-configured", IppValue::Charset("utf-8".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("charset-supported", IppValue::Array(vec![
            IppValue::Charset("utf-8".to_string()),
        ])));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("natural-language-configured", IppValue::NaturalLanguage("en".to_string())));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("generated-natural-language-supported", IppValue::Array(vec![
            IppValue::NaturalLanguage("en".to_string()),
        ])));
        
        // ===== 压缩支持 =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("compression-supported", IppValue::Array(vec![
            IppValue::Keyword("none".to_string()),
        ])));
        
        // ===== 打印机 kind (IPP Everywhere) =====
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new("printer-kind", IppValue::Array(vec![
            IppValue::Keyword("document".to_string()),
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
        use std::os::windows::process::CommandExt;
        
        let path_str = filepath.to_string_lossy();
        
        // 方案：使用 PowerShell 的 Out-Printer 或 .NET PrintDialog
        // 这是弹出标准 Windows 打印对话框的最可靠方式
        
        let ps_script = format!(
                        r#"
            Add-Type -AssemblyName System.Windows.Forms
            Add-Type -AssemblyName System.Drawing

            $form = New-Object System.Windows.Forms.Form
            $form.WindowState = 'Minimized'  # 隐藏辅助窗口
            $form.ShowInTaskbar = $false

            $printDialog = New-Object System.Windows.Forms.PrintDialog
            $printDialog.UseEXDialog = $true  # 使用现代样式的打印对话框
            $printDialog.AllowSomePages = $true
            $printDialog.AllowSelection = $true

            # 创建 PrintDocument 来承载设置
            $printDoc = New-Object System.Drawing.Printing.PrintDocument
            $printDialog.Document = $printDoc

            # 设置默认值（从 IPP 请求传递过来的）
            $printDoc.PrinterSettings.Copies = {copies}

            # 显示打印对话框
            $result = $printDialog.ShowDialog($form)

            if ($result -eq [System.Windows.Forms.DialogResult]::OK) {{
                $printer = $printDoc.PrinterSettings.PrinterName
                $copies = $printDoc.PrinterSettings.Copies
                
                Write-Host "Selected printer: $printer"
                Write-Host "Copies: $copies"
                
                # 使用选择的打印机打印文件
                # 方法1: 使用 WMI 设置默认打印机后打印
                $wsnet = New-Object -ComObject WScript.Network
                $originalPrinter = $wsnet.EnumPrinterConnections() | Select-Object -Index 1
                
                try {{
                    $wsnet.SetDefaultPrinter($printer)
                    
                    # 现在用默认动词打印
                    $psi = New-Object System.Diagnostics.ProcessStartInfo
                    $psi.FileName = "{path}"
                    $psi.Verb = "Print"
                    $psi.UseShellExecute = $true
                    $psi.WindowStyle = 'Hidden'
                    
                    $proc = [System.Diagnostics.Process]::Start($psi)
                    $proc.WaitForExit()
                    
                    Write-Host "PRINT_SUCCESS"
                }} finally {{
                    # 恢复原来的默认打印机
                    if ($originalPrinter) {{
                        $wsnet.SetDefaultPrinter($originalPrinter)
                    }}
                }}
            }} else {{
                Write-Host "PRINT_CANCELLED"
            }}

            $form.Close()
            "#,
            path = path_str.replace("\\", "\\\\").replace("\"", "`\""),
            copies = options.copies
        );

        match Command::new("powershell")
            .args(&["-NoProfile", "-Sta", "-ExecutionPolicy", "Bypass", "-Command", &ps_script])
            .creation_flags(CREATE_NO_WINDOW) // CREATE_NO_WINDOW
            .output()
        {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if stdout.contains("PRINT_SUCCESS") {
                        println!("{}", t!("logs.ipp_print_success"));
                        true
                    } else if stdout.contains("PRINT_CANCELLED") {
                        println!("{}", t!("logs.ipp_print_cancelled"));
                        false
                    } else {
                        eprintln!("Print dialog error: {}", String::from_utf8_lossy(&out.stderr));
                        Self::fallback_windows_print(filepath, options)
                    }
                },
                Err(e) => {
                    eprintln!("Failed to start print dialog: {}", e);
                    Self::fallback_windows_print(filepath, options)
                }
            }
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

    fn fallback_windows_print(filepath: &Path, _options: &PrintOptions) -> bool {
        // 最后的降级方案：直接用默认程序打开
        let path_str = filepath.to_string_lossy();
        println!("{}", t!("logs.ipp_fallback_open", file = path_str));
        
        let _ = Command::new("cmd")
            .args(&["/C", "start", "", &path_str])
            .creation_flags(0x08000000)
            .spawn();
        
        true
    }

    fn handle_validate_job(request_id: u32) -> Vec<u8> {
        let version = IppVersion::v2_0();
        let response = IppRequestResponse::new_response(version, StatusCode::SuccessfulOk, request_id);
        response.to_bytes().to_vec()
    }

    fn handle_get_jobs(request_id: u32, _server_address: &str) -> Vec<u8> {
        // 返回空作业列表（简化实现）
        let version = IppVersion::v2_0();
        let mut response = IppRequestResponse::new_response(version, StatusCode::SuccessfulOk, request_id);
        let _attrs = response.attributes_mut();
        
        // 没有作业，直接返回成功响应
        // 实际应用应该查询当前的打印作业列表
        println!("[IPP] GetJobs 请求 - 返回空作业列表");
        
        response.to_bytes().to_vec()
    }

    fn handle_cancel_job(request_id: u32) -> Vec<u8> {
        // 简化实现：返回成功（即使作业不存在或已完成）
        let version = IppVersion::v2_0();
        let response = IppRequestResponse::new_response(version, StatusCode::SuccessfulOk, request_id);
        println!("[IPP] CancelJob 请求 - 已接受");
        response.to_bytes().to_vec()
    }

    fn create_error_response(request_id: u32, status: StatusCode) -> Vec<u8> {
        let version = IppVersion::v2_0();
        let response = IppRequestResponse::new_response(version, status, request_id);
        response.to_bytes().to_vec()
    }
}