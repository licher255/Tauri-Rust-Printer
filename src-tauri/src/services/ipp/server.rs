use crate::models::{Printer, PrinterStatus};
use crate::services::print_job::{self, PrintOptions, FORMATS};
use ipp::attribute::{IppAttribute, IppAttributeGroup};
use ipp::model::{DelimiterTag, IppVersion, StatusCode};
use ipp::parser::IppParser;
use ipp::reader::IppReader;
use ipp::request::IppRequestResponse;
use ipp::value::IppValue;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::{
    atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Header, Response, Server};

static NEXT_JOB_ID: AtomicI32 = AtomicI32::new(1);
const MAX_UPLOAD: u64 = 64 * 1024 * 1024;
type Backend = fn(&str, &[u8], &str, &PrintOptions, &AtomicBool) -> Result<i32, String>;

#[derive(Clone)]
struct Job {
    id: i32,
    path: String,
    printer: String,
    owner: String,
    name: String,
    options: PrintOptions,
    state: i32,
    reason: String,
    message: String,
    canceled: Arc<AtomicBool>,
    spool_id: Option<i32>,
    created: Instant,
}
struct State {
    printers: HashMap<String, (Printer, String)>,
    jobs: HashMap<i32, Job>,
    started: Instant,
    backend: Backend,
}
type Shared = Arc<Mutex<State>>;

pub struct IppServer {
    address: String,
    shared: Shared,
    running: Arc<AtomicBool>,
    listener: Option<thread::JoinHandle<()>>,
    secure_listener: Option<thread::JoinHandle<()>>,
    secure_port: Option<u16>,
}

impl IppServer {
    pub fn new(bind_address: &str, port: u16) -> Self {
        Self {
            address: format!("{bind_address}:{port}"),
            shared: Arc::new(Mutex::new(State {
                printers: HashMap::new(),
                jobs: HashMap::new(),
                started: Instant::now(),
                backend: print_job::submit,
            })),
            running: Arc::new(AtomicBool::new(false)),
            listener: None,
            secure_listener: None,
            secure_port: None,
        }
    }

    pub fn add_printer(&self, printer: Printer, hostname: &str) -> Result<(), String> {
        let path = format!("/{}", printer.resource_path());
        let mut state = self.shared.lock().map_err(|e| e.to_string())?;
        if state.printers.contains_key(&path) {
            return Err("Duplicate printer resource".into());
        }
        state.printers.insert(path, (printer, hostname.to_string()));
        Ok(())
    }
    pub fn remove_printer(&self, printer: &Printer) -> Result<(), String> {
        self.shared
            .lock()
            .map_err(|e| e.to_string())?
            .printers
            .remove(&format!("/{}", printer.resource_path()));
        Ok(())
    }
    pub fn update_printer(&self, printer: Printer) -> Result<(), String> {
        let path = format!("/{}", printer.resource_path());
        let mut state = self.shared.lock().map_err(|e| e.to_string())?;
        let current = state
            .printers
            .get_mut(&path)
            .ok_or("Printer is not shared")?;
        current.0 = printer;
        Ok(())
    }
    pub fn start(&mut self) -> Result<(), String> {
        if self.listener.is_some() {
            return Err("IPP server already running".into());
        }
        let server = match Server::http(&self.address) {
            Ok(server) => server,
            Err(error)
                if self.address.ends_with(":631")
                    && error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::AddrInUse) =>
            {
                // Windows can retain TCP 631 after tiny_http closes a connection.
                // DNS-SD publishes the actual SRV port, so use a reserved fallback
                // range instead of forcibly sharing a port with another process.
                let bind_host = self
                    .address
                    .rsplit_once(':')
                    .map(|(host, _)| host)
                    .unwrap_or("0.0.0.0");
                (8631..=8699)
                    .find_map(|port| Server::http(format!("{bind_host}:{port}")).ok())
                    .ok_or("No free IPP port in 8631-8699")?
            }
            Err(error) => return Err(error.to_string()),
        };
        self.address = server.server_addr().to_string();
        let port = server
            .server_addr()
            .to_ip()
            .ok_or("Missing listener address")?
            .port();
        self.running.store(true, Ordering::Release);
        self.listener = Some(self.serve(server, port));
        eprintln!("[IPP] listening on {}", self.address);
        Ok(())
    }

    pub fn start_tls(&mut self, hostname: &str, directory: &std::path::Path) -> Result<(), String> {
        if self.secure_listener.is_some() {
            return Ok(());
        }
        let identity = super::tls::identity(directory, hostname)?;
        let host = self
            .address
            .rsplit_once(':')
            .map(|(h, _)| h)
            .ok_or("Invalid IPP address")?;
        // This range is already covered by the application's LAN firewall rule.
        let mut last_error = String::from("No free IPPS port");
        for port in 8631..=8699 {
            let listener = match std::net::TcpListener::bind(format!("{host}:{port}")) {
                Ok(listener) => listener,
                Err(error) => {
                    last_error = error.to_string();
                    continue;
                }
            };
            let server =
                Server::from_listener(listener, Some(identity)).map_err(|e| e.to_string())?;
            self.secure_listener = Some(self.serve(server, port));
            self.secure_port = Some(port);
            eprintln!("[IPPS] TLS listening on {host}:{port}");
            return Ok(());
        }
        Err(last_error)
    }

    pub fn secure_port(&self) -> Option<u16> {
        self.secure_port
    }

    fn serve(&self, server: Server, port: u16) -> thread::JoinHandle<()> {
        let shared = self.shared.clone();
        let running = self.running.clone();
        running.store(true, Ordering::Release);
        let active = Arc::new(AtomicUsize::new(0));
        thread::spawn(move || {
            while running.load(Ordering::Acquire) {
                match server.recv_timeout(Duration::from_millis(100)) {
                    Ok(Some(request)) => {
                        // ponytail: bounded handlers, replace with a worker pool only if needed.
                        if active.load(Ordering::Acquire) >= 8 {
                            eprintln!("[IPP] rejecting request: handler limit reached");
                            let _ = request.respond(Response::empty(503));
                            continue;
                        }
                        active.fetch_add(1, Ordering::AcqRel);
                        let active = active.clone();
                        let shared = shared.clone();
                        thread::spawn(move || {
                            struct Guard(Arc<AtomicUsize>);
                            impl Drop for Guard {
                                fn drop(&mut self) {
                                    self.0.fetch_sub(1, Ordering::AcqRel);
                                }
                            }
                            let _guard = Guard(active);
                            Self::handle_request(request, &shared, port);
                        });
                    }
                    Ok(None) => (),
                    Err(_) => break,
                }
            }
        })
    }

    pub fn port(&self) -> Result<u16, String> {
        self.address
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse().ok())
            .ok_or("Invalid IPP listener address".into())
    }

    fn handle_request(mut request: tiny_http::Request, shared: &Shared, port: u16) {
        eprintln!(
            "[IPP] {} {} from {:?} secure={}",
            request.method(),
            request.url(),
            request.remote_addr(),
            request.secure()
        );
        #[cfg(debug_assertions)]
        if request.method() == &tiny_http::Method::Get && request.url() == "/debug/test.jpg" {
            let _ = request.respond(
                Response::from_data(
                    include_bytes!("../../../tests/fixtures/quadrants.jpg").to_vec(),
                )
                .with_header(Header::from_bytes("Content-Type", "image/jpeg").unwrap()),
            );
            return;
        }
        #[cfg(debug_assertions)]
        if request.method() == &tiny_http::Method::Get && request.url() == "/debug/jobs" {
            let state = shared.lock().unwrap_or_else(|e| e.into_inner());
            let job = state.jobs.values().max_by_key(|job| job.id).map(|job| {
                serde_json::json!({
                    "id": job.id,
                    "state": job.state,
                    "reason": job.reason,
                    "message": job.message,
                    "spool_id": job.spool_id,
                })
            });
            let _ = request.respond(
                Response::from_string(serde_json::json!({"job": job}).to_string())
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap()),
            );
            return;
        }
        #[cfg(debug_assertions)]
        if request.method() == &tiny_http::Method::Get && request.url() == "/debug" {
            let path = shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .printers
                .keys()
                .next()
                .cloned();
            let Some(path) = path else {
                let _ = request.respond(Response::from_string("No shared printer"));
                return;
            };
            let page = include_str!("debug.html").replace("__PATH__", &path);
            let _ = request.respond(Response::from_string(page).with_header(
                Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap(),
            ));
            return;
        }
        let requested_path = request.url().to_string();
        let path = requested_path
            .split_once("/jobs/")
            .map(|(path, _)| path)
            .unwrap_or(&requested_path)
            .to_string();
        let selected = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .printers
            .get(&path)
            .cloned();
        let Some((_, hostname)) = selected else {
            eprintln!("[IPP] rejecting request: unknown path");
            let _ = request.respond(Response::empty(404));
            return;
        };
        if request.method() != &tiny_http::Method::Post {
            eprintln!("[IPP] rejecting request: method is not POST");
            let _ = request.respond(Response::empty(405));
            return;
        }
        let host = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Host"))
            .map(|h| h.value.as_str())
            .unwrap_or("")
            .to_string();
        eprintln!(
            "[IPP] host={host:?} body_length={:?} content_type={:?} transfer_encoding={:?} expect={:?}",
            request.body_length(),
            request.headers().iter().find(|h| h.field.equiv("Content-Type")).map(|h| h.value.as_str()),
            request.headers().iter().find(|h| h.field.equiv("Transfer-Encoding")).map(|h| h.value.as_str()),
            request.headers().iter().find(|h| h.field.equiv("Expect")).map(|h| h.value.as_str()),
        );
        if !valid_host(&host, &hostname, port) {
            eprintln!("[IPP] rejecting request: invalid Host header");
            let _ = request.respond(Response::empty(400));
            return;
        }
        let content_type = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Content-Type"))
            .map(|h| h.value.as_str().split(';').next().unwrap_or("").trim());
        if !content_type.is_some_and(|c| c.eq_ignore_ascii_case("application/ipp")) {
            eprintln!("[IPP] rejecting request: unsupported Content-Type");
            let _ = request.respond(Response::empty(415));
            return;
        }
        if request
            .body_length()
            .is_some_and(|size| size as u64 > MAX_UPLOAD)
        {
            eprintln!("[IPP] rejecting request: Content-Length exceeds limit");
            let _ = request.respond(Response::empty(413));
            return;
        }
        let mut body = Vec::new();
        if request
            .as_reader()
            .take(MAX_UPLOAD + 1)
            .read_to_end(&mut body)
            .is_err()
        {
            eprintln!("[IPP] rejecting request: failed to read body");
            let _ = request.respond(Response::empty(400));
            return;
        }
        eprintln!("[IPP] body_bytes={}", body.len());
        if body.len() as u64 > MAX_UPLOAD {
            eprintln!("[IPP] rejecting request: body exceeds limit");
            let _ = request.respond(Response::empty(413));
            return;
        }
        if body.len() < 9 {
            eprintln!("[IPP] rejecting request: short body");
            let _ = request.respond(Response::empty(400));
            return;
        }
        let id = u32::from_be_bytes(body[4..8].try_into().unwrap());
        let parsed =
            std::panic::catch_unwind(|| IppParser::new(IppReader::new(Cursor::new(body))).parse());
        let response = match parsed {
            Ok(Ok(req)) => {
                eprintln!(
                    "[IPP] request_id={id} operation={:#06x}",
                    req.header().operation_or_status
                );
                let job_path_matches = if requested_path != path {
                    requested_path
                        .strip_prefix(&format!("{path}/jobs/"))
                        .and_then(|id| id.parse::<i32>().ok())
                        .is_some_and(|id| {
                            attr(&req, "job-id") == Some(&IppValue::Integer(id))
                                || string_attr(&req, "job-uri")
                                    .is_some_and(|uri| uri.ends_with(&format!("/jobs/{id}")))
                        })
                } else {
                    true
                };
                if job_path_matches {
                    process_request(shared, &path, &host, req, request.secure())
                } else {
                    reply(req.header().version, id, StatusCode::ClientErrorNotFound)
                }
            }
            _ => reply(IppVersion::v2_0(), id, StatusCode::ClientErrorBadRequest),
        };
        eprintln!(
            "[IPP] request_id={id} status={:#06x}",
            response.header().operation_or_status
        );
        let _ = request.respond(
            Response::from_data(response.to_bytes().to_vec())
                .with_header(Header::from_bytes("Content-Type", "application/ipp").unwrap())
                .with_header(Header::from_bytes("Cache-Control", "no-cache").unwrap()),
        );
    }
}

