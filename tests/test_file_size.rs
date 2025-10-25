use gapless_lossy_codec::codec::{Encoder, save_encoded};
use std::path::PathBuf;

mod utils;
use utils::generate_sine_wave;

#[test]
fn test_compression_file_size()
{
    let samples = generate_sine_wave(440.0, 44100, 2, 10.0); // 10 seconds stereo
    println!("Original samples: {} ({} bytes as f32)", samples.len(), samples.len() * 4);

    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 2).unwrap();

    let output_path = PathBuf::from("/tmp/inputs/test_encoded.glc");
    save_encoded(&encoded, &output_path).unwrap();

    let file_size = std::fs::metadata(&output_path).unwrap().len();
    let original_size = samples.len() * 4; // f32 = 4 bytes
    let ratio = original_size as f64 / file_size as f64;

    println!("Encoded file size: {} bytes", file_size);
    println!("Compression ratio: {:.2}x", ratio);
    println!("Space savings: {:.1}%", (1.0 - 1.0/ratio) * 100.0);

    // Cleanup
    std::fs::remove_file(output_path).ok();

    // We should achieve at least 2x compression for reasonable quality lossy codec
    assert!(ratio >= 2.0, "Compression ratio too low: {:.2}x", ratio);
    println!("✓ Compression achieved {:.2}x ratio", ratio);
}