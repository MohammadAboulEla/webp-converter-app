# WebP Converter App

A very fast, lightweight, and multithreaded GUI application built in Rust for bulk image conversion to WebP format. Leveraging Rust’s threading capabilities, it efficiently processes multiple images in parallel for high-speed performance.

## Features

- Convert images from common formats (JPEG, PNG, etc.) to WebP  
- Adjustable quality settings (0–100)  
- Optional lossless conversion mode  
- Multi-threaded processing for high performance  
- Clean, user-friendly interface  
- Real-time conversion logs

## Usage

1. Select the input directory containing the images  
2. Choose the output directory for the converted files  
3. Adjust the quality setting (higher = better quality and larger size)  
4. Enable lossless mode if needed (overrides quality setting)  
5. Click **Convert** to begin  
6. Monitor progress via the built-in log view

## Building from Source
```bash
cargo build --release
```

## Dependencies

| Crate         | Purpose                |
|---------------|------------------------|
| `eframe/egui` | GUI framework          |
| `rfd`         | Native file dialogs    |
| `image`       | Image decoding         |
| `webp`        | WebP encoding          |
| `rayon`       | Parallel processing    |
| `anyhow`      | Error handling         |






















