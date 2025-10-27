mod codec;
#[cfg(feature = "ui")]
mod ui;
mod audio;

use std::path::PathBuf;

#[cfg(feature = "ui")]
use eframe::egui;

/// Encode a single audio file (WAV or FLAC) to GLC format
fn encode_file(input_path: PathBuf) -> Result<(), anyhow::Error>
{
    use codec::{Encoder, save_encoded};
    use audio::load_audio_file_lossless;

    println!("Loading: {:?}", input_path.file_name().unwrap());

    // Load the input file
    let (samples, sample_rate, channels) = load_audio_file_lossless(&input_path)?;

    println!("Encoding: {} Hz, {} channels, {} samples", sample_rate, channels, samples.len());

    // Create encoder and encode
    let mut encoder = Encoder::new(sample_rate);
    let encoded = encoder.encode(&samples, channels)?;

    // Generate output path
    let mut output_path = input_path.clone();
    output_path.set_extension("glc");

    // Save encoded file
    save_encoded(&encoded, &output_path)?;

    let input_size = std::fs::metadata(&input_path)?.len();
    let output_size = std::fs::metadata(&output_path)?.len();
    let ratio = (output_size as f64 / input_size as f64) * 100.0;

    println!("Saved: {:?} ({} bytes, {:.1}% of original)",
             output_path.file_name().unwrap(), output_size, ratio);

    Ok(())
}

/// Check if a path has a supported audio file extension
fn is_audio_file(path: &PathBuf) -> bool
{
    if let Some(ext) = path.extension()
    {
        if let Some(ext_str) = ext.to_str()
        {
            let ext_lower = ext_str.to_lowercase();
            return ext_lower == "wav" || ext_lower == "flac";
        }
    }
    false
}

fn main() -> Result<(), Box<dyn std::error::Error>>
{
    let args: Vec<String> = std::env::args().collect();

    // Check if we have command-line arguments (skip program name)
    if args.len() > 1
    {
        // CLI mode: encode files
        let mut has_errors = false;

        for arg in &args[1..]
        {
            let path = PathBuf::from(arg);

            if !path.exists()
            {
                eprintln!("Error: File not found: {:?}", path);
                has_errors = true;
                continue;
            }

            if !is_audio_file(&path)
            {
                eprintln!("Error: Unsupported file type: {:?}", path);
                eprintln!("Supported formats: WAV, FLAC");
                has_errors = true;
                continue;
            }

            match encode_file(path)
            {
                Ok(()) => {},
                Err(e) =>
                    {
                        eprintln!("Error encoding file: {}", e);
                        has_errors = true;
                    }
            }
        }

        if has_errors
        {
            std::process::exit(1);
        }

        Ok(())
    }
    else
    {
        // GUI mode
        #[cfg(feature = "ui")]
        {
            let options = eframe::NativeOptions
            {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([900.0, 700.0])
                    .with_title("Gapless Lossy Codec"),
                ..Default::default()
            };

            eframe::run_native(
                "Gapless Lossy Codec",
                options,
                Box::new(|_cc| Box::new(ui::CodecApp::new())),
            )?;
        }

        #[cfg(not(feature = "ui"))]
        {
            eprintln!("Usage: glc <audio_file.wav|audio_file.flac> [more_files...]");
            eprintln!("No GUI available (built without UI feature)");
            std::process::exit(1);
        }

        Ok(())
    }
}
