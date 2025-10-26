use gapless_lossy_codec::codec::{Encoder, Decoder};

mod utils;
use utils::generate_sine_wave;

#[test]
fn test_compression_effectiveness()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 2.0);
    println!("Original samples: {}", samples.len());

    let mut encoder = Encoder::new(44100);
    let encoded = encoder.encode(&samples, 1).unwrap();

    println!("Frames: {}", encoded.frames.len());

    let mut total_coeffs = 0;
    let mut total_possible = 0;
    for frame in &encoded.frames
    {
        for ch_coeffs in &frame.sparse_coeffs_per_channel
        {
            total_coeffs += ch_coeffs.len();
            total_possible += 1024; // HOP_SIZE
        }
    }

    println!("Total non-zero coefficients: {} out of {} ({:.2}%)",
             total_coeffs, total_possible,
             (total_coeffs as f32 / total_possible as f32) * 100.0);

    let sparsity = total_coeffs as f32 / total_possible as f32;
    assert!(sparsity < 0.5, "Compression is not effective enough: {:.2}% coefficients retained", sparsity * 100.0);

    println!("✓ Compression is effective: only {:.2}% of coefficients retained", sparsity * 100.0);
}