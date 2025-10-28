// Micro-benchmarks for individual compression functions
// Note: These need access to private functions, so you may need to add
// a test-only public wrapper or move these into src/codec.rs as a #[cfg(test)] mod

use std::time::Instant;
use std::f32::consts::PI;

// If the functions are private, you'll need to expose them for testing
// Add this to src/codec.rs:
// #[cfg(test)]
// pub use self::{compute_masking_thresholds, compress_coefficients, PerceptualWeights};

#[test]
fn benchmark_mdct_computation()
{
    use gapless_lossy_codec::codec::Encoder;

    // Generate a single frame's worth of data
    let frame_size = 2048;
    let samples: Vec<f32> = (0..frame_size)
        .map(|i| (2.0 * PI * 440.0 * i as f32 / 44100.0).sin() * 0.5)
        .collect();

    let mut encoder = Encoder::new(44100);

    // Encode just enough to process a few frames
    let start = Instant::now();
    let iterations = 1000;
    for _ in 0..iterations
    {
        // This will process multiple frames but gives us an idea
        let _encoded = encoder.encode(&samples, 1).unwrap();
    }
    let elapsed = start.elapsed();

    println!("MDCT computation (approx, {} iterations): {:.4}ms per iteration",
             iterations,
             elapsed.as_secs_f64() * 1000.0 / iterations as f64);
}

#[test]
fn benchmark_frame_processing_sequential()
{
    use gapless_lossy_codec::codec::Encoder;

    // Generate different complexities of audio
    let test_cases = vec![
        ("sine", generate_test_sine(2.0)),
        ("square", generate_test_square(2.0)),
        ("white_noise", generate_white_noise(2.0)),
    ];

    for (name, samples) in test_cases
    {
        let mut encoder = Encoder::new(44100);

        let start = Instant::now();
        let encoded = encoder.encode(&samples, 1).unwrap();
        let elapsed = start.elapsed();

        let num_frames = encoded.frames.len();
        let avg_coeffs: f64 = encoded.frames.iter()
                                     .map(|f| f.sparse_coeffs_per_channel[0].len())
                                     .sum::<usize>() as f64 / num_frames as f64;

        println!("{:12} - {} frames in {:.2}ms ({:.4}ms/frame, avg {:.1} coeffs/frame)",
                 name,
                 num_frames,
                 elapsed.as_secs_f64() * 1000.0,
                 elapsed.as_secs_f64() * 1000.0 / num_frames as f64,
                 avg_coeffs);
    }
}

#[test]
fn benchmark_compression_overhead()
{
    use gapless_lossy_codec::codec::Encoder;

    println!("\nComparing frame processing time vs coefficient count:");

    // Generate increasingly complex signals
    let frequencies = vec![
        (1, "1 freq (pure sine)"),
        (10, "10 frequencies"),
        (50, "50 frequencies"),
        (100, "100 frequencies"),
    ];

    for (num_freqs, desc) in frequencies
    {
        let samples = generate_multi_sine(2.0, num_freqs);
        let mut encoder = Encoder::new(44100);

        let start = Instant::now();
        let encoded = encoder.encode(&samples, 1).unwrap();
        let elapsed = start.elapsed();

        let num_frames = encoded.frames.len();
        let avg_coeffs: f64 = encoded.frames.iter()
                                     .map(|f| f.sparse_coeffs_per_channel[0].len())
                                     .sum::<usize>() as f64 / num_frames as f64;

        let sparsity = (avg_coeffs / 1024.0) * 100.0;

        println!("  {:25} - {:.2}ms total, {:.4}ms/frame, {:.1} coeffs ({:.1}% sparse)",
                 desc,
                 elapsed.as_secs_f64() * 1000.0,
                 elapsed.as_secs_f64() * 1000.0 / num_frames as f64,
                 avg_coeffs,
                 sparsity);
    }
}

