use anyhow::{Context, Result, anyhow, ensure};
use rayon::prelude::*;
use stb_image::image::{LoadResult, load_with_depth};
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use webp::Encoder;

const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "tiff", "gif"];

#[derive(Debug, Clone)]
pub enum LogEvent {
    Started { input_dir: String },
    Discovered { total: usize },
    Converted { path: PathBuf },
    Skipped { path: PathBuf, reason: SkipReason },
    Error { msg: String },
    Finished {
        success: usize,
        skipped: usize,
        errors: usize,
        total: usize,
    },
}

#[derive(Debug, Clone)]
pub enum SkipReason {
    OutputExists,
}

pub fn convert_to_webp(
    input_path: &Path,
    output_path: &Path,
    quality: f32,
    lossless: bool,
) -> Result<()> {
    ensure!(
        input_path != output_path,
        "Input and output paths must differ."
    );

    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ensure!(ext != "webp", "Input is already a WebP image.");

    let img_data = match load_with_depth(input_path, 4, false) {
        LoadResult::ImageU8(data) => data,
        LoadResult::Error(msg) => {
            return Err(anyhow!("stb_image failed for {:?}: {}", input_path, msg));
        }
        _ => return Err(anyhow!("Unsupported format: {:?}", input_path)),
    };

    let encoder = Encoder::from_rgba(
        &img_data.data,
        img_data.width as u32,
        img_data.height as u32,
    );
    let webp = if lossless {
        encoder.encode_lossless()
    } else {
        encoder.encode(quality.clamp(0.0, 100.0))
    };

    let mut writer = BufWriter::new(
        File::create(output_path)
            .with_context(|| format!("Failed to create file: {:?}", output_path))?,
    );
    writer.write_all(&webp)?;

    Ok(())
}

pub fn convert_to_webp_dir_threads<F>(
    input_dir: &str,
    output_dir: &str,
    quality: f32,
    lossless: bool,
    log_fn: F,
) -> anyhow::Result<()>
where
    F: Fn(LogEvent) + Sync + Send,
{
    ensure!(!input_dir.is_empty(), "Input path is empty.");
    ensure!(!output_dir.is_empty(), "Output path is empty.");

    log_fn(LogEvent::Started {
        input_dir: input_dir.to_string(),
    });
    fs::create_dir_all(output_dir)?;
    let output_dir = Path::new(output_dir);

    let entries: Vec<PathBuf> = fs::read_dir(input_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map_or(false, |ext| {
                        let lower = ext.to_ascii_lowercase();
                        SUPPORTED_EXTENSIONS.contains(&lower.as_str())
                    })
        })
        .collect();

    log_fn(LogEvent::Discovered { total: entries.len() });

    let success_count = AtomicUsize::new(0);
    let skipped_count = AtomicUsize::new(0);
    let error_count = AtomicUsize::new(0);

    entries.par_iter().for_each(|path| {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let output_path = output_dir.join(format!("{stem}.webp"));

        if output_path.exists() {
            skipped_count.fetch_add(1, Ordering::Relaxed);
            log_fn(LogEvent::Skipped {
                path: path.clone(),
                reason: SkipReason::OutputExists,
            });
            return;
        }

        match convert_to_webp(path, &output_path, quality, lossless) {
            Ok(_) => {
                success_count.fetch_add(1, Ordering::Relaxed);
                log_fn(LogEvent::Converted { path: path.clone() });
            }
            Err(e) => {
                error_count.fetch_add(1, Ordering::Relaxed);
                log_fn(LogEvent::Error {
                    msg: format!("{}: {}", path.display(), e),
                });
            }
        }
    });

    log_fn(LogEvent::Finished {
        success: success_count.load(Ordering::Relaxed),
        skipped: skipped_count.load(Ordering::Relaxed),
        errors: error_count.load(Ordering::Relaxed),
        total: entries.len(),
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_to_webp_rejects_same_path() {
        let p = Path::new("a.webp");
        let result = convert_to_webp(p, p, 87.0, false);
        assert!(result.is_err());
    }

    #[test]
    fn convert_to_webp_rejects_webp_input() {
        let result = convert_to_webp(
            Path::new("foo.webp"),
            Path::new("bar.webp.copy"),
            87.0,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn convert_dir_rejects_empty_paths() {
        let r = convert_to_webp_dir_threads("", "out", 87.0, false, |_| {});
        assert!(r.is_err());
        let r = convert_to_webp_dir_threads("in", "", 87.0, false, |_| {});
        assert!(r.is_err());
    }
}
