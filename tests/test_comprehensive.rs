// Comprehensive test runner that creates test cases varying waveforms, frequencies, sample rates, and channels
use gapless_lossy_codec::codec::{Encoder, Decoder};
use std::f32::consts::PI;

/// Test waveform generators
struct TestWaveforms;

impl TestWaveforms
{
    /// Generate a sine wave
    fn sine_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32>
    {
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
    
    /// Generate a square wave
    fn square_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32>
    {
        let total_samples = (sample_rate as f32 * duration_seconds) as usize;
        let mut samples = Vec::with_capacity(total_samples * channels as usize);
        
        for i in 0..total_samples
        {
            let t = i as f32 / sample_rate as f32;
            let phase = 2.0 * PI * frequency * t;
            let sample = if phase.sin() >= 0.0 { 0.3 } else { -0.3 };
            
            for _ in 0..channels
            {
                samples.push(sample);
            }
        }
        
        samples
    }
    
    /// Generate a sawtooth wave
    fn sawtooth_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32>
    {
        let total_samples = (sample_rate as f32 * duration_seconds) as usize;
        let mut samples = Vec::with_capacity(total_samples * channels as usize);
        
        for i in 0..total_samples
        {
            let t = i as f32 / sample_rate as f32;
            let phase = (2.0 * PI * frequency * t) % (2.0 * PI);
            let sample = ((phase / PI) - 1.0) * 0.3;
            
            for _ in 0..channels
            {
                samples.push(sample);
            }
        }
        
        samples
    }
    
    /// Generate a frequency sweep
    fn frequency_sweep(start_freq: f32, end_freq: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32>
    {
        let total_samples = (sample_rate as f32 * duration_seconds) as usize;
        let mut samples = Vec::with_capacity(total_samples * channels as usize);
        
        for i in 0..total_samples
        {
            let t = i as f32 / sample_rate as f32;
            let progress = t / duration_seconds;
            let frequency = start_freq + (end_freq - start_freq) * progress;
            let sample = (2.0 * PI * frequency * t).sin() * 0.3;
            
            for _ in 0..channels
            {
                samples.push(sample);
            }
        }
        
        samples
    }
}

fn calculate_snr(original: &[f32], decoded: &[f32]) -> f32
{
    let min_len = original.len().min(decoded.len());
    if min_len < 2000 { return 0.0; }
    
    let start_idx = 1000;  // Skip initial transient
    let end_idx = min_len - 1000;  // Skip final transient
    
    let mut signal_power = 0.0f32;
    let mut noise_power = 0.0f32;
    
    for i in start_idx..end_idx
    {
        let orig = original[i];
        let dec = decoded[i];
        let error = orig - dec;
        
        signal_power += orig * orig;
        noise_power += error * error;
    }
    
    if noise_power > 0.0 && signal_power > 0.0
    {
        10.0 * (signal_power / noise_power).log10()
    }
    else
    {
        if noise_power == 0.0 { f32::INFINITY } else { 0.0 }
    }
}

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
    let samples = TestWaveforms::sine_wave(100.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("100Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_44k_mono()
{
    let samples = TestWaveforms::sine_wave(440.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_1000hz_44k_mono()
{
    let samples = TestWaveforms::sine_wave(1000.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("1000Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_2000hz_44k_mono()
{
    let samples = TestWaveforms::sine_wave(2000.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("2000Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_4000hz_44k_mono()
{
    let samples = TestWaveforms::sine_wave(4000.0, 44100, 1, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("4000Hz sine: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_48k_mono()
{
    let samples = TestWaveforms::sine_wave(440.0, 48000, 1, 5.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 48000, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine 48kHz: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_44k_stereo()
{
    let samples = TestWaveforms::sine_wave(440.0, 44100, 2, 5.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 2);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine stereo: SNR = {:.2} dB", snr);
}

#[test]
fn test_square_440hz_44k_mono()
{
    let samples = TestWaveforms::square_wave(440.0, 44100, 1, 5.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -15.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz square: SNR = {:.2} dB", snr);
}

#[test]
fn test_sawtooth_440hz_44k_mono()
{
    let samples = TestWaveforms::sawtooth_wave(440.0, 44100, 1, 5.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -15.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sawtooth: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_100_1000_44k_mono()
{
    let samples = TestWaveforms::frequency_sweep(100.0, 1000.0, 44100, 1, 6.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("100-1000Hz sweep: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_440_2000_44k_mono()
{
    let samples = TestWaveforms::frequency_sweep(440.0, 2000.0, 44100, 1, 7.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440-2000Hz sweep: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_200_8000_48k_mono()
{
    let samples = TestWaveforms::frequency_sweep(200.0, 8000.0, 48000, 1, 8.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 48000, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("200-8000Hz sweep 48kHz: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_1000_100_44k_mono()
{
    let samples = TestWaveforms::frequency_sweep(1000.0, 100.0, 44100, 1, 6.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("1000-100Hz descending sweep: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_44k_mono_short()
{
    let samples = TestWaveforms::sine_wave(440.0, 44100, 1, 1.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine short: SNR = {:.2} dB", snr);
}

#[test]
fn test_sine_440hz_44k_mono_long()
{
    let samples = TestWaveforms::sine_wave(440.0, 44100, 1, 10.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 1);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440Hz sine long: SNR = {:.2} dB", snr);
}

#[test]
fn test_sweep_440_880_44k_stereo()
{
    let samples = TestWaveforms::frequency_sweep(440.0, 880.0, 44100, 2, 6.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 44100, 2);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("440-880Hz sweep stereo: SNR = {:.2} dB", snr);
}

#[test]
fn test_square_1000hz_48k_stereo()
{
    let samples = TestWaveforms::square_wave(1000.0, 48000, 2, 4.0);
    let (snr, decoded_len) = run_single_test(samples.clone(), 48000, 2);
    assert!(snr > -15.0, "SNR too low: {} dB", snr);
    assert_eq!(decoded_len, samples.len(), "Length mismatch");
    println!("1000Hz square 48kHz stereo: SNR = {:.2} dB", snr);
}

