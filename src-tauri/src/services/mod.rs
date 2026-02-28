pub mod printer_detector;
pub use printer_detector::PrinterDetector;

pub mod airprint_server;
pub use airprint_server::AirPrintServer;

pub mod mdns_broadcaster;
pub use mdns_broadcaster::MdnsBroadcaster;

pub mod ipp;
pub use ipp::IppServer;

use rust_i18n::t;