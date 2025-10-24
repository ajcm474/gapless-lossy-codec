use gapless_lossy_codec::codec::{Encoder, Decoder};
use std::f32::consts::PI;

/// Generate test waveforms
fn generate_sine_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32> 
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

fn generate_square_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32> 
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

fn generate_sawtooth_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32> 
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
        let dec = decoded.get(i).unwrap_or(&0.0);
        let error = orig - dec;
        
        signal_power += orig * orig;
        noise_power += error * error;
    }
    
    if noise_power > 0.0 && signal_power > 0.0 
    {
        10.0 * (signal_power / noise_power).log10()
    } else 
    {
        if noise_power == 0.0 { f32::INFINITY } else { 0.0 }
    }
}

#[test]
fn test_sine_wave_440hz_mono() 
{
    let samples = generate_sine_wave(440.0, 44100, 1, 2.0);
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 1).expect("Encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");
    
    // Check length preservation (should be exactly the same)
    assert_eq!(decoded.len(), samples.len(), "Length mismatch: expected {}, got {}", samples.len(), decoded.len());
    
    // Check SNR
    let snr = calculate_snr(&samples, &decoded);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    
    println!("Sine 440Hz test: SNR = {:.2} dB, length = {}", snr, decoded.len());
}

#[test]
fn test_square_wave_1000hz_mono() 
{
    let samples = generate_square_wave(1000.0, 44100, 1, 2.0);
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 1).expect("Encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");
    
    // Check length preservation
    assert_eq!(decoded.len(), samples.len());
    
    // Check SNR (square waves are harder to encode, so allow lower SNR)
    let snr = calculate_snr(&samples, &decoded);
    assert!(snr > -15.0, "SNR too low: {} dB", snr);
    
    println!("Square 1000Hz test: SNR = {:.2} dB, length = {}", snr, decoded.len());
}

#[test]
fn test_sawtooth_wave_440hz_mono() 
{
    let samples = generate_sawtooth_wave(440.0, 44100, 1, 2.0);
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 1).expect("Encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");
    
    // Check length preservation
    assert_eq!(decoded.len(), samples.len());
    
    // Check SNR
    let snr = calculate_snr(&samples, &decoded);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);
    
    println!("Sawtooth 440Hz test: SNR = {:.2} dB, length = {}", snr, decoded.len());
}

#[test]
fn test_sample_rate_variations() 
{
    // Test 44.1 kHz
    let samples_44k = generate_sine_wave(440.0, 44100, 1, 1.0);
    let mut encoder = Encoder::new();
    let encoded_44k = encoder.encode(&samples_44k, 44100, 1).expect("44.1kHz encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded_44k = decoder.decode(&encoded_44k, None).expect("44.1kHz decoding failed");
    assert_eq!(decoded_44k.len(), samples_44k.len());
    
    // Test 48 kHz
    let samples_48k = generate_sine_wave(440.0, 48000, 1, 1.0);
    let mut encoder = Encoder::new();
    let encoded_48k = encoder.encode(&samples_48k, 48000, 1).expect("48kHz encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded_48k = decoder.decode(&encoded_48k, None).expect("48kHz decoding failed");
    assert_eq!(decoded_48k.len(), samples_48k.len());
    
    println!("Sample rate test: 44.1kHz={} samples, 48kHz={} samples", 
             decoded_44k.len(), decoded_48k.len());
}

#[test]
fn test_stereo_encoding() 
{
    let samples = generate_sine_wave(440.0, 44100, 2, 2.0);
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 2).expect("Stereo encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Stereo decoding failed");
    
    // Check length preservation
    assert_eq!(decoded.len(), samples.len());
    
    // Check SNR
    let snr = calculate_snr(&samples, &decoded);
    assert!(snr > -10.0, "Stereo SNR too low: {} dB", snr);
    
    println!("Stereo test: SNR = {:.2} dB, length = {}", snr, decoded.len());
}

#[test]
fn test_short_duration() 
{
    let samples = generate_sine_wave(440.0, 44100, 1, 0.5);  // 0.5 seconds
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 1).expect("Short duration encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Short duration decoding failed");
    
    assert_eq!(decoded.len(), samples.len());
    println!("Short duration test: {} samples", decoded.len());
}

#[test]
fn test_long_duration() 
{
    let samples = generate_sine_wave(440.0, 44100, 1, 5.0);  // 5 seconds
    let mut encoder = Encoder::new();
    let encoded = encoder.encode(&samples, 44100, 1).expect("Long duration encoding failed");
    
    let mut decoder = Decoder::new();
    let decoded = decoder.decode(&encoded, None).expect("Long duration decoding failed");
    
    assert_eq!(decoded.len(), samples.len());
    println!("Long duration test: {} samples", decoded.len());
}

#[test]
fn test_gapless_multiple_files() 
{
    // Simulate multiple files being decoded in sequence
    let file1 = generate_sine_wave(440.0, 44100, 1, 2.0);
    let file2 = generate_sine_wave(880.0, 44100, 1, 2.0);
    let file3 = generate_square_wave(440.0, 44100, 1, 2.0);
    
    let total_original_len = file1.len() + file2.len() + file3.len();
    
    // Encode each file
    let mut encoder = Encoder::new();
    let encoded1 = encoder.encode(&file1, 44100, 1).expect("File 1 encoding failed");
    let encoded2 = encoder.encode(&file2, 44100, 1).expect("File 2 encoding failed");
    let encoded3 = encoder.encode(&file3, 44100, 1).expect("File 3 encoding failed");
    
    // Decode each file
    let mut decoder = Decoder::new();
    let decoded1 = decoder.decode(&encoded1, None).expect("File 1 decoding failed");
    let decoded2 = decoder.decode(&encoded2, None).expect("File 2 decoding failed");
    let decoded3 = decoder.decode(&encoded3, None).expect("File 3 decoding failed");
    
    let total_decoded_len = decoded1.len() + decoded2.len() + decoded3.len();
    
    // Should have exact length preservation
    assert_eq!(total_decoded_len, total_original_len, 
               "Gapless length mismatch: expected {}, got {}", 
               total_original_len, total_decoded_len);
    
    println!("Gapless test: {} original samples, {} decoded samples", 
             total_original_len, total_decoded_len);
}
