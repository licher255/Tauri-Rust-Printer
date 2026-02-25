use tiny_http::{Server, Response, Request};
use std::thread;
use std::collections::HashMap;
use crate::services::ipp::protocol::*;

pub struct IppServer {
    address: String, // Add this field to store the bind address
}

impl IppServer {
    // Modify the constructor to accept both IP and Port
    pub fn new(bind_address: &str, port: u16) -> Self {
        Self {
            address: format!("{}:{}", bind_address, port), // Store full address
        }
    }

    pub fn start(&self) {
        // Use the stored address for binding
        let server = match Server::http(&self.address) {
            Ok(s) => s,
            Err(e) => {
                println!("IPP 服务器启动失败: {}", e);
                return;
            }
        };

        // Clone the address to move into the thread
        let server_address = self.address.clone();
        thread::spawn(move || {
            // Main loop to accept incoming requests
            for mut request in server.incoming_requests() {
                // Clone the address again to move into the *request-specific* thread
                let addr_clone = server_address.clone();
                
                // Spawn a new thread for each request to handle it concurrently
                thread::spawn(move || {
                    println!("收到请求: {} {} from {:?}", 
                        request.method(), 
                        request.url(),
                        request.remote_addr()
                    );

                    let content_type: String = request.headers().iter()
                        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("content-type"))
                        .map(|h| h.value.as_str().to_string())
                        .unwrap_or_default();
                    
                    println!("Content-Type: {}", content_type);

                    // Pass the server's bound address to the handler
                    let response = Self::handle_request(&mut request, &addr_clone); // Use the cloned address
                    
                    if let Err(e) = request.respond(response) {
                        println!("响应失败: {}", e);
                    } else {
                        println!("响应已发送");
                    }
                });
                // Loop immediately back to accept the next request without waiting for the previous one to finish
            }
        });

