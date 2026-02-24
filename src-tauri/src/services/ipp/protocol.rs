// IPP 操作码
pub const OPERATION_GET_PRINTER_ATTRIBUTES: u16 = 0x000B;
pub const OPERATION_PRINT_JOB: u16 = 0x0002;
pub const OPERATION_VALIDATE_JOB: u16 = 0x0026;

// IPP 属性组标签
pub const OPERATION_ATTRIBUTES_TAG: u8 = 0x01;
pub const PRINTER_ATTRIBUTES_TAG: u8 = 0x04;
pub const END_OF_ATTRIBUTES_TAG: u8 = 0x03;

// 常用属性
pub const ATTR_PRINTER_URI: &str = "printer-uri";
pub const ATTR_REQUESTING_USER_NAME: &str = "requesting-user-name";
pub const ATTR_DOCUMENT_FORMAT: &str = "document-format";
pub const ATTR_JOB_NAME: &str = "job-name";