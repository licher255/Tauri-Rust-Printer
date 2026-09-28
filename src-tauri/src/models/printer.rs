use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Printer {
    pub name: String,
    pub id: String,
    pub status: PrinterStatus,
    #[serde(default)]
    pub capabilities: PrinterCapabilities,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PrinterCapabilities {
    pub color: bool,
    pub duplex: bool,
    pub max_copies: i32,
    pub media: Vec<String>,
    pub default_media: String,
    #[serde(default = "default_media_sizes")]
    pub media_sizes: Vec<MediaSize>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MediaSize {
    pub name: String,
    /// IPP dimensions, in hundredths of a millimetre.
    pub width: i32,
    pub height: i32,
    pub windows_kind: i32,
}

fn default_media_sizes() -> Vec<MediaSize> {
    vec![MediaSize {
        name: "iso_a4_210x297mm".into(),
        width: 21000,
        height: 29700,
        windows_kind: 9,
    }]
}

impl Default for PrinterCapabilities {
    fn default() -> Self {
        Self {
            color: false,
            duplex: false,
            max_copies: 1,
            media: vec!["iso_a4_210x297mm".into()],
            default_media: "iso_a4_210x297mm".into(),
            media_sizes: default_media_sizes(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

impl Printer {
    /// A bounded, stable URI even when a Windows queue has a long Unicode name.
    pub fn resource_path(&self) -> String {
        format!(
            "ipp/print/{}",
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, self.id.as_bytes()).simple()
        )
    }

    pub fn uuid(&self, hostname: &str) -> String {
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_DNS,
            format!("{hostname}/{}", self.id).as_bytes(),
        )
        .to_string()
    }
}

pub fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    let mut end = max_bytes.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_names_and_resource_paths_are_safe() {
        assert_eq!(truncate_utf8("办公室打印机", 8), "办公");
        let printer = Printer {
            name: "办公室".into(),
            id: "a/b %中".into(),
            status: PrinterStatus::Online,
            capabilities: PrinterCapabilities::default(),
        };
        assert_eq!(printer.resource_path().len(), 42);
        assert!(printer.resource_path().is_ascii());
        assert_eq!(printer.clone().resource_path(), printer.resource_path());
    }
}
