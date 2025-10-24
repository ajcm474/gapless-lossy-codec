// Simple tests for basic codec functionality
use gapless_lossy_codec::codec::{Encoder, Decoder};
use std::f32::consts::PI;

fn generate_sine_wave(frequency: f32, sample_rate: u32, duration: f32) -> Vec<f32>
{
    let total_samples = (sample_rate as f32 * duration) as usize;
    let mut samples = Vec::with_capacity(total_samples);
    
    for i in 0..total_samples
    {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * PI * frequency * t).sin() * 0.5;
        samples.push(sample);
    }
    
    samples
}

fn calculate_snr(original: &[f32], decoded: &[f32], start_idx: usize, end_idx: usize) -> f32
{
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

#[test]
fn test_basic_encode_decode()
{
    let sample_rate = 44100u32;
    let frequency = 440.0f32;
    let duration = 2.0f32;
    let channels = 1u16;
    
    let samples = generate_sine_wave(frequency, sample_rate, duration);
    
    println!("Generated {} samples of {}Hz sine wave at {}Hz sample rate", 
             samples.len(), frequency, sample_rate);
    
    // Encode
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, sample_rate, channels).expect("Encoding failed");
    
    println!("Encoded successfully: {} frames", encoded.frames.len());
    
    // Decode
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");
    
    println!("Decoded successfully: {} samples", decoded.len());
    
    // Calculate basic SNR
    let min_len = samples.len().min(decoded.len());
    assert!(min_len > 1000, "Not enough samples for SNR calculation");
    
    let start_idx = 1000;
    let end_idx = min_len.min(samples.len() - 1000);
    
    let snr = calculate_snr(&samples, &decoded, start_idx, end_idx);
    println!("SNR: {:.2} dB", snr);
    
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
}

#[test]
fn test_length_preservation()
{
    let sample_rate = 44100u32;
    let frequency = 440.0f32;
    let duration = 2.0f32;
    let channels = 1u16;
    
    let samples = generate_sine_wave(frequency, sample_rate, duration);
    
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, sample_rate, channels).expect("Encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");
    
    // Check for length issues (speed problem indicator)
    let length_ratio = decoded.len() as f32 / samples.len() as f32;
    println!("Length ratio (decoded/original): {:.6}", length_ratio);
    
    assert!((length_ratio - 1.0).abs() < 0.01, 
            "Significant length difference detected! Ratio: {}", length_ratio);
}

#[test]
fn test_speed_ratio()
{
    let sample_rate = 44100u32;
    let frequency = 440.0f32;
    let duration = 2.0f32;
    let channels = 1u16;
    
    let samples = generate_sine_wave(frequency, sample_rate, duration);
    
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, sample_rate, channels).expect("Encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");
    
    // Calculate expected vs actual duration
    let expected_duration = samples.len() as f32 / sample_rate as f32;
    let actual_duration = decoded.len() as f32 / sample_rate as f32;
    let speed_ratio = actual_duration / expected_duration;
    
    println!("Speed ratio: {:.6} (1.0 = correct speed)", speed_ratio);
    println!("Expected duration: {:.3}s, Actual duration: {:.3}s", 
             expected_duration, actual_duration);
    
    assert!((speed_ratio - 1.0).abs() < 0.01,
            "Speed issue detected! Speed ratio: {}", speed_ratio);
}

#[test]
fn test_multiple_frequencies()
{
    let sample_rate = 44100u32;
    let channels = 1u16;
    let frequencies = vec![100.0, 440.0, 1000.0, 2000.0];
    
    for frequency in frequencies
    {
        let samples = generate_sine_wave(frequency, sample_rate, 1.0);
        
        let mut encoder = Encoder::new();
        let encoded = encoder.encode(&samples, sample_rate, channels)
            .expect(&format!("Encoding failed for {}Hz", frequency));
        
        let mut decoder = Decoder::new();
        let decoded = decoder.decode(&encoded, None)
            .expect(&format!("Decoding failed for {}Hz", frequency));
        
        assert_eq!(decoded.len(), samples.len(), 
                   "Length mismatch for {}Hz", frequency);
        
        println!("{}Hz: OK ({} samples)", frequency, decoded.len());
    }
}

#[test]
fn test_various_durations()
{
    let sample_rate = 44100u32;
    let frequency = 440.0f32;
    let channels = 1u16;
    let durations = vec![0.5, 1.0, 2.0, 5.0];
    
    for duration in durations
    {
        let samples = generate_sine_wave(frequency, sample_rate, duration);
        
        let mut encoder = Encoder::new();
        let encoded = encoder.encode(&samples, sample_rate, channels)
            .expect(&format!("Encoding failed for {:.1}s", duration));
        
        let mut decoder = Decoder::new();
        let decoded = decoder.decode(&encoded, None)
            .expect(&format!("Decoding failed for {:.1}s", duration));
        
        assert_eq!(decoded.len(), samples.len(), 
                   "Length mismatch for {:.1}s", duration);
        
        println!("{:.1}s: OK ({} samples)", duration, decoded.len());
    }
}

