use tiny_http::{Server, Response};
use std::thread;

// 使用 prelude 导入所有常用类型
use ipp::prelude::*;
use ipp::model::{StatusCode, Operation, DelimiterTag, IppVersion};

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
                println!("IPP 服务器启动失败: {}", e);
                return;
            }
        };

        let server_address = self.address.clone();
        thread::spawn(move || {
            for request in server.incoming_requests() {
                let addr_clone = server_address.clone();
                thread::spawn(move || {
                    println!("收到请求: {} {} from {:?}", 
                        request.method(), request.url(), request.remote_addr());

                    // 简化 Content-Type 检查
                    let is_ipp = request.headers().iter().any(|h| {
                        let field_lower = h.field.as_str().to_ascii_lowercase();
                        let value_lower = h.value.as_str().to_ascii_lowercase();
                        field_lower == "content-type" && value_lower.contains("application/ipp")
                    });

                    if !is_ipp {
                        let html = r#"<!DOCTYPE html><html><body><h1>AirPrinter IPP Server</h1></body></html>"#;
                        let _ = request.respond(Response::from_string(html)
                            .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()));
                        return;
                    }

                    let mut body = Vec::new();
                    let mut request = request;
                    let _ = request.as_reader().read_to_end(&mut body);
                    println!("IPP 请求体: {} bytes", body.len());

                    let response_body = if body.is_empty() {
                        Self::bad_request()
                    } else {
                        Self::parse_and_respond(&body, &addr_clone)
                    };

                    println!("返回 IPP 响应: {} bytes", response_body.len());
                    let _ = request.respond(Response::from_data(response_body)
                        .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/ipp"[..]).unwrap()));
                });
            }
        });

        println!("IPP 服务器启动在 http://{}", self.address);
    }

    fn parse_and_respond(body: &[u8], server_address: &str) -> Vec<u8> {
        // 手动解析 IPP header (前9字节)
        if body.len() < 9 {
            return Self::bad_request();
        }

        let version_major = body[0];
        let version_minor = body[1];
        let operation_or_status = u16::from_be_bytes([body[2], body[3]]);
        let request_id = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
        
        // 判断是请求还是响应（通过上下文判断，这里假设是请求）
        // 0x0001-0x000F 是操作码范围
        let operation = if operation_or_status <= 0x000F {
            match Operation::from_u16(operation_or_status) {
                Some(op) => op,
                None => return Self::bad_request_with_id(request_id),
            }
        } else {
            return Self::bad_request_with_id(request_id);
        };

        println!("IPP 请求: version={}.{}, operation={:?}, request_id={}", 
            version_major, version_minor, operation, request_id);

        match operation {
            Operation::GetPrinterAttributes => {
                Self::handle_get_printer_attributes(request_id, server_address)
            }
            Operation::PrintJob => {
                Self::handle_print_job(request_id, server_address)
            }
            Operation::ValidateJob => {
                Self::handle_validate_job(request_id)
            }
            _ => {
                Self::bad_request_with_id(request_id)
            }
        }
    }

    fn handle_get_printer_attributes(request_id: u32, server_address: &str) -> Vec<u8> {
        let printer_uri = format!("ipp://{}/ipp/print", server_address);
<<<<<<< HEAD
=======
        Self::add_attr_uri(&mut resp, "printer-uri-supported", &printer_uri);

        // --- State (CRITICAL) ---
        Self::add_attr_enum(&mut resp, "printer-state", 3); // idle
        Self::add_attr_keyword(&mut resp, "printer-state-reasons", "none");
        Self::add_attr_boolean(&mut resp, "printer-is-accepting-jobs", true); // MUST be boolean!

        // --- Operations (CRITICAL) ---
        Self::add_attr_integer_list(&mut resp, "operations-supported", vec![
            0x0002, // Print-Job
            0x000B, // Get-Printer-Attributes
            0x0026, // Validate-Job
        ]);

        // --- Document Formats (CRITICAL FIX: use keyword, not mimeMediaType) ---
        // AirPrint requires image/urf as default, and all formats as keywords
        Self::add_attr_keyword(&mut resp, "document-format-supported", "application/pdf");
        Self::add_attr_keyword(&mut resp, "document-format-supported", "image/jpeg");
        Self::add_attr_keyword(&mut resp, "document-format-supported", "image/urf");
        Self::add_attr_keyword(&mut resp, "document-format-default", "image/urf"); // ✅ MUST be image/urf + keyword

        // --- Color & Duplex ---
        Self::add_attr_boolean(&mut resp, "color-supported", true);
        Self::add_attr_keyword(&mut resp, "output-mode-supported", "monochrome");
        Self::add_attr_keyword(&mut resp, "output-mode-supported", "color");

        // --- Copies ---
        Self::add_attr_integer(&mut resp, "copies-supported", 99);
        Self::add_attr_integer(&mut resp, "copies-default", 1);

        // --- URF Support (AirPrint specific, includes paper size via W8 etc.) ---
        // This is often sufficient instead of explicit media-size
        Self::add_attr_keyword(&mut resp, "urf-supported", "V1.4");
        Self::add_attr_keyword(&mut resp, "urf-supported", "CP1");   // Color
        Self::add_attr_keyword(&mut resp, "urf-supported", "DM1");   // Duplex mode (one-sided)
        Self::add_attr_keyword(&mut resp, "urf-supported", "IS1");   // Image scaling
        Self::add_attr_keyword(&mut resp, "urf-supported", "MT1-2-3-4-5"); // Media types
        Self::add_attr_keyword(&mut resp, "urf-supported", "RS300"); // Resolution
        Self::add_attr_keyword(&mut resp, "urf-supported", "W8");    // Supports up to A4 width (critical!)
        Self::add_attr_keyword(&mut resp, "urf-supported", "SRGB24"); // Color space

        // --- Job Sheets (optional but recommended) ---
        Self::add_attr_name_without_language(&mut resp, "job-sheets-supported", "none");
        Self::add_attr_name_without_language(&mut resp, "job-sheets-default", "none");

        // --- End ---
        resp.push(END_OF_ATTRIBUTES_TAG); // 0x03
>>>>>>> parent of 60b946b (Fix document format attributes for IPP compliance)
        
        // 创建 IppVersion
        let version = IppVersion::v2_0();
        
        // 创建响应
        let mut response = IppRequestResponse::new_response(
            version,
            StatusCode::SuccessfulOk,
            request_id
        );
        
        // 使用 attributes_mut().add() 添加属性
        let attrs = response.attributes_mut();
        
        // 打印机属性组
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "printer-name",
            IppValue::NameWithoutLanguage("AirPrinter".into())
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "printer-info",
            IppValue::TextWithoutLanguage("Virtual AirPrint Printer".into())
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "printer-location",
            IppValue::TextWithoutLanguage("Local".into())
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "printer-make-and-model",
            IppValue::TextWithoutLanguage("AirPrinter Model A".into())
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "printer-uri-supported",
            IppValue::Uri(printer_uri)
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "printer-state",
            IppValue::Enum(3) // idle
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "printer-is-accepting-jobs",
            IppValue::Boolean(true)
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "printer-state-reasons",
            IppValue::Keyword("none".into())
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "operations-supported",
            IppValue::Array(vec![
                IppValue::Enum(Operation::PrintJob as i32),
                IppValue::Enum(Operation::GetPrinterAttributes as i32),
                IppValue::Enum(Operation::ValidateJob as i32),
            ])
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "document-format-supported",
            IppValue::Array(vec![
                IppValue::MimeMediaType("image/urf".into()),
                IppValue::MimeMediaType("application/pdf".into()),
                IppValue::MimeMediaType("image/jpeg".into()),
            ])
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "document-format-default",
            IppValue::MimeMediaType("image/urf".into())
        ));
        // AirPrint 关键：URF
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "urf-supported",
            IppValue::Array(vec![
                IppValue::Keyword("V1.4".into()),
                IppValue::Keyword("CP1".into()),
                IppValue::Keyword("DM1".into()),
                IppValue::Keyword("IS1".into()),
                IppValue::Keyword("W8".into()),
                IppValue::Keyword("RS300".into()),
                IppValue::Keyword("SRGB24".into()),
            ])
        ));
        // RangeOfInteger 使用结构体语法
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "copies-supported",
            IppValue::RangeOfInteger { min: 1, max: 99 }
        ));
        attrs.add(DelimiterTag::PrinterAttributes, IppAttribute::new(
            "color-supported",
            IppValue::Boolean(true)
        ));

        // 转换为字节
        response.to_bytes().to_vec()
    }

    fn handle_print_job(request_id: u32, server_address: &str) -> Vec<u8> {
        let job_uri = format!("ipp://{}/jobs/1", server_address);
        
        let version = IppVersion::v2_0();
        let mut response = IppRequestResponse::new_response(
            version,
            StatusCode::SuccessfulOk,
            request_id
        );
        
        let attrs = response.attributes_mut();
        
        // Job 属性组
        attrs.add(DelimiterTag::JobAttributes, IppAttribute::new(
            "job-id",
            IppValue::Integer(1)
        ));
        attrs.add(DelimiterTag::JobAttributes, IppAttribute::new(
            "job-state",
            IppValue::Enum(9) // completed
        ));
        attrs.add(DelimiterTag::JobAttributes, IppAttribute::new(
            "job-state-reasons",
            IppValue::Keyword("job-completed-successfully".into())
        ));
        attrs.add(DelimiterTag::JobAttributes, IppAttribute::new(
            "job-uri",
            IppValue::Uri(job_uri)
        ));

        response.to_bytes().to_vec()
    }

    fn handle_validate_job(request_id: u32) -> Vec<u8> {
        let version = IppVersion::v2_0();
        let mut response = IppRequestResponse::new_response(
            version,
            StatusCode::SuccessfulOk,
            request_id
        );
        
        // new_response 已经添加了 operation attributes，不需要再添加
        // 如果需要添加其他属性，使用 response.attributes_mut().add()

        response.to_bytes().to_vec()
    }

    fn bad_request() -> Vec<u8> {
        let version = IppVersion::v2_0();
        let response = IppRequestResponse::new_response(
            version,
            StatusCode::ClientErrorBadRequest,
            1
        );
        response.to_bytes().to_vec()
    }

<<<<<<< HEAD
    fn bad_request_with_id(request_id: u32) -> Vec<u8> {
        let version = IppVersion::v2_0();
        let response = IppRequestResponse::new_response(
            version,
            StatusCode::ClientErrorBadRequest,
            request_id
        );
        response.to_bytes().to_vec()
    }
}
=======
// Example usage in your main function or wherever you create the server:
// let ipp_server = IppServer::new("0.0.0.0", 631); // Make sure IP is correct
// ipp_server.start();
>>>>>>> parent of 60b946b (Fix document format attributes for IPP compliance)