impl Drop for IppServer {
    fn drop(&mut self) {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .printers
            .clear();
        self.running.store(false, Ordering::Release);
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
        if let Some(listener) = self.secure_listener.take() {
            let _ = listener.join();
        }
    }
}

fn valid_host(authority: &str, hostname: &str, port: u16) -> bool {
    // IPP's default port is 631; some clients omit it from the HTTP Host field.
    let (host, supplied_port) = authority.rsplit_once(':').unwrap_or((authority, "631"));
    if supplied_port.parse::<u16>().ok() != Some(port) {
        return false;
    }
    if host
        .trim_end_matches('.')
        .eq_ignore_ascii_case(hostname.trim_end_matches('.'))
    {
        return true;
    }
    let Ok(ip) = host.parse::<std::net::Ipv4Addr>() else {
        return false;
    };
    if ip.is_loopback() {
        return true;
    }
    local_ip_address::list_afinet_netifas()
        .map(|interfaces| {
            interfaces
                .iter()
                .any(|(_, address)| *address == std::net::IpAddr::V4(ip))
        })
        .unwrap_or(false)
}

fn valid_ipp_authority(authority: &str, hostname: &str, port: u16) -> bool {
    // RFC 3510: an ipp:// URI without an explicit port means TCP 631.
    // The HTTP Host header can also omit the default 631 port.
    if authority.rsplit_once(':').is_some() {
        valid_host(authority, hostname, port)
    } else {
        valid_host(&format!("{authority}:631"), hostname, port)
    }
}

fn job_id_from_uri(
    value: &str,
    path: &str,
    hostname: &str,
    port: u16,
    scheme: &str,
) -> Option<i32> {
    let uri = value.parse::<ipp::prelude::Uri>().ok()?;
    if uri.scheme_str() != Some(scheme) || uri.query().is_some() {
        return None;
    }
    let authority = uri.authority()?;
    if !valid_ipp_authority(authority.as_str(), hostname, port) {
        return None;
    }
    uri.path()
        .strip_prefix(&format!("{path}/jobs/"))?
        .parse::<i32>()
        .ok()
}
fn attr<'a>(req: &'a IppRequestResponse, name: &str) -> Option<&'a IppValue> {
    req.attributes()
        .groups()
        .iter()
        .find_map(|g| g.attributes().get(name).map(|a| a.value()))
}
fn string_attr<'a>(req: &'a IppRequestResponse, name: &str) -> Option<&'a str> {
    match attr(req, name)? {
        IppValue::Keyword(v)
        | IppValue::Uri(v)
        | IppValue::NameWithoutLanguage(v)
        | IppValue::MimeMediaType(v)
        | IppValue::Charset(v) => Some(v),
        _ => None,
    }
}
fn reply(version: IppVersion, id: u32, status: StatusCode) -> IppRequestResponse {
    IppRequestResponse::new_response(version, status, id)
}
fn add(response: &mut IppRequestResponse, tag: DelimiterTag, name: &str, value: IppValue) {
    response
        .attributes_mut()
        .add(tag, IppAttribute::new(name, value));
}
fn keywords(values: &[&str]) -> IppValue {
    IppValue::Array(
        values
            .iter()
            .map(|v| IppValue::Keyword((*v).into()))
            .collect(),
    )
}

