use anyhow::{Context, Result, anyhow, ensure};
// use image::ImageReader;
use rayon::prelude::*;
use stb_image::image::{LoadResult, load_with_depth};
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use webp::Encoder;

pub fn convert_to_webp(
    input_path: &str,
    output_path: &str,
    quality: f32,
    lossless: bool,
) -> Result<()> {
    let input_path = Path::new(input_path);
    let output_path = Path::new(output_path);

    ensure!(
        input_path != output_path,
        "Input and output paths must differ."
    );

    if output_path.exists() {
        return Ok(());
    }

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
    F: Fn(String) + Sync + Send,
{
    if input_dir.is_empty() {
        return Err(anyhow!("Input path is empty."));
    }

    if output_dir.is_empty() {
        return Err(anyhow!("Output path is empty."));
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
    let success_count = AtomicUsize::new(0);
    let error_count = AtomicUsize::new(0);

    entries.par_iter().for_each(|path| {
        let filename_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let output_path = format!("{}/{}.webp", output_dir, filename_stem);

        match convert_to_webp(path.to_str().unwrap(), &output_path, quality, lossless) {
            Ok(_) => {
                success_count.fetch_add(1, Ordering::Relaxed);
                log_fn(format!("Converted: {}", path.display()));
            }
            Err(e) => {
                error_count.fetch_add(1, Ordering::Relaxed);
                log_fn(format!("Error: {}", e));
            }
        }
    });

    let total_success = success_count.load(Ordering::Relaxed);
    let total_errors = error_count.load(Ordering::Relaxed);

    log_fn(format!(
        "\nFinished processing all files\nSuccess: {}\nErrors: {}\nTotal: {}",
        total_success,
        total_errors,
        entries.len()
    ));

    Ok(())
}

// test
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_to_webp() {
        let input_path = r"test\input\bad.jpg";
        let output_path = r"test\output\bad.webp";
        let quality = 87.0;
        let lossless = false;

        let result = convert_to_webp(input_path, output_path, quality, lossless);
        assert!(result.is_ok());
    }

    #[test]
    fn test_convert_to_webp2() {
        let input_path = r"test\input\good.png";
        let output_path = r"test\output\good.webp";
        let quality = 87.0;
        let lossless = false;

        let result = convert_to_webp(input_path, output_path, quality, lossless);
        assert!(result.is_ok());
    }
}
