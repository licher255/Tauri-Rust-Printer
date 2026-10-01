// src-tauri/src/lib.rs
rust_i18n::i18n!("locales", fallback = "en");

pub mod commands;
pub mod models;
pub mod services;
pub mod tray;

pub use commands::*;