fn wants_printer_attribute(requested: Option<&IppValue>, name: &str) -> bool {
    let matches = |value: &IppValue| matches!(value, IppValue::Keyword(key) if key == name || key == "all" || key == "printer-description" || key == "job-template");
    match requested {
        None => true,
        Some(IppValue::Array(values)) => values.iter().any(matches),
        Some(value) => matches(value),
    }
}

fn options(
    req: &IppRequestResponse,
    printer: &Printer,
    base: Option<&PrintOptions>,
) -> Result<PrintOptions, StatusCode> {
    let mut options = base.cloned().unwrap_or_else(|| PrintOptions {
        media: printer.capabilities.default_media.clone(),
        ..Default::default()
    });
    if let Some(value) = attr(req, "orientation-requested") {
        if let IppValue::Enum(value @ 3..=6) = value {
            options.orientation = *value;
        } else {
            return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
        }
    }
    for (name, supported) in [("number-up", IppValue::Integer(1))] {
        if attr(req, name).is_some_and(|v| v != &supported) {
            return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
        }
    }
    if let Some(value) = attr(req, "print-quality") {
        if let IppValue::Enum(quality @ 3..=5) = value {
            options.quality = *quality;
        } else {
            return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
        }
    }
    if attr(req, "page-ranges").is_some() {
        return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
    }
    if string_attr(req, "print-scaling").is_some_and(|value| !["auto", "fit"].contains(&value)) {
        return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
    }
    if let Some(value) = attr(req, "copies") {
        if let IppValue::Integer(value) = value {
            options.copies = *value;
        } else {
            return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
        }
    }
    for (name, output) in [
        ("sides", &mut options.sides),
        ("media", &mut options.media),
        ("print-color-mode", &mut options.color_mode),
    ] {
        if let Some(value) = attr(req, name) {
            if let IppValue::Keyword(value) = value {
                *output = value.clone();
            } else {
                return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
            }
        }
    }
    if let Some(value) = attr(req, "media-col") {
        let IppValue::Collection(media) = value else {
            return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
        };
        if let Some(IppValue::Keyword(name)) = media.get("media-size-name") {
            options.media = name.clone();
        } else if let Some(IppValue::Collection(size)) = media.get("media-size") {
            let dimensions = (size.get("x-dimension"), size.get("y-dimension"));
            let matched = printer.capabilities.media_sizes.iter().find(|media| {
                dimensions
                    == (
                        Some(&IppValue::Integer(media.width)),
                        Some(&IppValue::Integer(media.height)),
                    )
            });
            options.media = matched
                .ok_or(StatusCode::ClientErrorAttributesOrValuesNotSupported)?
                .name
                .clone();
        } else {
            return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
        }
    }
    if options.copies < 1
        || options.copies > printer.capabilities.max_copies
        || !["one-sided", "two-sided-long-edge", "two-sided-short-edge"]
            .contains(&options.sides.as_str())
        || (options.sides != "one-sided" && !printer.capabilities.duplex)
        || !["monochrome", "color", "auto"].contains(&options.color_mode.as_str())
        || (options.color_mode == "color" && !printer.capabilities.color)
        || !printer.capabilities.media.contains(&options.media)
    {
        return Err(StatusCode::ClientErrorAttributesOrValuesNotSupported);
    }
    let paper = printer
        .capabilities
        .media_sizes
        .iter()
        .find(|media| media.name == options.media)
        .ok_or(StatusCode::ClientErrorAttributesOrValuesNotSupported)?;
    options.paper_kind = paper.windows_kind;
    options.paper_width = (paper.width as f64 / 25.4).round() as i32;
    options.paper_height = (paper.height as f64 / 25.4).round() as i32;
    if let Some(compression) = string_attr(req, "compression") {
        if compression != "none" {
            return Err(StatusCode::ClientErrorCompressionNotSupported);
        }
    }
    Ok(options)
}

#[cfg(test)]
fn process(shared: &Shared, path: &str, host: &str, req: IppRequestResponse) -> IppRequestResponse {
    process_request(shared, path, host, req, false)
}

