use anyhow::{Context, Result};
use image::io::Reader as ImageReader;
use rayon::prelude::*;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use webp::Encoder;

pub fn convert_to_webp(
    input_path: &str,
    output_path: &str,
    quality: f32,
    lossless: bool,
) -> Result<()> {

    let input_path = Path::new(input_path);
    let output_path = Path::new(output_path);

    if output_path.exists() {
        // println!("Warning: Output file already exists and will be overwritten.");
        let msg = format!("Output file already exists: {:?}", output_path);
        return Err(anyhow::anyhow!(msg));
        // return Ok(());
    }

    if input_path == output_path {
        return Err(anyhow::anyhow!("Input and output paths must differ."));
    }

    // Early return if input is already WebP
    if input_path
        .extension()
        .and_then(|e| e.to_str())
        .map_or(false, |ext| ext.eq_ignore_ascii_case("webp"))
    {
        println!("Input is already a WebP image. Skipping conversion.");
        return Ok(());
    }

    let q = quality.clamp(0.0, 100.0);

    let img = ImageReader::open(input_path)
        .with_context(|| format!("Failed to open image: {:?}", input_path))?
        .decode()
        .with_context(|| format!("Failed to decode image: {:?}", input_path))?
        .to_rgba8();

    let (w, h) = img.dimensions();
    let encoder = Encoder::from_rgba(&img, w, h);

    let webp = encoder
        .encode_simple(lossless, q)
        .map_err(|e| anyhow::anyhow!("WebP encoding failed: {:?}", e))?;

    let file = File::create(output_path)
        .with_context(|| format!("Failed to create file: {:?}", output_path))?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(&webp)
        .with_context(|| "Failed to write WebP data")?;

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
    F: Fn(String) + Sync + Send,
{
    if input_dir.is_empty() {
        return Err(anyhow::anyhow!("Input path is empty."));
    }

    if output_dir.is_empty() {
        return Err(anyhow::anyhow!("Output path is empty."));
    }

    log_fn(format!("Starting conversion from: {}", input_dir));
    fs::create_dir_all(output_dir)?;

    let entries: Vec<_> = fs::read_dir(input_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map_or(false, |ext| {
                        matches!(
                            ext.to_ascii_lowercase().as_str(),
                            "png" | "jpg" | "jpeg" | "bmp" | "tiff" | "gif"
                        )
                    })
        })
        .collect();

    log_fn(format!("Found {} files to convert\n", entries.len()));

    // Use Atomic counters for thread-safe counting
    let success_count = std::sync::atomic::AtomicUsize::new(0);
    let error_count = std::sync::atomic::AtomicUsize::new(0);

    entries.par_iter().for_each(|path| {
        let filename_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let output_path = format!("{}/{}.webp", output_dir, filename_stem);

        match convert_to_webp(path.to_str().unwrap(), &output_path, quality, lossless) {
            Ok(_) => {
                success_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                log_fn(format!("Converted: {}", path.display()));
            }
            Err(e) => {
                error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // log_fn(format!("Error converting {}: {}", path.display(), e));
                log_fn(format!("Error: {}", e));
            }
        }
    });

    let total_success = success_count.load(std::sync::atomic::Ordering::Relaxed);
    let total_errors = error_count.load(std::sync::atomic::Ordering::Relaxed);

    log_fn(format!(
        "\nFinished processing all files\nSuccess: {}\nErrors: {}\nTotal: {}",
        total_success,
        total_errors,
        entries.len()
    ));

    Ok(())
}
