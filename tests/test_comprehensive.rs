// Comprehensive test runner that creates test cases varying waveforms, frequencies, sample rates, and channels
use gapless_lossy_codec::codec::{Encoder, Decoder};

mod utils;
use utils::{generate_sine_wave, generate_square_wave, generate_sawtooth_wave, generate_frequency_sweep, calculate_snr};

fn run_single_test(samples: Vec<f32>, sample_rate: u32, channels: u16) -> (f32, usize)
{
    // Encode
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, sample_rate, channels).expect("Encoding failed");
    
    // Decode
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");
    
    // Calculate quality metrics
    let snr = calculate_snr(&samples, &decoded);
    
    (snr, decoded.len())
}

#[test]
fn test_sine_100hz_44k_mono()
{
    let samples = generate_sine_wave(100.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("100Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_44k_mono()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_1000hz_44k_mono()
{
    let samples = generate_sine_wave(1000.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("1000Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_2000hz_44k_mono()
{
    let samples = generate_sine_wave(2000.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("2000Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_4000hz_44k_mono()
{
    let samples = generate_sine_wave(4000.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("4000Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_48k_mono()
{
    let samples = generate_sine_wave(440.0, 48000, 1, 5.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 48000, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine 48kHz: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_44k_stereo()
{
    let samples = generate_sine_wave(440.0, 44100, 2, 5.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 2);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine stereo: SNR = {:.2} dB", snr);
}

#[test]
fn test_square_440hz_44k_mono()
{
    let samples = generate_square_wave(440.0, 44100, 1, 5.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -15.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz square: SNR = {:.2} dB", snr);
}

#[test]
fn test_sawtooth_440hz_44k_mono()
{
    let samples = generate_sawtooth_wave(440.0, 44100, 1, 5.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -15.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sawtooth: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_100_1000_44k_mono()
{
    let samples = generate_frequency_sweep(100.0, 1000.0, 44100, 1, 6.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("100-1000Hz sweep: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_440_2000_44k_mono()
{
    let samples = generate_frequency_sweep(440.0, 2000.0, 44100, 1, 7.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440-2000Hz sweep: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_200_8000_48k_mono()
{
    let samples = generate_frequency_sweep(200.0, 8000.0, 48000, 1, 8.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 48000, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("200-8000Hz sweep 48kHz: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_1000_100_44k_mono()
{
    let samples = generate_frequency_sweep(1000.0, 100.0, 44100, 1, 6.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("1000-100Hz descending sweep: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_44k_mono_short()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 1.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine short: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_44k_mono_long()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 10.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine long: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_440_880_44k_stereo()
{
    let samples = generate_frequency_sweep(440.0, 880.0, 44100, 2, 6.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 2);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440-880Hz sweep stereo: SNR = {:.2} dB", snr);
}

#[test]
fn test_square_1000hz_48k_stereo()
{
    let samples = generate_square_wave(1000.0, 48000, 2, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 48000, 2);
    assert!(snr > -15.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("1000Hz square 48kHz stereo: SNR = {:.2} dB", snr);
}

#[test]
fn test_amplitude_modulation_detection()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 2.0);
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 1).unwrap();
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).unwrap();

    // Check for amplitude modulation by measuring envelope variation
    let window_size = 100;
    let mut max_variation = 0.0f32;
    for i in window_size..(decoded.len() - window_size)
    {
        let local_max = decoded[i.saturating_sub(window_size)..i+window_size]
            .iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        let expected = 0.5; // Our sine wave amplitude
        let variation = (local_max - expected).abs() / expected;
        max_variation = max_variation.max(variation);
    }

    assert!(max_variation < 0.1, "Excessive amplitude modulation detected: {}", max_variation);
}
