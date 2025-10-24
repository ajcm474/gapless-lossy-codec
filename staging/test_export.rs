// Simple test to export before/after WAV files
use gapless_codec_minimal::codec::{Encoder, Decoder};
use std::f32::consts::PI;
use std::fs::File;
use std::io::{Write, BufWriter};

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

fn generate_frequency_sweep(start_freq: f32, end_freq: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32> 
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

fn write_wav(filename: &str, samples: &[f32], sample_rate: u32, channels: u16) -> Result<(), Box<dyn std::error::Error>>
{
    let mut file = BufWriter::new(File::create(filename)?);
    
    // Simple WAV header
    let data_size = (samples.len() * 2) as u32;  // 16-bit samples
    let file_size = 36 + data_size;
    
    // RIFF header
    file.write_all(b"RIFF")?;
    file.write_all(&file_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;
    
    // Format chunk
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;  // chunk size
    file.write_all(&1u16.to_le_bytes())?;   // PCM format
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * channels as u32 * 2).to_le_bytes())?; // byte rate
    file.write_all(&(channels * 2).to_le_bytes())?; // block align
    file.write_all(&16u16.to_le_bytes())?;  // bits per sample
    
    // Data chunk
    file.write_all(b"data")?;
    file.write_all(&data_size.to_le_bytes())?;
    
    for sample in samples 
    {
        let amplitude = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
        file.write_all(&amplitude.to_le_bytes())?;
    }
    
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>>
{
    println!("=== Codec Test Export - Fixed Version ===");
    
    let sample_rate = 44100u32;
    let channels = 1u16;
    
    // Test cases requested by user (matching the playlist they tested)
    let test_cases = vec![
        ("sawtooth_440hz_44k_mono", generate_sawtooth_wave(440.0, sample_rate, channels, 3.5)),
        ("sine_440hz_44k_mono", generate_sine_wave(440.0, sample_rate, channels, 3.5)),
        ("square_440hz_44k_mono", generate_square_wave(440.0, sample_rate, channels, 3.5)),
        ("sweep_100_2000_44k_mono", generate_frequency_sweep(100.0, 2000.0, sample_rate, channels, 4.0)),
    ];
    
    let mut total_original_duration = 0.0;
    let mut total_decoded_duration = 0.0;
    
    for (name, samples) in test_cases 
    {
        println!("\nTesting: {}", name);
        
        // Write original
        let original_filename = format!("/tmp/inputs/{}_original.wav", name);
        write_wav(&original_filename, &samples, sample_rate, channels)?;
        println!("  Exported original: {}", original_filename);
        
        // Encode
        let mut encoder = Encoder::new();
        let encoded = encoder.encode(&samples, sample_rate, channels)?;
        println!("  Encoded: {} frames", encoded.frames.len());
        
        // Decode
        let mut decoder = Decoder::new();
        let decoded = decoder.decode(&encoded, None)?;
        
        // Write decoded
        let decoded_filename = format!("/tmp/inputs/{}_decoded.wav", name);
        write_wav(&decoded_filename, &decoded, sample_rate, channels)?;
        println!("  Exported decoded: {}", decoded_filename);
        
        // Check durations
        let original_duration = samples.len() as f32 / (sample_rate * channels as u32) as f32;
        let decoded_duration = decoded.len() as f32 / (sample_rate * channels as u32) as f32;
        
        total_original_duration += original_duration;
        total_decoded_duration += decoded_duration;
        
        println!("  Duration: original={:.3}s, decoded={:.3}s, ratio={:.6}", 
                 original_duration, decoded_duration, decoded_duration / original_duration);
        
        // Calculate SNR
        if samples.len() == decoded.len() 
        {
            let min_len = samples.len().min(decoded.len());
            if min_len > 2000 
            {
                let start_idx = 1000;
                let end_idx = min_len - 1000;
                
                let mut signal_power = 0.0f32;
                let mut noise_power = 0.0f32;
                
                for i in start_idx..end_idx 
                {
                    let orig = samples[i];
                    let dec = decoded[i];
                    let error = orig - dec;
                    
                    signal_power += orig * orig;
                    noise_power += error * error;
                }
                
                if noise_power > 0.0 && signal_power > 0.0 
                {
                    let snr = 10.0 * (signal_power / noise_power).log10();
                    println!("  SNR: {:.2} dB", snr);
                }
            }
        } else 
        {
            println!("  WARNING: Length mismatch! original={}, decoded={}", samples.len(), decoded.len());
        }
    }
    
    println!("\n=== Summary ===");
    println!("Total duration: original={:.3}s, decoded={:.3}s", total_original_duration, total_decoded_duration);
    println!("Overall ratio: {:.6}", total_decoded_duration / total_original_duration);
    
    // Check for the specific issues mentioned by user
    let ratio = total_decoded_duration / total_original_duration;
    if (ratio - 1.0).abs() < 0.001 
    {
        println!("✅ SPEED ISSUE: FIXED (ratio ≈ 1.0)");
    } else if ratio > 1.05 
    {
        println!("❌ SPEED ISSUE: Still present - audio is {:.1}% slower", (ratio - 1.0) * 100.0);
    } else 
    {
        println!("⚠️  Minor timing difference: {:.3}%", (ratio - 1.0) * 100.0);
    }
    
    println!("\nExpected total should be exactly 14.000 seconds for the test playlist");
    println!("Test files exported to /tmp/inputs/");
    
    Ok(())
}
