use gapless_lossy_codec::codec::{Encoder, Decoder};

fn generate_sine_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32>
{
    use std::f32::consts::PI;
    let total_samples = (sample_rate as f32 * duration_seconds) as usize;
    let mut samples = Vec::with_capacity(total_samples * channels as usize);

    for i in 0..total_samples
    {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * PI * frequency * t).sin() * 0.5;

        for _ in 0..channels
        {
            samples.push(sample);
        }
    }

    samples
}

fn main()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 2.0);
    println!("Original samples: {}", samples.len());

    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 1).unwrap();

    let original_size = samples.len() * 4; // f32 = 4 bytes
    let encoded_size = bincode::serialize(&encoded).unwrap().len();

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

    println!("Original size: {} bytes", original_size);
    println!("Encoded size: {} bytes", encoded_size);
    println!("Compression ratio: {:.2}x", original_size as f32 / encoded_size as f32);

    let mut decoder = Decoder::new(1, 44100);
    let decoded = decoder.decode(&encoded, None).unwrap();

    // Check amplitude
    let window_size = 100;
    let mut max_variation = 0.0f32;
    for i in window_size..(decoded.len() - window_size)
    {
        let local_max = decoded[i.saturating_sub(window_size)..i+window_size]
            .iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        let expected = 0.5;
        let variation = (local_max - expected).abs() / expected;
        max_variation = max_variation.max(variation);
    }

    println!("Max amplitude variation: {:.4} ({:.2}%)", max_variation, max_variation * 100.0);
}