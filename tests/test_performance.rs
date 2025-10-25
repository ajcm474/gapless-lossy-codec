use gapless_lossy_codec::codec::{Encoder, Decoder};
use std::time::Instant;

mod utils;
use utils::generate_sine_wave;

#[test]
fn test_benchmark_perceptual_weights_creation()
{
    let start = Instant::now();
    for _ in 0..1000
    {
        let _encoder = Encoder::new();
    }
    let elapsed = start.elapsed();
    println!("Creating 1000 encoders (with perceptual weights): {:.2}ms",
             elapsed.as_secs_f64() * 1000.0);
    println!("Per encoder: {:.4}ms", elapsed.as_secs_f64() * 1000.0 / 1000.0);
}

#[test]
fn benchmark_single_frame_encoding()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 0.1); // Just 0.1 seconds

    let mut encoder = Encoder::new();

    let start = Instant::now();
    let _encoded = encoder.encode(&samples, 44100, 1).unwrap();
    let elapsed = start.elapsed();

    println!("Encoding 0.1s of audio: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
}

#[test]
fn benchmark_encoding_by_duration()
{
    let durations = vec![0.5, 1.0, 2.0, 5.0, 10.0];

    for duration in durations
    {
        let samples = generate_sine_wave(440.0, 44100, 1, duration);
        let mut encoder = Encoder::new();

        let start = Instant::now();
        let _encoded = encoder.encode(&samples, 44100, 1).unwrap();
        let elapsed = start.elapsed();

        let frames_per_sec = (samples.len() as f64 / 44100.0) / elapsed.as_secs_f64();
        println!("{:.1}s audio encoded in {:.2}ms ({:.1}x realtime)",
                 duration,
                 elapsed.as_secs_f64() * 1000.0,
                 frames_per_sec);
    }
}

#[test]
fn benchmark_complex_waveform_encoding()
{
    let duration = 2.0;

    // Test different waveform types
    println!("\nBenchmarking 2.0s encoding for different waveforms:");

    // Sine wave (simple, sparse spectrum)
    let samples = generate_sine_wave(440.0, 44100, 1, duration);
    let mut encoder = Encoder::new();
    let start = Instant::now();
    let encoded_sine = encoder.encode(&samples, 44100, 1).unwrap();
    let sine_time = start.elapsed();

    // Square wave (complex, many harmonics)
    let samples = utils::generate_square_wave(440.0, 44100, 1, duration);
    let mut encoder = Encoder::new();
    let start = Instant::now();
    let encoded_square = encoder.encode(&samples, 44100, 1).unwrap();
    let square_time = start.elapsed();

    // Sawtooth wave (very complex, most harmonics)
    let samples = utils::generate_sawtooth_wave(440.0, 44100, 1, duration);
    let mut encoder = Encoder::new();
    let start = Instant::now();
    let encoded_saw = encoder.encode(&samples, 44100, 1).unwrap();
    let saw_time = start.elapsed();

    println!("  Sine wave:     {:.2}ms ({} frames, {} total coeffs)",
             sine_time.as_secs_f64() * 1000.0,
             encoded_sine.frames.len(),
             encoded_sine.frames.iter()
                         .map(|f| f.sparse_coeffs_per_channel[0].len())
                         .sum::<usize>());

    println!("  Square wave:   {:.2}ms ({} frames, {} total coeffs)",
             square_time.as_secs_f64() * 1000.0,
             encoded_square.frames.len(),
             encoded_square.frames.iter()
                           .map(|f| f.sparse_coeffs_per_channel[0].len())
                           .sum::<usize>());

    println!("  Sawtooth wave: {:.2}ms ({} frames, {} total coeffs)",
             saw_time.as_secs_f64() * 1000.0,
             encoded_saw.frames.len(),
             encoded_saw.frames.iter()
                        .map(|f| f.sparse_coeffs_per_channel[0].len())
                        .sum::<usize>());
}