fn process_request(
    shared: &Shared,
    path: &str,
    host: &str,
    req: IppRequestResponse,
    secure: bool,
) -> IppRequestResponse {
    let id = req.header().request_id;
    let version = req.header().version;
    let error = |status| reply(version, id, status);
    if ![IppVersion::v1_1(), IppVersion::v2_0()].contains(&version) {
        return error(StatusCode::ServerErrorVersionNotSupported);
    }
    if string_attr(&req, "attributes-charset") != Some("utf-8") {
        return error(StatusCode::ClientErrorCharsetNotSupported);
    }
    let printer = {
        let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
        for job in state
            .jobs
            .values_mut()
            .filter(|j| j.state == 4 && j.created.elapsed() > Duration::from_secs(60))
        {
            job.state = 8;
            job.reason = "job-data-insufficient".into();
        }
        match state.printers.get(path) {
            Some((p, _)) => p.clone(),
            None => return error(StatusCode::ClientErrorNotFound),
        }
    };
    let scheme = if secure { "ipps" } else { "ipp" };
    let uri = format!("{scheme}://{host}{path}");
    if let Some(target) = string_attr(&req, "printer-uri") {
        let hostname = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .printers
            .get(path)
            .map(|(_, h)| h.clone())
            .unwrap_or_default();
        let port = host
            .rsplit_once(':')
            .and_then(|(_, p)| p.parse().ok())
            .unwrap_or(631);
        let valid = target.parse::<ipp::prelude::Uri>().ok().is_some_and(|uri| {
            uri.scheme_str() == Some(scheme)
                && uri.path() == path
                && uri.query().is_none()
                && uri
                    .authority()
                    .is_some_and(|a| valid_ipp_authority(a.as_str(), &hostname, port))
        });
        if !valid {
            return error(StatusCode::ClientErrorNotFound);
        }
    }
    let op = req.header().operation_or_status;
    if op == 0x000b {
        #[cfg(debug_assertions)]
        eprintln!(
            "[IPP] requested-attributes={:?} document-format={:?}",
            attr(&req, "requested-attributes"),
            attr(&req, "document-format")
        );
        return printer_attributes(
            shared,
            &printer,
            &uri,
            version,
            id,
            attr(&req, "requested-attributes"),
        );
    }
    if [0x0002, 0x0004, 0x0005, 0x0006].contains(&op)
        && matches!(
            printer.status,
            PrinterStatus::Offline | PrinterStatus::Error(_)
        )
    {
        return error(StatusCode::ServerErrorNotAcceptingJobs);
    }
    if op == 0x000a {
        let which = string_attr(&req, "which-jobs").unwrap_or("not-completed");
        if !["completed", "not-completed", "all"].contains(&which) {
            return error(StatusCode::ClientErrorAttributesOrValuesNotSupported);
        }
        let mine = attr(&req, "my-jobs") == Some(&IppValue::Boolean(true));
        let owner = string_attr(&req, "requesting-user-name").unwrap_or("anonymous");
        let limit = match attr(&req, "limit") {
            Some(IppValue::Integer(n)) if *n > 0 => *n as usize,
            None => usize::MAX,
            _ => return error(StatusCode::ClientErrorAttributesOrValuesNotSupported),
        };
        let state = shared.lock().unwrap_or_else(|e| e.into_inner());
        let mut response = error(StatusCode::SuccessfulOk);
        let mut jobs: Vec<_> = state
            .jobs
            .values()
            .filter(|j| {
                j.path == path
                    && (!mine || j.owner == owner)
                    && match which {
                        "completed" => j.state >= 7,
                        "all" => true,
                        _ => j.state < 7,
                    }
            })
            .collect();
        jobs.sort_by_key(|job| job.id);
        for job in jobs.into_iter().take(limit) {
            response
                .attributes_mut()
                .groups_mut()
                .push(job_group(job, &uri));
        }
        return response;
    }
    let supplied_job_id = match attr(&req, "job-id") {
        Some(IppValue::Integer(id)) => Some(*id),
        Some(_) => return error(StatusCode::ClientErrorBadRequest),
        None => None,
    };
    let hostname = shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .printers
        .get(path)
        .map(|(_, hostname)| hostname.clone())
        .unwrap_or_default();
    let port = host
        .rsplit_once(':')
        .and_then(|(_, number)| number.parse().ok())
        .unwrap_or(631);
    let supplied_job_uri_id = string_attr(&req, "job-uri")
        .and_then(|value| job_id_from_uri(value, path, &hostname, port, scheme));
    if attr(&req, "job-uri").is_some() && supplied_job_uri_id.is_none() {
        return error(StatusCode::ClientErrorNotFound);
    }
    if supplied_job_id.is_some()
        && supplied_job_uri_id.is_some()
        && supplied_job_id != supplied_job_uri_id
    {
        return error(StatusCode::ClientErrorConflictingAttributes);
    }
    let target_id = supplied_job_id.or(supplied_job_uri_id);
    if [0x0008, 0x0009].contains(&op) {
        let job = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .jobs
            .get(&target_id.unwrap_or(0))
            .filter(|j| j.path == path)
            .cloned();
        let Some(job) = job else {
            return error(StatusCode::ClientErrorNotFound);
        };
        if op == 0x0008 {
            let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(current) = state.jobs.get_mut(&job.id) {
                if current.state >= 7 {
                    return error(StatusCode::ClientErrorNotPossible);
                }
                if let Some(spool_id) = current.spool_id {
                    if print_job::cancel_spool_job(&current.printer, spool_id).is_err() {
                        return error(StatusCode::ClientErrorNotPossible);
                    }
                }
                current.canceled.store(true, Ordering::Release);
                if current.state == 4 {
                    current.state = 7;
                    current.reason = "job-canceled-by-user".into();
                }
            }
            return error(StatusCode::SuccessfulOk);
        }
        let mut response = error(StatusCode::SuccessfulOk);
        response
            .attributes_mut()
            .groups_mut()
            .push(job_group(&job, &uri));
        return response;
    }
    if ![0x0002, 0x0004, 0x0005, 0x0006].contains(&op) {
        return error(StatusCode::ServerErrorOperationNotSupported);
    }
    let existing = if op == 0x0006 {
        let state = shared.lock().unwrap_or_else(|e| e.into_inner());
        match state
            .jobs
            .get(&target_id.unwrap_or(0))
            .filter(|j| j.path == path && j.state == 4)
        {
            Some(job) => Some(job.clone()),
            None => return error(StatusCode::ClientErrorNotFound),
        }
    } else {
        None
    };
    if op == 0x0006 && attr(&req, "last-document") != Some(&IppValue::Boolean(true)) {
        return error(StatusCode::ServerErrorMultipleDocumentJobsNotSupported);
    }
    #[cfg(debug_assertions)]
    for name in [
        "orientation-requested",
        "number-up",
        "print-quality",
        "page-ranges",
        "print-scaling",
        "copies",
        "sides",
        "media",
        "print-color-mode",
        "media-col",
        "compression",
        "document-format",
    ] {
        if let Some(value) = attr(&req, name) {
            eprintln!("[IPP] job option {name}={value:?}");
        }
    }
    let opts = match options(&req, &printer, existing.as_ref().map(|j| &j.options)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[IPP] job options rejected: {e:?}");
            return error(e);
        }
    };
    let mut format = string_attr(&req, "document-format")
        .unwrap_or("application/pdf")
        .to_string();
    if format != "application/octet-stream" && !FORMATS.contains(&format.as_str()) {
        return error(StatusCode::ClientErrorDocumentFormatNotSupported);
    }
    if op == 0x0004 {
        return error(StatusCode::SuccessfulOk);
    }
    let owner = string_attr(&req, "requesting-user-name")
        .unwrap_or("anonymous")
        .to_string();
    let name = string_attr(&req, "job-name")
        .unwrap_or("AirPrint document")
        .to_string();
    let mut document = Vec::new();
    if req.into_payload().read_to_end(&mut document).is_err() {
        return error(StatusCode::ClientErrorBadRequest);
    }
    if format == "application/octet-stream" && op != 0x0005 {
        match FORMATS
            .iter()
            .find(|format| print_job::validate_document(format, &document))
        {
            Some(detected) => format = (*detected).to_string(),
            None => return error(StatusCode::ClientErrorDocumentFormatNotSupported),
        }
    }
    if op != 0x0005 && !print_job::validate_document(&format, &document) {
        return error(StatusCode::ClientErrorDocumentFormatError);
    }
    if op == 0x0005 && !document.is_empty() {
        return error(StatusCode::ClientErrorBadRequest);
    }
    let job = {
        let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
        if !state.printers.contains_key(path) {
            return error(StatusCode::ClientErrorNotFound);
        }
        if state.jobs.values().filter(|j| j.state < 7).count() >= 32 && existing.is_none() {
            return error(StatusCode::ServerErrorBusy);
        }
        if op != 0x0005
            && state
                .jobs
                .values()
                .filter(|j| matches!(j.state, 3 | 5 | 6))
                .count()
                >= 4
        {
            return error(StatusCode::ServerErrorBusy);
        }
        if state.jobs.len() >= 256 {
            if let Some(oldest) = state
                .jobs
                .values()
                .filter(|j| j.state >= 7)
                .map(|j| j.id)
                .min()
            {
                state.jobs.remove(&oldest);
            }
        }
        let mut job = if let Some(existing) = existing {
            if !state.jobs.get(&existing.id).is_some_and(|j| j.state == 4) {
                return error(StatusCode::ClientErrorNotPossible);
            }
            existing
        } else {
            let job_id = match NEXT_JOB_ID
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1))
            {
                Ok(id) => id,
                Err(_) => return error(StatusCode::ServerErrorBusy),
            };
            Job {
                id: job_id,
                path: path.into(),
                printer: printer.name.clone(),
                options: opts.clone(),
                owner,
                name,
                state: 4,
                reason: "job-incoming".into(),
                message: String::new(),
                canceled: Arc::new(AtomicBool::new(false)),
                spool_id: None,
                created: Instant::now(),
            }
        };
        job.options = opts;
        if op != 0x0005 {
            job.state = 3;
            job.reason = "job-queued".into();
        }
        state.jobs.insert(job.id, job.clone());
        job
    };
    let mut response = error(StatusCode::SuccessfulOk);
    response
        .attributes_mut()
        .groups_mut()
        .push(job_group(&job, &uri));
    if op != 0x0005 {
        run_job(shared.clone(), job, document, format);
    }
    response
}

