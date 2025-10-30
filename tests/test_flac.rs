use gapless_lossy_codec::audio::{export_to_flac, export_to_wav, load_audio_file_lossless};
use std::path::Path;

fn test_signal(name: &str, samples: Vec<f32>, sample_rate: u32, channels: u16) -> Result<(), Box<dyn std::error::Error>>
{
    println!("\n=== Testing {} ===", name);
    println!("Samples: {}, Rate: {} Hz, Channels: {}", samples.len(), sample_rate, channels);

    let flac_path_str = format!("target/{}.flac", name);
    let wav_path_str = format!("target/{}.wav", name);
    let flac_path = Path::new(&flac_path_str);
    let wav_path = Path::new(&wav_path_str);

    // Export to FLAC
    export_to_flac(flac_path, &samples, sample_rate, channels)?;
    let flac_size = std::fs::metadata(flac_path)?.len();

    // Export to WAV for comparison
    export_to_wav(wav_path, &samples, sample_rate, channels)?;
    let wav_size = std::fs::metadata(wav_path)?.len();

    // Load back and verify
    let (loaded_samples, loaded_rate, loaded_channels) = load_audio_file_lossless(flac_path)?;

    println!("FLAC size: {} bytes, WAV size: {} bytes", flac_size, wav_size);
    println!("Compression ratio: {:.2}%", (flac_size as f64 / wav_size as f64) * 100.0);

    // Verify metadata
    assert_eq!(loaded_rate, sample_rate, "Sample rate mismatch");
    assert_eq!(loaded_channels, channels, "Channel count mismatch");
    assert_eq!(loaded_samples.len(), samples.len(), "Sample count mismatch");

    // Calculate RMS error
    let mut sum_sq_error = 0.0;
    for (orig, loaded) in samples.iter().zip(loaded_samples.iter())
    {
        let error = orig - loaded;
        sum_sq_error += error * error;
    }
    let rms_error = (sum_sq_error / samples.len() as f32).sqrt();
    println!("RMS error: {:.6}", rms_error);

    // Quantization error for 16-bit should be at most 1/32768
    assert!(rms_error < 0.0001, "RMS error too high: {}", rms_error);

    // Clean up
    std::fs::remove_file(flac_path).ok();
    std::fs::remove_file(wav_path).ok();

    Ok(())
}

#[test]
fn test_flac_silence()
{
    let silence = vec![0.0; 1000];
    test_signal("silence", silence, 44100, 1).unwrap();
}

#[test]
fn test_flac_dc_offset()
{
    let dc = vec![0.5; 1000];
    test_signal("dc", dc, 44100, 1).unwrap();
}

#[test]
fn test_flac_sine_wave()
{
    let mut sine = Vec::new();
    for i in 0..4410
    {
        let t = i as f32 / 44100.0;
        sine.push((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.8);
    }
    test_signal("sine", sine, 44100, 1).unwrap();
}

#[test]
fn test_flac_white_noise()
{
    let mut noise = Vec::new();
    let mut seed = 12345u32;
    for _ in 0..8820
    {
        // Simple LCG for reproducible pseudo-random
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let val = ((seed >> 16) & 0x7fff) as f32 / 32768.0;
        noise.push(val * 2.0 - 1.0);
    }
    test_signal("noise", noise, 44100, 1).unwrap();
}

#[test]
fn test_flac_stereo()
{
    let mut stereo = Vec::new();
    for i in 0..4410
    {
        let t = i as f32 / 44100.0;
        // Left channel: 440Hz
        stereo.push((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5);
        // Right channel: 880Hz
        stereo.push((2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.5);
    }
    test_signal("stereo", stereo, 44100, 2).unwrap();
}

#[test]
fn test_flac_sample_rates()
{
    let samples_48k = vec![0.0; 4800];
    test_signal("48khz", samples_48k, 48000, 1).unwrap();

    let samples_96k = vec![0.0; 9600];
    test_signal("96khz", samples_96k, 96000, 1).unwrap();
}

#[test]
fn test_flac_minimum_size()
{
    // Test minimum FLAC block size (16 samples)
    let mut small = Vec::new();
    for i in 0..16
    {
        small.push((i as f32 / 16.0) * 2.0 - 1.0);
    }
    test_signal("small", small, 8000, 1).unwrap();
}

#[test]
fn test_flac_compression_levels()
{
    // Test different compression levels
    use gapless_lossy_codec::flac::export_to_flac_with_level;

    let mut samples = Vec::new();
    for i in 0..1000
    {
        let t = i as f32 / 44100.0;
        samples.push((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5);
    }

    for level in 0..=8
    {
        let path_str = format!("target/test_level_{}.flac", level);
        let path = Path::new(&path_str);

        export_to_flac_with_level(path, &samples, 44100, 1, level).unwrap();

        let size = std::fs::metadata(path).unwrap().len();
        println!("Compression level {}: {} bytes", level, size);

        // Verify it can be loaded
        let (loaded, _, _) = load_audio_file_lossless(path).unwrap();
        assert_eq!(loaded.len(), samples.len());

        std::fs::remove_file(path).ok();
    }
}