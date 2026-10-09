//! Diagnostic adapter for PWG ippeveprinter's -c command, using our Windows backend.
use airprinter::{
    models::Printer,
    services::{
        print_job::{self, PrintOptions},
        PrinterDetector,
    },
};
use std::{
    collections::HashMap,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn options(printer: &Printer, env: &HashMap<String, String>) -> Result<PrintOptions, String> {
    let value =
        |name: &str, default: &str| env.get(name).cloned().unwrap_or_else(|| default.into());
    let media = value("IPP_MEDIA", &printer.capabilities.default_media);
    let paper = printer
        .capabilities
        .media_sizes
        .iter()
        .find(|p| p.name == media)
        .ok_or_else(|| format!("Windows driver does not support media {media}"))?;
    let copies: i32 = value("IPP_COPIES", "1")
        .parse()
        .map_err(|_| "Invalid copies")?;
    if copies < 1 || copies > printer.capabilities.max_copies {
        return Err("Unsupported copies".into());
    }
    let quality = match value("IPP_PRINT_QUALITY", "normal").as_str() {
        "draft" | "3" => 3,
        "normal" | "4" => 4,
        "high" | "5" => 5,
        _ => return Err("Unsupported print quality".into()),
    };
    let orientation = match value("IPP_ORIENTATION_REQUESTED", "portrait").as_str() {
        "portrait" | "3" => 3,
        "landscape" | "4" => 4,
        "reverse-landscape" | "5" => 5,
        "reverse-portrait" | "6" => 6,
        _ => return Err("Unsupported orientation".into()),
    };
    let sides = value("IPP_SIDES", "one-sided");
    if !["one-sided", "two-sided-long-edge", "two-sided-short-edge"].contains(&sides.as_str())
        || (sides != "one-sided" && !printer.capabilities.duplex)
    {
        return Err("Unsupported duplex mode".into());
    }
    let color_mode = value("IPP_PRINT_COLOR_MODE", "auto");
    if !["auto", "color", "monochrome"].contains(&color_mode.as_str())
        || (color_mode == "color" && !printer.capabilities.color)
    {
        return Err("Unsupported color mode".into());
    }
    Ok(PrintOptions {
        copies,
        quality,
        sides,
        color_mode,
        media,
        orientation,
        paper_kind: paper.windows_kind,
        paper_width: (paper.width as f64 / 25.4).round() as i32,
        paper_height: (paper.height as f64 / 25.4).round() as i32,
    })
}

fn run() -> Result<(), String> {
    let queue = std::env::var("AIRPRINTER_QUEUE").map_err(|_| "AIRPRINTER_QUEUE is required")?;
    let file = std::env::args_os()
        .nth(1)
        .ok_or("Missing document filename")?;
    let format = std::env::var("CONTENT_TYPE").map_err(|_| "CONTENT_TYPE is required")?;
    if std::fs::metadata(&file).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("Document exceeds 64 MiB".into());
    }
    let document = std::fs::read(&file).map_err(|e| e.to_string())?;
    if document.len() > 64 * 1024 * 1024 || !print_job::validate_document(&format, &document) {
        return Err("Invalid or oversized document".into());
    }
    let printer = PrinterDetector::new()
        .detect_one(&queue)?
        .ok_or("Windows queue not found")?;
    let selected: HashMap<_, _> = [
        "IPP_COPIES",
        "IPP_PRINT_QUALITY",
        "IPP_ORIENTATION_REQUESTED",
        "IPP_MEDIA",
        "IPP_SIDES",
        "IPP_PRINT_COLOR_MODE",
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|value| (key.into(), value)))
    .collect();
    let options = options(&printer, &selected)?;
    eprintln!(
        "INFO: Received {} document bytes, format={format}, quality={}",
        document.len(),
        options.quality
    );
    let id = print_job::submit(
        &queue,
        &document,
        &format,
        &options,
        &AtomicBool::new(false),
    )?;
    eprintln!("INFO: Windows spool_id={id}");
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match print_job::spool_state(&queue, id)? {
            None => break,
            Some(flags) if flags & (0x80 | 0x1000) != 0 => break,
            Some(flags) if flags & (0x2 | 0x20 | 0x40 | 0x200) != 0 => {
                return Err(format!("Windows queue error: {flags:#x}"))
            }
            _ => (),
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Windows job {id} is still queued after 120 seconds"
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("INFO: Windows queue completed job {id}; confirm physical output at printer");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cups_keywords_reach_windows_options() {
        let printer = Printer {
            id: "test".into(),
            name: "test".into(),
            status: airprinter::models::PrinterStatus::Online,
            capabilities: Default::default(),
        };
        let mut env = HashMap::from([
            ("IPP_PRINT_QUALITY".into(), "high".into()),
            ("IPP_ORIENTATION_REQUESTED".into(), "landscape".into()),
        ]);
        let settings = options(&printer, &env).unwrap();
        assert_eq!(
            (settings.quality, settings.orientation, settings.paper_kind),
            (5, 4, 9)
        );
        env.insert("IPP_COPIES".into(), "999".into());
        assert!(options(&printer, &env).is_err());
    }
}