fn job_group(job: &Job, uri: &str) -> IppAttributeGroup {
    let mut group = IppAttributeGroup::new(DelimiterTag::JobAttributes);
    for (name, value) in [
        ("job-id", IppValue::Integer(job.id)),
        ("job-uri", IppValue::Uri(format!("{uri}/jobs/{}", job.id))),
        ("job-printer-uri", IppValue::Uri(uri.into())),
        ("job-state", IppValue::Enum(job.state)),
        ("job-state-reasons", IppValue::Keyword(job.reason.clone())),
        (
            "job-state-message",
            IppValue::TextWithoutLanguage(job.message.clone()),
        ),
        ("job-name", IppValue::NameWithoutLanguage(job.name.clone())),
        (
            "job-originating-user-name",
            IppValue::NameWithoutLanguage(job.owner.clone()),
        ),
    ] {
        group
            .attributes_mut()
            .insert(name.into(), IppAttribute::new(name, value));
    }
    group
}

fn run_job(shared: Shared, job: Job, document: Vec<u8>, format: String) {
    eprintln!(
        "[IPP] job={} received document bytes={} format={format}",
        job.id,
        document.len()
    );
    thread::spawn(move || {
        let backend = shared.lock().unwrap_or_else(|e| e.into_inner()).backend;
        set_job(&shared, job.id, 5, "job-transforming", "");
        match backend(
            &job.printer,
            &document,
            &format,
            &job.options,
            &job.canceled,
        ) {
            Err(error) => {
                if job.canceled.load(Ordering::Acquire) {
                    set_job(&shared, job.id, 7, "job-canceled-by-user", "");
                } else {
                    set_job(&shared, job.id, 8, "document-unprintable-error", &error);
                }
            }
            Ok(spool_id) => {
                eprintln!("[IPP] job={} Windows spool_id={spool_id}", job.id);
                if let Some(current) = shared
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .jobs
                    .get_mut(&job.id)
                {
                    current.spool_id = Some(spool_id);
                }
                loop {
                    if job.canceled.load(Ordering::Acquire) {
                        let _ = print_job::cancel_spool_job(&job.printer, spool_id);
                        set_job(&shared, job.id, 7, "job-canceled-by-user", "");
                        break;
                    }
                    #[cfg(test)]
                    if spool_id == 0 {
                        set_job(
                            &shared,
                            job.id,
                            9,
                            "job-completed-successfully",
                            "Simulated backend",
                        );
                        break;
                    }
                    match print_job::spool_state(&job.printer, spool_id) {
                        Ok(None) => {
                            set_job(&shared, job.id, 9, "job-completed-successfully", "Job left the Windows spool queue; physical output is not independently confirmed");
                            break;
                        }
                        Ok(Some(status)) if status & (0x80 | 0x1000) != 0 => {
                            set_job(
                                &shared,
                                job.id,
                                9,
                                "job-completed-successfully",
                                "Windows reports completion",
                            );
                            break;
                        }
                        Ok(Some(status)) if status & (0x4 | 0x100) != 0 => {
                            set_job(
                                &shared,
                                job.id,
                                7,
                                "job-canceled-at-device",
                                "Windows removed the job",
                            );
                            break;
                        }
                        Ok(Some(status)) if status & (0x1 | 0x2 | 0x20 | 0x40 | 0x200) != 0 => {
                            set_job(
                                &shared,
                                job.id,
                                6,
                                "printer-stopped",
                                "Windows printer requires attention",
                            )
                        }
                        Ok(Some(_)) => set_job(
                            &shared,
                            job.id,
                            5,
                            "job-queued-in-device",
                            "Submitted to the selected Windows queue",
                        ),
                        Err(error) => set_job(&shared, job.id, 6, "printer-stopped", &error),
                    }
                    thread::sleep(Duration::from_secs(2));
                }
            }
        }
    });
}
fn set_job(shared: &Shared, id: i32, state: i32, reason: &str, message: &str) {
    if let Some(job) = shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .jobs
        .get_mut(&id)
    {
        if job.state != state || job.message != message {
            eprintln!("[IPP] job={id} state={state} reason={reason} message={message}");
        }
        if job.canceled.load(Ordering::Acquire) && state >= 7 {
            job.state = 7;
            job.reason = "job-canceled-by-user".into();
            job.message.clear();
        } else {
            job.state = state;
            job.reason = reason.into();
            job.message = message.chars().take(1024).collect();
        }
    }
}

