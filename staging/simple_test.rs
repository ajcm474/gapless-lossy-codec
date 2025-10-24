// Simple test for codec without GUI dependencies
use std::f32::consts::PI;

mod codec;

use codec::{Encoder, Decoder};

fn main() 
{
    println!("=== Simple Codec Test ===");
    
    // Generate a simple sine wave
    let sample_rate = 44100u32;
    let frequency = 440.0f32;
    let duration = 2.0f32;
    let channels = 1u16;
    
    let total_samples = (sample_rate as f32 * duration) as usize;
    let mut samples = Vec::with_capacity(total_samples);
    
    for i in 0..total_samples 
    {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * PI * frequency * t).sin() * 0.5;
        samples.push(sample);
    }
    
    println!("Generated {} samples of {}Hz sine wave at {}Hz sample rate", 
             samples.len(), frequency, sample_rate);
    
    // Encode
    println!("Encoding...");
    let mut encoder = Encoder::new();
    match encoder.encode(&samples, sample_rate, channels) 
    {
        Ok(encoded) => 
        {
            println!("Encoded successfully: {} frames", encoded.frames.len());
            
            // Decode
            println!("Decoding...");
            let mut decoder = Decoder::new();
            match decoder.decode(&encoded, None) 
            {
                Ok(decoded) => 
                {
                    println!("Decoded successfully: {} samples", decoded.len());
                    
                    // Calculate basic SNR
                    let min_len = samples.len().min(decoded.len());
                    if min_len > 1000  // Skip initial samples for SNR calculation
                    {
                        let start_idx = 1000;
                        let end_idx = min_len.min(samples.len() - 1000);
                        
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
                            println!("SNR: {:.2} dB", snr);
                        }
                        
                        // Check for length issues (speed problem indicator)
                        let length_ratio = decoded.len() as f32 / samples.len() as f32;
                        println!("Length ratio (decoded/original): {:.6}", length_ratio);
                        if (length_ratio - 1.0).abs() > 0.01 
                        {
                            println!("WARNING: Significant length difference detected!");
                        }
                        
                        // Calculate expected vs actual duration
                        let expected_duration = samples.len() as f32 / sample_rate as f32;
                        let actual_duration = decoded.len() as f32 / sample_rate as f32;
                        let speed_ratio = actual_duration / expected_duration;
                        println!("Speed ratio: {:.6} (1.0 = correct speed)", speed_ratio);
                        
                        if (speed_ratio - 1.08841).abs() < 0.001 
                        {
                            println!("CONFIRMED: 8.84% speed issue detected!");
                        }
                    }
                }
                Err(e) => 
                {
                    println!("Decode failed: {}", e);
                }
            }
        }
        Err(e) => 
        {
            println!("Encode failed: {}", e);
        }
    }
}
