// src-tauri/src/print_job.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use chrono::{DateTime, Local};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintJob {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub source_device: String, // 来自哪个设备（iPhone/iPad 名称）
    pub source_ip: String,
    pub copies: i32,
    pub sides: String,
    pub color_mode: String,
    pub media: String,
    pub status: JobStatus,
    pub created_at: String,
    pub file_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,    // 等待用户确认
    Printing,   // 正在打印
    Completed,  // 完成
    Failed,     // 失败
    Cancelled,  // 用户取消
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Pending => write!(f, "pending"),
            JobStatus::Printing => write!(f, "printing"),
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
            JobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

pub struct PrintJobManager {
    jobs: Arc<Mutex<HashMap<String, PrintJob>>>,
    temp_dir: PathBuf,
}

impl PrintJobManager {
    pub fn new() -> Self {
        let temp_dir = std::env::temp_dir().join("airprinter_jobs");
        std::fs::create_dir_all(&temp_dir).unwrap_or_default();
        
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            temp_dir,
        }
    }

    pub fn create_job(&self, data: Vec<u8>, options: PrintOptions, source_ip: String) -> PrintJob {
        let id = Uuid::new_v4().to_string();
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let file_name = format!("job_{}_{}.pdf", timestamp, &id[..8]);
        let file_path = self.temp_dir.join(&file_name);
        
        // 保存文件
        std::fs::write(&file_path, &data).expect("Failed to save print file");
        
        let job = PrintJob {
            id: id.clone(),
            file_path: file_path.to_string_lossy().to_string(),
            file_name,
            source_device: options.job_name.unwrap_or_else(|| "Unknown Device".to_string()),
            source_ip,
            copies: options.copies,
            sides: options.sides,
            color_mode: options.color_mode,
            media: options.media,
            status: JobStatus::Pending,
            created_at: Local::now().to_rfc3339(),
            file_size: data.len() as u64,
        };
        
        self.jobs.lock().unwrap().insert(id.clone(), job.clone());
        job
    }

    pub fn get_job(&self, id: &str) -> Option<PrintJob> {
        self.jobs.lock().unwrap().get(id).cloned()
    }

    pub fn update_status(&self, id: &str, status: JobStatus) {
        if let Some(job) = self.jobs.lock().unwrap().get_mut(id) {
            job.status = status;
        }
    }

    pub fn get_pending_jobs(&self) -> Vec<PrintJob> {
        self.jobs.lock().unwrap()
            .values()
            .filter(|j| matches!(j.status, JobStatus::Pending))
            .cloned()
            .collect()
    }

    pub fn cleanup_job(&self, id: &str) {
        if let Some(job) = self.jobs.lock().unwrap().remove(id) {
            let _ = std::fs::remove_file(&job.file_path);
        }
    }
}

#[derive(Debug, Default)]
pub struct PrintOptions {
    pub copies: i32,
    pub sides: String,
    pub color_mode: String,
    pub media: String,
    pub job_name: Option<String>,
}