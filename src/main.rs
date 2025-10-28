mod codec;
#[cfg(feature = "ui")]
mod ui;
mod audio;

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::io::Write;
use std::sync::Arc;

#[cfg(feature = "ui")]
use eframe::egui;

#[cfg(feature = "playback")]
use ui::SamplesSource;

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

/// Decode a GLC file to a lossless format (FLAC or WAV)
fn decode_file(input_path: PathBuf) -> Result<(), anyhow::Error>
{
    use codec::{Decoder, load_encoded};
    use audio::export_to_wav;

    #[cfg(feature = "flac-export")]
    use audio::export_to_flac;

    println!("Loading: {:?}", input_path.file_name().unwrap());

    // Load the encoded file
    let encoded = load_encoded(&input_path)?;

    println!("Decoding: {} Hz, {} channels",
             encoded.header.sample_rate, encoded.header.channels);

    // Create decoder and decode
    let mut decoder = Decoder::new(
        encoded.header.channels as usize,
        encoded.header.sample_rate
    );
    let samples = decoder.decode(&encoded, None)?;

    println!("Decoded {} samples", samples.len());

    // Generate output path
    let mut output_path = input_path.clone();

    #[cfg(feature = "flac-export")]
    {
        output_path.set_extension("flac");
        export_to_flac(
            &output_path,
            &samples,
            encoded.header.sample_rate,
            encoded.header.channels,
        )?;
        println!("Saved: {:?} (FLAC)", output_path.file_name().unwrap());
    }

    #[cfg(not(feature = "flac-export"))]
    {
        output_path.set_extension("wav");
        export_to_wav(
            &output_path,
            &samples,
            encoded.header.sample_rate,
            encoded.header.channels,
        )?;
        println!("Saved: {:?} (WAV)", output_path.file_name().unwrap());
    }

    Ok(())
}

/// Play a GLC file using rodio (same as GUI playback)
#[cfg(feature = "playback")]
fn play_file(input_path: PathBuf) -> Result<(), anyhow::Error>
{
    use codec::{Decoder, load_encoded};
    use rodio::{OutputStream, Sink, Source};

    println!("Loading: {:?}", input_path.file_name().unwrap());

    // Load the encoded file
    let encoded = load_encoded(&input_path)?;
    let encoded = Arc::new(encoded);

    let sample_rate = encoded.header.sample_rate;
    let channels = encoded.header.channels;

    println!("Playing: {} Hz, {} channels", sample_rate, channels);
    println!("Press Ctrl+C to stop playback");

    // Create audio output stream
    let (_stream, stream_handle) = OutputStream::try_default()
        .map_err(|e| anyhow::anyhow!("Failed to get default audio output: {}", e))?;

    let sink = Sink::try_new(&stream_handle)
        .map_err(|e| anyhow::anyhow!("Failed to create audio sink: {}", e))?;

    // Create decoder and get streaming receiver
    let mut decoder = Decoder::new(channels as usize, sample_rate);
    let rx = decoder.decode_streaming(encoded, None);

    // Receive and play all chunks
    while let Ok(chunk) = rx.recv()
    {
        // Create a rodio Source from the chunk samples
        let source = SamplesSource::new(chunk.samples.clone(), sample_rate, channels);
        sink.append(source);

        if chunk.is_last
        {
            break;
        }
    }

    // Wait for playback to finish
    sink.sleep_until_end();

    println!("Playback finished");

    Ok(())
}

/// Play file stub when playback feature is not available
#[cfg(not(feature = "playback"))]
fn play_file(_input_path: PathBuf) -> Result<(), anyhow::Error>
{
    eprintln!("Error: Playback support not compiled in");
    eprintln!("Build with: cargo build --release --no-default-features --features playback");
    eprintln!("Or run glc -p --ffplay <file.glc> to use ffplay instead");
    Err(anyhow::anyhow!("Playback not available"))
}

