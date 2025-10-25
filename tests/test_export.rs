// Test audio export functionality (FLAC when available, WAV as fallback)
use gapless_lossy_codec::codec::{Encoder, Decoder};
use gapless_lossy_codec::audio::load_audio_file;
use std::path::PathBuf;

#[cfg(feature = "flac-export")]
use gapless_lossy_codec::audio::export_to_flac;

#[cfg(not(feature = "flac-export"))]
use gapless_lossy_codec::audio::export_to_wav;

mod utils;
use utils::generate_sine_wave;

#[test]
fn test_export_basic()
{
    let samples = generate_sine_wave(440.0, 44100, 2, 2.0);
    let sample_rate = 44100;
    let channels = 2;

    // Encode
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, sample_rate, channels).expect("Encoding failed");

    // Decode
    let mut decoder = Decoder::new(channels as usize, sample_rate);
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");

    // Export based on feature
    #[cfg(feature = "flac-export")]
    let output_path = PathBuf::from("/tmp/inputs/test_export.flac");

    #[cfg(not(feature = "flac-export"))]
    let output_path = PathBuf::from("/tmp/inputs/test_export.wav");

    #[cfg(feature = "flac-export")]
    {
        export_to_flac(&output_path, &decoded, sample_rate, channels).expect("FLAC export failed");
        println!("Created FLAC file");
    }

    #[cfg(not(feature = "flac-export"))]
    {
        export_to_wav(&output_path, &decoded, sample_rate, channels).expect("WAV export failed");
        println!("Created WAV file");
    }

    assert!(output_path.exists(), "Output file was not created");

    // Load it back
    let (loaded_samples, loaded_rate, loaded_channels) = load_audio_file(&output_path)
        .expect("Failed to load exported file");

    assert_eq!(loaded_rate, sample_rate, "Sample rate mismatch");
    assert_eq!(loaded_channels, channels, "Channels mismatch");
    assert_eq!(loaded_samples.len(), decoded.len(), "Sample count mismatch");

    // Clean up
    std::fs::remove_file(output_path).ok();

    println!("Export test passed: {} samples, {}Hz, {} channels",
             decoded.len(), sample_rate, channels);
}

#[test]
fn test_export_mono()
{
    let samples = generate_sine_wave(1000.0, 48000, 1, 1.5);
    let sample_rate = 48000;
    let channels = 1;

    // Encode
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, sample_rate, channels).expect("Encoding failed");

    // Decode
    let mut decoder = Decoder::new(channels as usize, sample_rate);
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");

    // Export based on feature
    #[cfg(feature = "flac-export")]
    let output_path = PathBuf::from("/tmp/inputs/test_export_mono.flac");

    #[cfg(not(feature = "flac-export"))]
    let output_path = PathBuf::from("/tmp/inputs/test_export_mono.wav");

    #[cfg(feature = "flac-export")]
    export_to_flac(&output_path, &decoded, sample_rate, channels).expect("FLAC export failed");

    #[cfg(not(feature = "flac-export"))]
    export_to_wav(&output_path, &decoded, sample_rate, channels).expect("WAV export failed");

    assert!(output_path.exists(), "Output file was not created");

    let (loaded_samples, loaded_rate, loaded_channels) = load_audio_file(&output_path)
        .expect("Failed to load exported file");

    assert_eq!(loaded_rate, sample_rate);
    assert_eq!(loaded_channels, channels);
    assert_eq!(loaded_samples.len(), decoded.len());

    // Clean up
    std::fs::remove_file(output_path).ok();

    println!("Mono export test passed");
}

#[test]
fn test_export_gapless_playlist()
{
    // Simulate exporting a gapless playlist
    let file1 = generate_sine_wave(440.0, 44100, 2, 1.0);
    let file2 = generate_sine_wave(880.0, 44100, 2, 1.0);
    let file3 = generate_sine_wave(1320.0, 44100, 2, 1.0);

    let sample_rate = 44100;
    let channels = 2;

    // Encode each file
    let mut encoder = Encoder::new();
    let encoded1 = encoder.encode(&file1, sample_rate, channels).expect("File 1 encoding failed");
    let encoded2 = encoder.encode(&file2, sample_rate, channels).expect("File 2 encoding failed");
    let encoded3 = encoder.encode(&file3, sample_rate, channels).expect("File 3 encoding failed");

    // Decode each file and concatenate
    let mut decoder = Decoder::new(channels as usize, sample_rate);
    let decoded1 = decoder.decode(&encoded1, None).expect("File 1 decoding failed");
    let decoded2 = decoder.decode(&encoded2, None).expect("File 2 decoding failed");
    let decoded3 = decoder.decode(&encoded3, None).expect("File 3 decoding failed");

    let mut all_samples = Vec::new();
    all_samples.extend_from_slice(&decoded1);
    all_samples.extend_from_slice(&decoded2);
    all_samples.extend_from_slice(&decoded3);

    // Export concatenated samples based on feature
    #[cfg(feature = "flac-export")]
    let output_path = PathBuf::from("/tmp/inputs/test_gapless_playlist.flac");

    #[cfg(not(feature = "flac-export"))]
    let output_path = PathBuf::from("/tmp/inputs/test_gapless_playlist.wav");

    #[cfg(feature = "flac-export")]
    export_to_flac(&output_path, &all_samples, sample_rate, channels)
        .expect("Gapless playlist FLAC export failed");

    #[cfg(not(feature = "flac-export"))]
    export_to_wav(&output_path, &all_samples, sample_rate, channels)
        .expect("Gapless playlist WAV export failed");

    assert!(output_path.exists());

    let (loaded_samples, loaded_rate, loaded_channels) = load_audio_file(&output_path)
        .expect("Failed to load exported file");

    assert_eq!(loaded_rate, sample_rate);
    assert_eq!(loaded_channels, channels);
    assert_eq!(loaded_samples.len(), all_samples.len(),
               "Gapless playlist sample count mismatch");

    // Clean up
    std::fs::remove_file(output_path).ok();

    println!("Gapless playlist export test passed: {} total samples", all_samples.len());
}