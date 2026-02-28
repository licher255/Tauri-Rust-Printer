use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Printer {
    pub name: String,
    pub id: String,
    pub status: PrinterStatus,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PrinterStatus {
    Online,
    Offline,
    Busy,
    Error(String),
}

impl PrinterStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrinterStatus::Online => "online",
            PrinterStatus::Offline => "offline",
            PrinterStatus::Busy => "busy",
            PrinterStatus::Error(_) => "error",
        }
    }
}