fn printer_attributes(
    shared: &Shared,
    printer: &Printer,
    uri: &str,
    version: IppVersion,
    id: u32,
    requested: Option<&IppValue>,
) -> IppRequestResponse {
    let state = shared.lock().unwrap_or_else(|e| e.into_inner());
    let active = state
        .jobs
        .values()
        .filter(|j| j.printer == printer.name && j.state < 7)
        .count();
    let available = matches!(printer.status, PrinterStatus::Online | PrinterStatus::Busy);
    let caps = &printer.capabilities;
    let mut response = reply(version, id, StatusCode::SuccessfulOk);
    let mut put = |name: &str, value| {
        if wants_printer_attribute(requested, name) {
            add(&mut response, DelimiterTag::PrinterAttributes, name, value);
        }
    };
    put("printer-uri-supported", IppValue::Uri(uri.into()));
    let hostname = state
        .printers
        .values()
        .find(|(p, _)| p.id == printer.id)
        .map(|(_, h)| h.as_str())
        .unwrap_or("");
    put(
        "printer-uuid",
        IppValue::Uri(format!("urn:uuid:{}", printer.uuid(hostname))),
    );
    put(
        "printer-name",
        IppValue::NameWithoutLanguage(printer.name.clone()),
    );
    put(
        "printer-info",
        IppValue::TextWithoutLanguage(printer.name.clone()),
    );
    put(
        "printer-state-message",
        IppValue::TextWithoutLanguage(
            if available {
                "Windows print queue available"
            } else {
                "Windows print queue unavailable"
            }
            .into(),
        ),
    );
    put("printer-kind", keywords(&["document"]));
    put(
        "printer-make-and-model",
        IppValue::TextWithoutLanguage(printer.name.clone()),
    );
    put(
        "printer-state",
        IppValue::Enum(if !available {
            5
        } else if active > 0 {
            4
        } else {
            3
        }),
    );
    put(
        "printer-state-reasons",
        keywords(if available { &["none"] } else { &["offline"] }),
    );
    put("printer-is-accepting-jobs", IppValue::Boolean(available));
    put(
        "printer-up-time",
        IppValue::Integer(state.started.elapsed().as_secs().min(i32::MAX as u64) as i32),
    );
    put("queued-job-count", IppValue::Integer(active as i32));
    put(
        "which-jobs-supported",
        keywords(&["completed", "not-completed", "all"]),
    );
    put("uri-authentication-supported", keywords(&["none"]));
    put(
        "uri-security-supported",
        keywords(&[if uri.starts_with("ipps:") {
            "tls"
        } else {
            "none"
        }]),
    );
    put("ipp-versions-supported", keywords(&["1.1", "2.0"]));
    // iOS/iPadOS 的系统打印对话框要求该标记才会列出打印机（见 AGENTS.md）。
    put("ipp-features-supported", keywords(&["ipp-everywhere"]));
    put(
        "operations-supported",
        IppValue::Array([2, 4, 5, 6, 8, 9, 10, 11].map(IppValue::Enum).to_vec()),
    );
    put(
        "document-format-supported",
        IppValue::Array(
            FORMATS
                .iter()
                .copied()
                .chain(std::iter::once("application/octet-stream"))
                .map(|s| IppValue::MimeMediaType(s.into()))
                .collect(),
        ),
    );
    put(
        "document-format-default",
        IppValue::MimeMediaType("application/pdf".into()),
    );
    put("document-password-supported", IppValue::Integer(0));
    put(
        "pdf-versions-supported",
        keywords(&[
            "adobe-1.3",
            "adobe-1.4",
            "adobe-1.5",
            "adobe-1.6",
            "adobe-1.7",
        ]),
    );
    put("compression-supported", keywords(&["none"]));
    put("compression-default", IppValue::Keyword("none".into()));
    put("charset-configured", IppValue::Charset("utf-8".into()));
    put("charset-supported", IppValue::Charset("utf-8".into()));
    put(
        "natural-language-configured",
        IppValue::NaturalLanguage("en".into()),
    );
    put(
        "generated-natural-language-supported",
        IppValue::NaturalLanguage("en".into()),
    );
    put("copies-default", IppValue::Integer(1));
    put("orientation-requested-default", IppValue::Enum(3));
    put(
        "orientation-requested-supported",
        IppValue::Array([3, 4, 5, 6].map(IppValue::Enum).to_vec()),
    );
    put("number-up-default", IppValue::Integer(1));
    put("number-up-supported", IppValue::Integer(1));
    put("page-ranges-supported", IppValue::Boolean(false));
    put("print-quality-default", IppValue::Enum(4));
    put(
        "print-quality-supported",
        IppValue::Array([3, 4, 5].map(IppValue::Enum).to_vec()),
    );
    put("print-scaling-default", IppValue::Keyword("fit".into()));
    put("print-scaling-supported", keywords(&["fit", "auto"]));
    put(
        "copies-supported",
        IppValue::RangeOfInteger {
            min: 1,
            max: caps.max_copies,
        },
    );
    put("color-supported", IppValue::Boolean(caps.color));
    put("print-color-mode-default", IppValue::Keyword("auto".into()));
    put(
        "print-color-mode-supported",
        keywords(if caps.color {
            &["monochrome", "color", "auto"]
        } else {
            &["monochrome", "auto"]
        }),
    );
    put("sides-default", IppValue::Keyword("one-sided".into()));
    put(
        "sides-supported",
        keywords(if caps.duplex {
            &["one-sided", "two-sided-long-edge", "two-sided-short-edge"]
        } else {
            &["one-sided"]
        }),
    );
    put(
        "media-default",
        IppValue::Keyword(caps.default_media.clone()),
    );
    put("media-type-default", IppValue::Keyword("stationery".into()));
    put("media-type-supported", keywords(&["stationery"]));
    put("media-source-supported", keywords(&["auto"]));
    put("finishings-default", IppValue::Enum(3));
    put("finishings-supported", IppValue::Enum(3));
    put("media-ready", IppValue::Keyword(caps.default_media.clone()));
    put(
        "media-supported",
        IppValue::Array(
            caps.media
                .iter()
                .map(|s| IppValue::Keyword(s.clone()))
                .collect(),
        ),
    );
    let media_col = |name: &str| {
        let paper = caps.media_sizes.iter().find(|m| m.name == name);
        let (width, height) = paper.map(|m| (m.width, m.height)).unwrap_or((21000, 29700));
        IppValue::Collection(
            [
                (
                    "media-size".into(),
                    IppValue::Collection(
                        [
                            ("x-dimension".into(), IppValue::Integer(width)),
                            ("y-dimension".into(), IppValue::Integer(height)),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
                ("media-size-name".into(), IppValue::Keyword(name.into())),
                ("media-top-margin".into(), IppValue::Integer(635)),
                ("media-bottom-margin".into(), IppValue::Integer(635)),
                ("media-left-margin".into(), IppValue::Integer(635)),
                ("media-right-margin".into(), IppValue::Integer(635)),
            ]
            .into_iter()
            .collect(),
        )
    };
    put("media-col-default", media_col(&caps.default_media));
    put("media-col-ready", media_col(&caps.default_media));
    put(
        "media-col-database",
        IppValue::Array(caps.media.iter().map(|m| media_col(m)).collect()),
    );
    put(
        "media-col-supported",
        keywords(&["media-size", "media-size-name"]),
    );
    put(
        "urf-supported",
        keywords(&["V1.4", "W8", "SRGB24", "RS300"]),
    );
    put(
        "pwg-raster-document-type-supported",
        keywords(&["sgray_8", "srgb_8"]),
    );
    put(
        "pwg-raster-document-sheet-back",
        IppValue::Keyword("normal".into()),
    );
    for name in [
        "printer-resolution-default",
        "printer-resolution-supported",
        "pwg-raster-document-resolution-supported",
    ] {
        put(
            name,
            IppValue::Resolution {
                cross_feed: 300,
                feed: 300,
                units: 3,
            },
        );
    }
    put(
        "pdl-override-supported",
        IppValue::Keyword("attempted".into()),
    );
    put("multiple-document-jobs-supported", IppValue::Boolean(false));
    put("multiple-operation-timeout", IppValue::Integer(60));
    put(
        "multiple-document-handling-default",
        IppValue::Keyword("separate-documents-collated-copies".into()),
    );
    put(
        "multiple-document-handling-supported",
        keywords(&["separate-documents-collated-copies"]),
    );
    put(
        "job-creation-attributes-supported",
        keywords(&[
            "copies",
            "sides",
            "media",
            "media-col",
            "print-color-mode",
            "orientation-requested",
            "print-scaling",
            "job-name",
        ]),
    );
    response
}
#[cfg(test)]
mod tests {
    use super::*;
    use ipp::model::Operation;
    use std::io::Write;
    use std::net::TcpStream;
    use std::sync::mpsc;

    fn printer(name: &str) -> Printer {
        Printer {
            name: name.into(),
            id: name.into(),
            status: PrinterStatus::Online,
            capabilities: Default::default(),
        }
    }
    fn req(op: Operation, uri: &str) -> IppRequestResponse {
        // Preserve the wire URI: ipp 5.4's client constructor rewrites ipps to ipp.
        let mut request = IppRequestResponse::new(IppVersion::v2_0(), op, None);
        add(
            &mut request,
            DelimiterTag::OperationAttributes,
            "printer-uri",
            IppValue::Uri(uri.into()),
        );
        request
    }
    fn job_id(response: &IppRequestResponse) -> i32 {
        match attr(response, "job-id").unwrap() {
            IppValue::Integer(id) => *id,
            _ => panic!("Missing job ID"),
        }
    }
    fn http(address: &str, path: &str, body: &[u8], host: &str) -> (u16, Vec<u8>) {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(socket, "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        socket.write_all(body).unwrap();
        let mut response = Vec::new();
        socket.read_to_end(&mut response).unwrap();
        let boundary = response.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
        let status = std::str::from_utf8(&response[..boundary])
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        (status, response[boundary + 4..].to_vec())
    }

    #[test]
    fn http_routes_two_queues_and_releases_listener() {
        let mut server = IppServer::new("127.0.0.1", 0);
        let a = printer("办公室 A");
        let b = printer("USB B");
        server.add_printer(a.clone(), "pc.local.").unwrap();
        server.add_printer(b.clone(), "pc.local.").unwrap();
        server.start().unwrap();
        let address = server.address.clone();
        for printer in [&a, &b] {
            let path = format!("/{}", printer.resource_path());
            let uri = format!("ipp://{address}{path}");
            let request = req(Operation::GetPrinterAttributes, &uri);
            let (status, body) = http(&address, &path, &request.to_bytes(), &address);
            assert_eq!(status, 200);
            let response = IppParser::new(IppReader::new(Cursor::new(body)))
                .parse()
                .unwrap();
            assert_eq!(
                string_attr(&response, "printer-name"),
                Some(printer.name.as_str())
            );
            assert_eq!(
                string_attr(&response, "printer-uri-supported"),
                Some(uri.as_str())
            );
        }
        let path = format!("/{}", a.resource_path());
        let request = req(
            Operation::GetPrinterAttributes,
            &format!("ipp://{address}{path}"),
        );
        assert_eq!(
            http(&address, &path, &request.to_bytes(), "evil.example:631").0,
            400
        );
        server.remove_printer(&a).unwrap();
        assert_eq!(http(&address, &path, &request.to_bytes(), &address).0, 404);
        drop(server);
        // tiny_http owns a short-lived accept thread that releases its socket on drop.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match std::net::TcpListener::bind(&address) {
                Ok(_) => break,
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Err(e) => panic!("Listener was not released: {e}"),
            }
        }
    }

    #[test]
    fn occupied_standard_port_uses_an_advertisable_alternate() {
        let mut first = IppServer::new("127.0.0.1", 631);
        first.start().unwrap();
        let mut second = IppServer::new("127.0.0.1", 631);
        second.start().unwrap();
        assert_ne!(first.port().unwrap(), second.port().unwrap());
        assert!((8631..=8699).contains(&second.port().unwrap()));
        std::net::TcpStream::connect(("127.0.0.1", second.port().unwrap())).unwrap();
    }

    #[test]
    fn create_send_validate_cancel_and_job_ids() {
        let server = IppServer::new("127.0.0.1", 0);
        let p = printer("test");
        server.add_printer(p.clone(), "pc.local.").unwrap();
        let path = format!("/{}", p.resource_path());
        let host = "127.0.0.1:631";
        let uri = format!("ipp://{host}{path}");
        let created = process(&server.shared, &path, host, req(Operation::CreateJob, &uri));
        let id = job_id(&created);
        assert_eq!(attr(&created, "job-state"), Some(&IppValue::Enum(4)));
        let again = process(&server.shared, &path, host, req(Operation::CreateJob, &uri));
        assert_ne!(job_id(&again), id);
        let mut cancel = req(Operation::CancelJob, &uri);
        add(
            &mut cancel,
            DelimiterTag::OperationAttributes,
            "job-id",
            IppValue::Integer(id),
        );
        assert_eq!(
            process(&server.shared, &path, host, cancel)
                .header()
                .operation_or_status,
            0
        );
        let mut get = req(Operation::GetJobAttributes, &uri);
        add(
            &mut get,
            DelimiterTag::OperationAttributes,
            "job-id",
            IppValue::Integer(id),
        );
        assert_eq!(
            attr(&process(&server.shared, &path, host, get), "job-state"),
            Some(&IppValue::Enum(7))
        );
        let mut validation = req(Operation::ValidateJob, &uri);
        add(
            &mut validation,
            DelimiterTag::JobAttributes,
            "copies",
            IppValue::Integer(999),
        );
        assert_eq!(
            process(&server.shared, &path, host, validation)
                .header()
                .operation_or_status,
            StatusCode::ClientErrorAttributesOrValuesNotSupported as u16
        );
        let mut send = req(Operation::SendDocument, &uri);
        add(
            &mut send,
            DelimiterTag::OperationAttributes,
            "job-id",
            IppValue::Integer(job_id(&again)),
        );
        add(
            &mut send,
            DelimiterTag::OperationAttributes,
            "last-document",
            IppValue::Boolean(false),
        );
        assert_eq!(
            process(&server.shared, &path, host, send)
                .header()
                .operation_or_status,
            StatusCode::ServerErrorMultipleDocumentJobsNotSupported as u16
        );
    }

    #[test]
    fn ipp_uris_may_omit_the_default_631_port() {
        assert!(valid_host("pc.local", "pc.local.", 631));
        assert!(valid_host("127.0.0.1", "pc.local.", 631));
        assert!(!valid_host("pc.local", "pc.local.", 8631));
        assert!(!valid_host("other.local", "pc.local.", 631));
        let server = IppServer::new("127.0.0.1", 0);
        let p = printer("default port client");
        server.add_printer(p.clone(), "pc.local.").unwrap();
        let path = format!("/{}", p.resource_path());
        let host = "127.0.0.1:631";
        let without_port = format!("ipp://127.0.0.1{path}");

        let attributes = process(
            &server.shared,
            &path,
            host,
            req(Operation::GetPrinterAttributes, &without_port),
        );
        assert_eq!(attributes.header().operation_or_status, 0);
        let bonjour_uri = format!("ipp://pc.local{path}");
        assert_eq!(
            process(
                &server.shared,
                &path,
                "pc.local",
                req(Operation::GetPrinterAttributes, &bonjour_uri)
            )
            .header()
            .operation_or_status,
            0
        );
        let mut create = req(Operation::CreateJob, &without_port);
        add(
            &mut create,
            DelimiterTag::JobAttributes,
            "print-quality",
            IppValue::Enum(5),
        );
        let created = process(&server.shared, &path, "127.0.0.1", create);
        assert_eq!(created.header().operation_or_status, 0);
        let id = job_id(&created);

        let mut lookup = req(Operation::GetJobAttributes, &without_port);
        add(
            &mut lookup,
            DelimiterTag::OperationAttributes,
            "job-uri",
            IppValue::Uri(format!("{without_port}/jobs/{id}")),
        );
        let result = process(&server.shared, &path, "127.0.0.1", lookup);
        assert_eq!(attr(&result, "job-state"), Some(&IppValue::Enum(4)));
        assert!(!valid_ipp_authority("127.0.0.1", "pc.local.", 8631));
    }

    #[test]
    fn custom_driver_media_is_matched_by_dimensions() {
        let mut p = printer("label printer");
        p.capabilities.media = vec!["custom_win256_4x6in".into()];
        p.capabilities.default_media = p.capabilities.media[0].clone();
        p.capabilities.media_sizes = vec![crate::models::printer::MediaSize {
            name: p.capabilities.default_media.clone(),
            width: 10160,
            height: 15240,
            windows_kind: 256,
        }];
        let mut request = req(Operation::ValidateJob, "ipp://127.0.0.1:631/ipp/print/test");
        add(
            &mut request,
            DelimiterTag::JobAttributes,
            "media-col",
            IppValue::Collection(
                [(
                    "media-size".into(),
                    IppValue::Collection(
                        [
                            ("x-dimension".into(), IppValue::Integer(10160)),
                            ("y-dimension".into(), IppValue::Integer(15240)),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                )]
                .into_iter()
                .collect(),
            ),
        );
        add(
            &mut request,
            DelimiterTag::JobAttributes,
            "print-quality",
            IppValue::Enum(5),
        );
        let selected = options(&request, &p, None).unwrap();
        assert_eq!(selected.quality, 5);
        assert_eq!(
            (
                selected.paper_kind,
                selected.paper_width,
                selected.paper_height
            ),
            (256, 400, 600)
        );
    }

    #[test]
    fn secure_requests_keep_ipps_uris_for_printer_and_jobs() {
        let server = IppServer::new("127.0.0.1", 0);
        let p = printer("secure queue");
        server.add_printer(p.clone(), "pc.local.").unwrap();
        let path = format!("/{}", p.resource_path());
        let host = "pc.local:8631";
        let uri = format!("ipps://{host}{path}");
        let response = process_request(
            &server.shared,
            &path,
            host,
            req(Operation::GetPrinterAttributes, &uri),
            true,
        );
        assert_eq!(response.header().operation_or_status, 0);
        assert_eq!(
            string_attr(&response, "printer-uri-supported"),
            Some(uri.as_str())
        );
        assert_eq!(
            attr(&response, "uri-security-supported"),
            Some(&keywords(&["tls"]))
        );
        let create = process_request(
            &server.shared,
            &path,
            host,
            req(Operation::CreateJob, &uri),
            true,
        );
        assert_eq!(create.header().operation_or_status, 0);
        let job_uri = string_attr(&create, "job-uri").unwrap();
        assert!(job_uri.starts_with("ipps://pc.local:8631/"));
        let mut lookup = req(Operation::GetJobAttributes, &uri);
        add(
            &mut lookup,
            DelimiterTag::OperationAttributes,
            "job-uri",
            IppValue::Uri(job_uri.into()),
        );
        assert_eq!(
            process_request(&server.shared, &path, host, lookup, true)
                .header()
                .operation_or_status,
            0
        );
        assert_ne!(
            process_request(
                &server.shared,
                &path,
                host,
                req(Operation::GetPrinterAttributes, &uri),
                false
            )
            .header()
            .operation_or_status,
            0
        );
    }

    #[test]
    fn printer_attributes_use_protocol_names_and_color_default() {
        let server = IppServer::new("127.0.0.1", 0);
        let mut p = printer("color queue");
        p.capabilities.color = true;
        server.add_printer(p.clone(), "pc.local.").unwrap();
        let path = format!("/{}", p.resource_path());
        let host = "127.0.0.1:631";
        let uri = format!("ipp://{host}{path}");
        let response = process(
            &server.shared,
            &path,
            host,
            req(Operation::GetPrinterAttributes, &uri),
        );
        assert_eq!(
            attr(&response, "multiple-operation-timeout"),
            Some(&IppValue::Integer(60))
        );
        assert_eq!(attr(&response, "multiple-operation-time-out"), None);
        assert_eq!(
            string_attr(&response, "print-color-mode-default"),
            Some("auto")
        );
        assert!(attr(&response, "which-jobs-supported").is_some());
        assert_eq!(
            attr(&response, "printer-make-and-model"),
            Some(&IppValue::TextWithoutLanguage(p.name.clone()))
        );

        let mut request = req(Operation::GetPrinterAttributes, &uri);
        add(
            &mut request,
            DelimiterTag::OperationAttributes,
            "requested-attributes",
            keywords(&["printer-state-reasons", "media-source-supported"]),
        );
        let filtered = process(&server.shared, &path, host, request);
        assert!(attr(&filtered, "printer-state-reasons").is_some());
        assert_eq!(
            attr(&filtered, "media-source-supported"),
            Some(&keywords(&["auto"]))
        );
        assert!(attr(&filtered, "printer-name").is_none());
    }

    #[test]
    fn print_acknowledges_pending_and_reports_actual_backend_error() {
        static QUEUE: std::sync::OnceLock<Mutex<Option<mpsc::Sender<String>>>> =
            std::sync::OnceLock::new();
        fn failure(
            queue: &str,
            _: &[u8],
            _: &str,
            _: &PrintOptions,
            _: &AtomicBool,
        ) -> Result<i32, String> {
            QUEUE
                .get()
                .unwrap()
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .send(queue.into())
                .unwrap();
            Err("simulated driver failure".into())
        }
        let (tx, rx) = mpsc::channel();
        *QUEUE.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(tx);
        let server = IppServer::new("127.0.0.1", 0);
        let p = printer("physical-queue-B");
        server.add_printer(p.clone(), "pc.local.").unwrap();
        server.shared.lock().unwrap().backend = failure;
        let path = format!("/{}", p.resource_path());
        let host = "127.0.0.1:631";
        let uri = format!("ipp://{host}{path}");
        let mut request = req(Operation::PrintJob, &uri);
        *request.payload_mut() =
            ipp::payload::IppPayload::new(Cursor::new(b"%PDF-simulated".to_vec()));
        let response = process(&server.shared, &path, host, request);
        let id = job_id(&response);
        assert_eq!(attr(&response, "job-state"), Some(&IppValue::Enum(3)));
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), p.name);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let job = server
                .shared
                .lock()
                .unwrap()
                .jobs
                .get(&id)
                .cloned()
                .unwrap();
            if job.state == 8 {
                assert!(job.message.contains("driver failure"));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "Full local HTTP-to-Windows simulation; prints only to a PDF file"]
    fn http_to_windows_pdf_end_to_end() {
        let output_directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/airprint-simulation");
        std::fs::create_dir_all(&output_directory).unwrap();
        let output = output_directory.join(format!("http-urf-{}.pdf", std::process::id()));
        print_job::TEST_OUTPUT.set(output.clone()).unwrap();
        let mut server = IppServer::new("127.0.0.1", 0);
        let mut p = printer("Microsoft Print to PDF");
        p.capabilities.color = true;
        server.add_printer(p.clone(), "simulation.local.").unwrap();
        server.shared.lock().unwrap().backend = print_job::submit_test_pdf;
        server.start().unwrap();
        let address = server.address.clone();
        let path = format!("/{}", p.resource_path());
        let uri = format!("ipp://{address}{path}");
        let mut data = b"UNIRAST\0".to_vec();
        data.extend(1u32.to_be_bytes());
        let mut header = [0; 32];
        header[0] = 24;
        header[1] = 1;
        header[12..16].copy_from_slice(&100u32.to_be_bytes());
        header[16..20].copy_from_slice(&100u32.to_be_bytes());
        header[20..24].copy_from_slice(&300u32.to_be_bytes());
        data.extend(header);
        // Four quadrants: red, green, blue, white. Exercises both row and pixel RLE.
        data.extend([
            49, 49, 255, 0, 0, 49, 0, 255, 0, 49, 49, 0, 0, 255, 49, 255, 255, 255,
        ]);
        let mut request = req(Operation::PrintJob, &uri);
        add(
            &mut request,
            DelimiterTag::OperationAttributes,
            "document-format",
            IppValue::MimeMediaType("image/urf".into()),
        );
        add(
            &mut request,
            DelimiterTag::JobAttributes,
            "print-color-mode",
            IppValue::Keyword("color".into()),
        );
        let mut body = request.to_bytes().to_vec();
        body.extend(data);
        let (status, body) = http(&address, &path, &body, &address);
        assert_eq!(status, 200);
        let response = IppParser::new(IppReader::new(Cursor::new(body)))
            .parse()
            .unwrap();
        let id = job_id(&response);
        assert_eq!(attr(&response, "job-state"), Some(&IppValue::Enum(3)));
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            let mut request = req(Operation::GetJobAttributes, &uri);
            add(
                &mut request,
                DelimiterTag::OperationAttributes,
                "job-id",
                IppValue::Integer(id),
            );
            let (_, body) = http(
                &address,
                &format!("{path}/jobs/{id}"),
                &request.to_bytes(),
                &address,
            );
            let response = IppParser::new(IppReader::new(Cursor::new(body)))
                .parse()
                .unwrap();
            if attr(&response, "job-state") == Some(&IppValue::Enum(9)) {
                break;
            }
            assert_ne!(
                attr(&response, "job-state"),
                Some(&IppValue::Enum(8)),
                "{:?}",
                attr(&response, "job-state-message")
            );
            assert!(
                Instant::now() < deadline,
                "Spool status did not complete: {:?}",
                attr(&response, "job-state-message")
            );
            thread::sleep(Duration::from_millis(250));
        }
        assert!(std::fs::read(&output).unwrap().starts_with(b"%PDF-"));
        println!("Simulation PDF: {}", output.display());
    }
}