        println!("IPP 服务器启动在 http://{}", self.address);
    }


    // Modify the function signature to accept the server address
    fn handle_request(request: &mut Request, server_address: &str) -> Response<std::io::Cursor<Vec<u8>>> {
        let is_ipp = request.headers().iter()
            .any(|h| {
                let field = h.field.as_str().as_str();
                let value = h.value.as_str();
                field.eq_ignore_ascii_case("content-type") && 
                value.to_ascii_lowercase().contains("application/ipp")
            });

        if !is_ipp {
            println!("非 IPP 请求，返回 HTML");
            let html = r#"<!DOCTYPE html>
<html>
<body>
    <h1>AirPrinter IPP Server</h1>
    <p>Status: Running</p>
    <p>This is an AirPrint compatible printer.</p>
</body>
</html>"#;
            return Response::from_string(html)
                .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
        }

        let mut body = Vec::new();
        {
            let reader = request.as_reader();
            let _ = reader.read_to_end(&mut body);
        }
        
        println!("IPP 请求体: {} bytes", body.len());

        // Pass the server address down to the response creation
        let response_body = if body.len() >= 9 {
            Self::parse_and_respond(&body, server_address)
        } else {
            println!("IPP 请求体太短");
            Self::create_simple_response_with_address(1, 0x0000, server_address) // successful-ok
        };

        println!("返回 IPP 响应: {} bytes", response_body.len());

        Response::from_data(response_body)
            .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/ipp"[..]).unwrap())
    }

    // Modify the function signature to accept the server address
    fn parse_and_respond(body: &[u8], server_address: &str) -> Vec<u8> {
        let version_major = body[0];
        let version_minor = body[1];
        let operation = u16::from_be_bytes([body[2], body[3]]);
        let request_id = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
        
        println!("IPP 解析: 版本 {}.{} 操作码 0x{:04X} 请求ID {}", 
            version_major, version_minor, operation, request_id);

        match operation {
            0x000B => { // Get-Printer-Attributes
                println!("处理 Get-Printer-Attributes");
                // Pass the address here
                Self::handle_get_printer_attributes(request_id, server_address)
            }
            0x0002 => { // Print-Job
                println!("处理 Print-Job");
                // Pass the address here too, if needed for job-uri
                Self::handle_print_job(request_id, body, server_address)
            }
            0x0026 => { // Validate-Job
                println!("处理 Validate-Job");
                // Pass the address here too
                Self::handle_validate_job(request_id, server_address)
            }
            _ => {
                println!("未知操作码: 0x{:04X}", operation);
                // Pass the address here too
                Self::create_simple_response_with_address(request_id, 0x0000, server_address) // successful-ok
            }
        }
    }

    // Modify the function to accept the server address and use it
    fn handle_get_printer_attributes(request_id: u32, server_address: &str) -> Vec<u8> {
        let mut resp = Vec::new();
        
        // 1. IPP Header (Version Major, Version Minor, Status Code, Request ID)
        resp.push(0x02); // 版本 2.0
        resp.push(0x00);
        resp.extend_from_slice(&0x0000u16.to_be_bytes()); // status: successful-ok
        resp.extend_from_slice(&request_id.to_be_bytes());

        // 2. Operation Attributes Group
        resp.push(OPERATION_ATTRIBUTES_TAG); // 0x01
        Self::add_attr_utf8(&mut resp, "attributes-charset", "utf-8");
        Self::add_attr_language(&mut resp, "attributes-natural-language", "en");

        // 3. Printer Attributes Group
        resp.push(PRINTER_ATTRIBUTES_TAG); // 0x04

        // --- Basic Info ---
        Self::add_attr_text_without_language(&mut resp, "printer-name", "AirPrinter");
        Self::add_attr_text_without_language(&mut resp, "printer-info", "Virtual AirPrint Printer");
        Self::add_attr_text_without_language(&mut resp, "printer-location", "Local");
        Self::add_attr_text_without_language(&mut resp, "printer-make-and-model", "AirPrinter Model A");

        // --- URIs ---
        let printer_uri = format!("ipp://{}/ipp/print", server_address);
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
        // IPP everywhere  requires image/urf as default, and all formats as keywords
        Self::add_attr_keyword(&mut resp, "document-format-default", "application/pdf"); 
        Self::add_attr_keyword(&mut resp, "document-format-supported", "image/jpeg");
        Self::add_attr_keyword(&mut resp, "document-format-supported", "image/urf");
       

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
        
        resp
    }

    // --- Helper Functions for Correct IPP Attribute Encoding ---
    // These functions correctly encode different IPP attribute tags and values.

    fn add_attr_utf8(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x47); // tag: charset (UTF8)
        Self::encode_attr_name_and_value(buf, name, value);
    }

    fn add_attr_language(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x48); // tag: naturalLanguage (NUL terminated string)
        Self::encode_attr_name_and_value(buf, name, value);
    }

    fn add_attr_text_without_language(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x41); // tag: textWithoutLanguage (no language specified)
        Self::encode_attr_name_and_value(buf, name, value);
    }

    fn add_attr_name_without_language(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x42); // tag: nameWithoutLanguage (no language specified)
        Self::encode_attr_name_and_value(buf, name, value);
    }

    // ✅ 确保 add_attr_keyword 存在（用于 document-format 和 urf-supported）
    fn add_attr_keyword(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x44); // tag: keyword
        Self::encode_attr_name_and_value(buf, name, value);
    }

    // ✅ 确保 add_attr_boolean 正确（返回 0x00 或 0x01）
    fn add_attr_boolean(buf: &mut Vec<u8>, name: &str, value: bool) {
        buf.push(0x22); // tag: boolean
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&(1u16).to_be_bytes());
        buf.push(if value { 0x01 } else { 0x00 });
    }
    fn add_attr_uri(buf: &mut Vec<u8>, name: &str, value: &str) {
        buf.push(0x45); // tag: uri (Uniform Resource Identifier)
        Self::encode_attr_name_and_value(buf, name, value);
    }

    fn add_attr_enum(buf: &mut Vec<u8>, name: &str, value: u32) {
        buf.push(0x23); // tag: enum (32-bit unsigned integer)
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&(4u16).to_be_bytes()); // Length of value (4 bytes for enum/u32)
        buf.extend_from_slice(&value.to_be_bytes()); // The actual enum value
    }

    fn add_attr_integer(buf: &mut Vec<u8>, name: &str, value: i32) {
        buf.push(0x21); // tag: integer (32-bit signed integer)
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&(4u16).to_be_bytes()); // Length of value (4 bytes for i32)
        buf.extend_from_slice(&value.to_be_bytes()); // The actual integer value
    }

    fn add_attr_integer_list(buf: &mut Vec<u8>, name: &str, values: Vec<u16>) {
        let first_tag: u8 = 0x21; // Start with integer tag (0x21), explicitly typed as u8
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(first_tag).to_be_bytes()); // Now u8.to_be_bytes() works
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&(2u16).to_be_bytes()); // Length of value (2 bytes for u16)
        buf.extend_from_slice(&values[0].to_be_bytes()); // First value

        for val in values.iter().skip(1) {
            // For subsequent values, the tag is repeated but doesn't need the name again.
            // The tag byte itself needs to be encoded. Since 0x21 is u8, we convert it.
            buf.extend_from_slice(&[0x21_u8]); // Write the tag byte directly for subsequent members
            // Name length is 0 for subsequent members of a multi-valued attribute
            buf.extend_from_slice(&(0u16).to_be_bytes()); 
            buf.extend_from_slice(&(2u16).to_be_bytes()); // Length of value
            buf.extend_from_slice(&val.to_be_bytes()); // Value
        }
    }

    fn encode_attr_name_and_value(buf: &mut Vec<u8>, name: &str, value: &str) {
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
        
        let value_bytes = value.as_bytes();
        buf.extend_from_slice(&(value_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(value_bytes);
    }

    // Modify the print job handler to use dynamic address for job-uri
    fn handle_print_job(request_id: u32, body: &[u8], server_address: &str) -> Vec<u8> {
        if body.len() > 100 {
            let filename = format!("print_job_{}.bin", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs());
            let _ = std::fs::write(&filename, body);
            println!("已保存打印任务到: {}", filename);
        }

        let mut resp = Vec::new();
        resp.push(0x02);
        resp.push(0x00);
        resp.extend_from_slice(&0x0000u16.to_be_bytes()); // successful-ok
        resp.extend_from_slice(&request_id.to_be_bytes());

        resp.push(0x01); // begin-operation-attributes
        Self::add_attr_utf8(&mut resp, "attributes-charset", "utf-8");
        Self::add_attr_language(&mut resp, "attributes-natural-language", "en");

        resp.push(0x02); // job-attributes-tag
        Self::add_attr_integer(&mut resp, "job-id", 1); // Use a proper job ID management if needed
        Self::add_attr_enum(&mut resp, "job-state", 9); // completed = 9
        Self::add_attr_keyword(&mut resp, "job-state-reasons", "job-completed-successfully");
        
        // Use dynamic address for job-uri
        let job_uri = format!("ipp://{}/jobs/1", server_address);
        Self::add_attr_uri(&mut resp, "job-uri", &job_uri);

        resp.push(0x03); // end-of-attributes
        resp
    }

    fn handle_validate_job(request_id: u32, server_address: &str) -> Vec<u8> {
        Self::create_simple_response_with_address(request_id, 0x0000, server_address)
    }

    fn create_simple_response_with_address(request_id: u32, status: u16, _server_address: &str) -> Vec<u8> {
        let mut resp = Vec::new();
        resp.push(0x02);
        resp.push(0x00);
        resp.extend_from_slice(&status.to_be_bytes());
        resp.extend_from_slice(&request_id.to_be_bytes());
        resp.push(OPERATION_ATTRIBUTES_TAG);
        Self::add_attr_utf8(&mut resp, "attributes-charset", "utf-8");
        Self::add_attr_language(&mut resp, "attributes-natural-language", "en");
        resp.push(END_OF_ATTRIBUTES_TAG);
        resp
    }
}

// Example usage in your main function or wherever you create the server:
// let ipp_server = IppServer::new("0.0.0.0", 631); // Make sure IP is correct
// ipp_server.start();