#[test]
fn benchmark_stereo_vs_mono()
{
    let duration = 2.0;

    // Mono
    let samples_mono = generate_sine_wave(440.0, 44100, 1, duration);
    let mut encoder = Encoder::new();
    let start = Instant::now();
    let _encoded_mono = encoder.encode(&samples_mono, 44100, 1).unwrap();
    let mono_time = start.elapsed();

    // Stereo
    let samples_stereo = generate_sine_wave(440.0, 44100, 2, duration);
    let mut encoder = Encoder::new();
    let start = Instant::now();
    let _encoded_stereo = encoder.encode(&samples_stereo, 44100, 2).unwrap();
    let stereo_time = start.elapsed();

    println!("Mono:   {:.2}ms", mono_time.as_secs_f64() * 1000.0);
    println!("Stereo: {:.2}ms ({:.2}x mono time)",
             stereo_time.as_secs_f64() * 1000.0,
             stereo_time.as_secs_f64() / mono_time.as_secs_f64());
}

#[test]
fn benchmark_parallel_scaling()
{
    use rayon::ThreadPoolBuilder;

    let samples = generate_sine_wave(440.0, 44100, 1, 10.0); // 10 seconds

    for num_threads in [1, 2, 4, 8]
    {
        let pool = ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .unwrap();

        let time = pool.install(|| {
            let mut encoder = Encoder::new();
            let start = Instant::now();
            let _encoded = encoder.encode(&samples, 44100, 1).unwrap();
            start.elapsed()
        });

        println!("{} threads: {:.2}ms", num_threads, time.as_secs_f64() * 1000.0);
    }
}

#[test]
fn profile_encoding_stages()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 2.0);

    println!("\nProfiling encoding stages for 2.0s audio:");

    let mut encoder = Encoder::new();

    // We'll do this manually to time each stage
    // Note: This is a rough approximation since we can't easily insert timing
    // into the parallel iterator without modifying the source

    let total_start = Instant::now();
    let encoded = encoder.encode(&samples, 44100, 1).unwrap();
    let total_time = total_start.elapsed();

    println!("  Total encoding: {:.2}ms", total_time.as_secs_f64() * 1000.0);
    println!("  Frames encoded: {}", encoded.frames.len());
    println!("  Avg per frame: {:.4}ms",
             total_time.as_secs_f64() * 1000.0 / encoded.frames.len() as f64);

    // Count coefficient statistics
    let total_possible_coeffs = encoded.frames.len() * 1024; // HOP_SIZE
    let total_kept_coeffs: usize = encoded.frames.iter()
                                          .map(|f| f.sparse_coeffs_per_channel[0].len())
                                          .sum();

    println!("  Sparsity: {:.2}% coefficients kept",
             (total_kept_coeffs as f64 / total_possible_coeffs as f64) * 100.0);
}

#[test]
fn benchmark_decode_speed()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 5.0);

    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 1).unwrap();

    let mut decoder = Decoder::new(1, 44100);

    let start = Instant::now();
    let _decoded = decoder.decode(&encoded, None).unwrap();
    let elapsed = start.elapsed();

    println!("Decoding 5.0s audio: {:.2}ms ({:.1}x realtime)",
             elapsed.as_secs_f64() * 1000.0,
             5.0 / elapsed.as_secs_f64());
}

#[test]
fn benchmark_full_roundtrip()
{
    let duration: f64 = 5.0;
    let samples = generate_sine_wave(440.0, 44100, 1, duration as f32);

    let encode_start = Instant::now();
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 1).unwrap();
    let encode_time = encode_start.elapsed();

    let decode_start = Instant::now();
    let mut decoder = Decoder::new(1, 44100);
    let _decoded = decoder.decode(&encoded, None).unwrap();
    let decode_time = decode_start.elapsed();

    let total_time = encode_time + decode_time;

    println!("\nFull roundtrip for {:.1}s audio:", duration);
    println!("  Encode: {:.2}ms ({:.1}x realtime)",
             encode_time.as_secs_f64() * 1000.0,
             duration / encode_time.as_secs_f64());
    println!("  Decode: {:.2}ms ({:.1}x realtime)",
             decode_time.as_secs_f64() * 1000.0,
             duration / decode_time.as_secs_f64());
    println!("  Total:  {:.2}ms ({:.1}x realtime)",
             total_time.as_secs_f64() * 1000.0,
             duration / total_time.as_secs_f64());
}
