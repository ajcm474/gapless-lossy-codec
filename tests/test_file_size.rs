use gapless_lossy_codec::codec::{Encoder, save_encoded};
use std::path::PathBuf;

mod utils;
use utils::
{
    generate_sine_wave,
    generate_square_wave,
    generate_sawtooth_wave,
    generate_frequency_sweep,
    generate_white_noise
};

/// Helper function to test compression for a specific waveform
fn test_waveform_compression(samples: Vec<f32>, waveform_name: &str) -> f64
{
    println!("\n{}", waveform_name);
    println!("Original samples: {} ({} bytes as f32)", samples.len(), samples.len() * 4);

    let mut encoder = Encoder::new(44100);
    let encoded = encoder.encode(&samples, 2).unwrap();

    let output_path = PathBuf::from(format!("/tmp/test_{}.glc", waveform_name.replace(" ", "_")));
    save_encoded(&encoded, &output_path).unwrap();

    let file_size = std::fs::metadata(&output_path).unwrap().len();
    let original_size = samples.len() * 4; // f32 = 4 bytes
    let ratio = original_size as f64 / file_size as f64;

    println!("Encoded file size: {} bytes", file_size);
    println!("Compression ratio: {:.2}x", ratio);
    println!("Space savings: {:.1}%", (1.0 - 1.0/ratio) * 100.0);

    // Cleanup
    std::fs::remove_file(output_path).ok();

    ratio
}

#[test]
fn test_compression_sine_wave()
{
    let samples = generate_sine_wave(440.0, 44100, 2, 10.0); // 10 seconds stereo
    let ratio = test_waveform_compression(samples, "Sine Wave (440 Hz)");

    // We should achieve at least 2x compression for reasonable quality lossy codec
    assert!(ratio >= 2.0, "Compression ratio too low: {:.2}x", ratio);
    println!("✓ Compression achieved {:.2}x ratio", ratio);
}

#[test]
fn test_compression_square_wave()
{
    let samples = generate_square_wave(440.0, 44100, 2, 10.0); // 10 seconds stereo
    let ratio = test_waveform_compression(samples, "Square Wave (440 Hz)");

    // Square waves have more harmonics, so compression might be different
    assert!(ratio >= 2.0, "Compression ratio too low: {:.2}x", ratio);
    println!("✓ Compression achieved {:.2}x ratio", ratio);
}

#[test]
fn test_compression_sawtooth_wave()
{
    let samples = generate_sawtooth_wave(440.0, 44100, 2, 10.0); // 10 seconds stereo
    let ratio = test_waveform_compression(samples, "Sawtooth Wave (440 Hz)");

    // Sawtooth waves have even more harmonics than square waves
    assert!(ratio >= 2.0, "Compression ratio too low: {:.2}x", ratio);
    println!("✓ Compression achieved {:.2}x ratio", ratio);
}

#[test]
fn test_compression_frequency_sweep()
{
    let samples = generate_frequency_sweep(100.0, 10000.0, 44100, 2, 10.0); // 10 seconds stereo
    let ratio = test_waveform_compression(samples, "Frequency Sweep (100-10000 Hz)");

    // Frequency sweeps have varying frequency content
    assert!(ratio >= 2.0, "Compression ratio too low: {:.2}x", ratio);
    println!("✓ Compression achieved {:.2}x ratio", ratio);
}

#[test]
fn test_compression_multiple_frequencies()
{
    // Generate a chord with multiple frequencies
    let duration = 10.0;
    let sample_rate = 44100;
    let channels = 2;
    let frequencies = [261.63, 329.63, 392.00]; // C major chord (C, E, G)

    let samples1 = generate_sine_wave(frequencies[0], sample_rate, channels, duration);
    let samples2 = generate_sine_wave(frequencies[1], sample_rate, channels, duration);
    let samples3 = generate_sine_wave(frequencies[2], sample_rate, channels, duration);

    // Mix the three frequencies
    let mut mixed_samples = Vec::with_capacity(samples1.len());
    for i in 0..samples1.len()
    {
        mixed_samples.push((samples1[i] + samples2[i] + samples3[i]) / 3.0);
    }

    let ratio = test_waveform_compression(mixed_samples, "C Major Chord");

    // Multiple frequencies should still compress well
    assert!(ratio >= 2.0, "Compression ratio too low: {:.2}x", ratio);
    println!("✓ Compression achieved {:.2}x ratio", ratio);
}

#[test]
fn test_compression_white_noise()
{
    let samples = generate_white_noise(44100, 2, 10.0, 12345); // 10 seconds stereo
    let ratio = test_waveform_compression(samples, "White Noise");

    // White noise has energy at all frequencies, so compression will be poor
    // In fact, it may not compress at all (ratio < 1.0) due to sparse coefficient overhead
    // We just verify the test runs and report the result
    println!("✓ White noise compression: {:.2}x ratio (expected to be poor)", ratio);

    // White noise is the worst case - it may actually expand the file
    // This is expected behavior for noise-like signals
    if ratio < 1.0
    {
        println!("  Note: File expanded (encoded larger than original) - this is expected for pure noise");
    }
}