#[test]
fn benchmark_memory_allocation()
{
    use gapless_lossy_codec::codec::Encoder;

    // Test if memory allocation is a bottleneck
    let samples = generate_test_sine(5.0);

    println!("\nTesting multiple encoding passes (checking allocation overhead):");

    for pass in 1..=5
    {
        let mut encoder = Encoder::new(44100);

        let start = Instant::now();
        let _encoded = encoder.encode(&samples, 1).unwrap();
        let elapsed = start.elapsed();

        println!("  Pass {}: {:.2}ms", pass, elapsed.as_secs_f64() * 1000.0);
    }
}

// Helper functions to generate test signals

fn generate_test_sine(duration: f32) -> Vec<f32>
{
    let sample_rate = 44100;
    let num_samples = (sample_rate as f32 * duration) as usize;

    (0..num_samples)
        .map(|i| (2.0 * PI * 440.0 * i as f32 / sample_rate as f32).sin() * 0.5)
        .collect()
}

fn generate_test_square(duration: f32) -> Vec<f32>
{
    let sample_rate = 44100;
    let num_samples = (sample_rate as f32 * duration) as usize;

    (0..num_samples)
        .map(|i|
            {
                let phase = 2.0 * PI * 440.0 * i as f32 / sample_rate as f32;
                if phase.sin() >= 0.0 { 0.3 } else { -0.3 }
            })
        .collect()
}

fn generate_white_noise(duration: f32) -> Vec<f32>
{
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};

    let sample_rate = 44100;
    let num_samples = (sample_rate as f32 * duration) as usize;
    let hasher = RandomState::new();

    (0..num_samples)
        .map(|i|
            {
                let mut h = hasher.build_hasher();
                i.hash(&mut h);
                let val = h.finish();
                ((val % 1000) as f32 / 1000.0 - 0.5) * 0.3
            })
        .collect()
}

fn generate_multi_sine(duration: f32, num_frequencies: usize) -> Vec<f32>
{
    let sample_rate = 44100;
    let num_samples = (sample_rate as f32 * duration) as usize;

    let mut samples = vec![0.0f32; num_samples];

    // Add multiple sine waves at different frequencies
    for freq_idx in 0..num_frequencies
    {
        let frequency = 100.0 + (freq_idx as f32 * 50.0);
        let amplitude = 0.3 / (num_frequencies as f32).sqrt();

        for i in 0..num_samples
        {
            samples[i] += (2.0 * PI * frequency * i as f32 / sample_rate as f32).sin() * amplitude;
        }
    }

    samples
}

#[test]
fn analyze_coefficient_distribution()
{
    use gapless_lossy_codec::codec::Encoder;

    println!("\nAnalyzing coefficient distribution for different signals:");

    let test_signals = vec![
        ("Sine 440Hz", generate_test_sine(2.0)),
        ("Square 440Hz", generate_test_square(2.0)),
        ("10 freqs", generate_multi_sine(2.0, 10)),
        ("White noise", generate_white_noise(2.0)),
    ];

    for (name, samples) in test_signals
    {
        let mut encoder = Encoder::new(44100);
        let encoded = encoder.encode(&samples, 1).unwrap();

        let mut coeff_counts: Vec<usize> = encoded.frames.iter()
                                                  .map(|f| f.sparse_coeffs_per_channel[0].len())
                                                  .collect();

        coeff_counts.sort();

        let min = *coeff_counts.first().unwrap_or(&0);
        let max = *coeff_counts.last().unwrap_or(&0);
        let avg: f64 = coeff_counts.iter().sum::<usize>() as f64 / coeff_counts.len() as f64;

        let median = if coeff_counts.len() > 0
        {
            coeff_counts[coeff_counts.len() / 2]
        }
        else
        {
            // If the data was raw PCM, coeffs is empty
            0usize
        };

        println!("  {:15} - min: {:4}, max: {:4}, avg: {:6.1}, median: {:4}",
                 name, min, max, avg, median);
    }
}