/// Play a GLC file using ffplay (alternative method)
fn play_file_with_ffplay(input_path: PathBuf) -> Result<(), anyhow::Error>
{
    use codec::{Decoder, load_encoded};

    println!("Loading: {:?}", input_path.file_name().unwrap());

    // Load the encoded file
    let encoded = load_encoded(&input_path)?;
    let encoded = Arc::new(encoded);

    let sample_rate = encoded.header.sample_rate;
    let channels = encoded.header.channels;

    println!("Playing: {} Hz, {} channels (via ffplay)", sample_rate, channels);
    println!("Press Ctrl+C or close ffplay window to stop");

    // Spawn ffplay process with stderr captured
    let mut child = Command::new("ffplay")
        .args(&[
            "-f", "f32le",                    // 32-bit float PCM
            "-ar", &sample_rate.to_string(),  // sample rate
            "-ac", &channels.to_string(),     // channels
            "-nodisp",                         // no video display
            "-autoexit",                       // exit when done
            "-",                               // read from stdin
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child
    {
        Ok(c) => c,
        Err(e) =>
            {
                eprintln!("Error: Failed to spawn ffplay: {}", e);
                eprintln!("Make sure ffplay is installed and in your PATH");
                return Err(e.into());
            }
    };

    let mut stdin = child.stdin.take().ok_or_else(||
        anyhow::anyhow!("Failed to open stdin for ffplay"))?;

    // Create decoder and stream
    let mut decoder = Decoder::new(channels as usize, sample_rate);
    let rx = decoder.decode_streaming(encoded, None);

    // Stream audio chunks to ffplay
    let mut chunks_sent = 0;
    while let Ok(chunk) = rx.recv()
    {
        chunks_sent += 1;

        // Convert f32 samples to bytes
        let bytes: Vec<u8> = chunk.samples.iter()
                                  .flat_map(|&f| f.to_le_bytes())
                                  .collect();

        if let Err(e) = stdin.write_all(&bytes)
        {
            eprintln!("Error writing to ffplay: {}", e);
            break;
        }

        if chunk.is_last
        {
            break;
        }
    }

    // Close stdin to signal end of data
    drop(stdin);

    println!("Sent {} chunks to ffplay", chunks_sent);

    // Wait for ffplay to finish and capture output
    let output = child.wait_with_output()?;

    if !output.status.success()
    {
        eprintln!("ffplay exited with status: {}", output.status);
        if !output.stderr.is_empty()
        {
            eprintln!("ffplay stderr:");
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        }
        if !output.stdout.is_empty()
        {
            eprintln!("ffplay stdout:");
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
        }
    }
    else
    {
        println!("Playback finished");
    }

    Ok(())
}

/// Check if a path has a supported lossless audio file extension
fn is_lossless_audio_file(path: &PathBuf) -> bool
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

/// Check if a path has a .glc extension
fn is_glc_file(path: &PathBuf) -> bool
{
    if let Some(ext) = path.extension()
    {
        if let Some(ext_str) = ext.to_str()
        {
            return ext_str.to_lowercase() == "glc";
        }
    }
    false
}

/// Print usage information
fn print_usage()
{
    eprintln!("Usage:");
    eprintln!("  glc <file.wav|file.flac> ...        Encode audio files to .glc");
    eprintln!("  glc -d <file.glc> ...               Decode .glc files to lossless");
    eprintln!("  glc -p <file.glc>                   Play .glc file");
    eprintln!("  glc -p --ffplay <file.glc>          Play .glc file using ffplay");
    eprintln!("  glc                                  Launch GUI (if available)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -d, --decode    Decode .glc files to FLAC (or WAV if FLAC export disabled)");
    eprintln!("  -p, --play      Play .glc file using audio system (default)");
    eprintln!("      --ffplay    Use ffplay for playback (with -p flag)");
    eprintln!();
    eprintln!("Supported formats: WAV, FLAC (input), GLC (decode/play)");
}

fn main() -> Result<(), Box<dyn std::error::Error>>
{
    let args: Vec<String> = std::env::args().collect();

    // Check if we have command-line arguments (skip program name)
    if args.len() > 1
    {
        let first_arg = args[1].as_str();

        // Check for decode flag
        if first_arg == "-d" || first_arg == "--decode"
        {
            if args.len() < 3
            {
                eprintln!("Error: -d requires at least one .glc file");
                print_usage();
                std::process::exit(1);
            }

            let mut has_errors = false;

            for arg in &args[2..]
            {
                let path = PathBuf::from(arg);

                if !path.exists()
                {
                    eprintln!("Error: File not found: {:?}", path);
                    has_errors = true;
                    continue;
                }

                if !is_glc_file(&path)
                {
                    eprintln!("Error: Not a .glc file: {:?}", path);
                    has_errors = true;
                    continue;
                }

                match decode_file(path)
                {
                    Ok(()) => {},
                    Err(e) =>
                        {
                            eprintln!("Error decoding file: {}", e);
                            has_errors = true;
                        }
                }
            }

            if has_errors
            {
                std::process::exit(1);
            }

            return Ok(());
        }

        // Check for play flag
        if first_arg == "-p" || first_arg == "--play"
        {
            // Check if --ffplay flag is present
            let use_ffplay = args.len() >= 3 && (args[2] == "--ffplay");
            let file_arg_idx = if use_ffplay { 3 } else { 2 };

            if args.len() < file_arg_idx + 1
            {
                eprintln!("Error: -p requires exactly one .glc file");
                print_usage();
                std::process::exit(1);
            }

            let path = PathBuf::from(&args[file_arg_idx]);

            if !path.exists()
            {
                eprintln!("Error: File not found: {:?}", path);
                std::process::exit(1);
            }

            if !is_glc_file(&path)
            {
                eprintln!("Error: Not a .glc file: {:?}", path);
                std::process::exit(1);
            }

            let result = if use_ffplay
            {
                play_file_with_ffplay(path)
            }
            else
            {
                play_file(path)
            };

            match result
            {
                Ok(()) => return Ok(()),
                Err(e) =>
                    {
                        eprintln!("Error playing file: {}", e);
                        std::process::exit(1);
                    }
            }
        }

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

            if !is_lossless_audio_file(&path)
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
            print_usage();
            std::process::exit(1);
        }

        Ok(())
    }
}
