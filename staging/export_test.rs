// Export test for creating WAV files
use std::f32::consts::PI;

mod codec;

use codec::{Encoder, Decoder};

fn create_sine_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32> 
{
    let total_samples = (sample_rate as f32 * duration_seconds) as usize;
    let mut samples = Vec::with_capacity(total_samples * channels as usize);
    
    for i in 0..total_samples 
    {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * PI * frequency * t).sin();
        
        for _ in 0..channels 
        {
            samples.push(sample * 0.5);  // Keep amplitude reasonable
        }
    }
    
    samples
}

fn write_wav(filename: &str, samples: &[f32], sample_rate: u32, channels: u16) -> Result<(), Box<dyn std::error::Error>>
{
    use std::fs::File;
    use std::io::{Write, BufWriter};
    
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
    println!("=== Codec Test with WAV Export ===");
    
    let sample_rate = 44100u32;
    let channels = 1u16;
    
    // Test cases
    let test_cases = vec![
        ("440Hz sine", 440.0, 3.0),
        ("1000Hz sine", 1000.0, 3.0),
        ("100Hz sine", 100.0, 3.0),
    ];
    
    for (name, frequency, duration) in test_cases 
    {
        println!("\nTesting {}", name);
        
        // Generate original
        let original = create_sine_wave(frequency, sample_rate, channels, duration);
        let original_filename = format!("/tmp/inputs/{}_original.wav", name.replace(" ", "_"));
        write_wav(&original_filename, &original, sample_rate, channels)?;
        println!("Wrote original: {}", original_filename);
        
        // Encode
        let mut encoder = Encoder::new();
        let encoded = encoder.encode(&original, sample_rate, channels)?;
        println!("Encoded: {} frames", encoded.frames.len());
        
        // Decode
        let mut decoder = Decoder::new();
        let decoded = decoder.decode(&encoded, None)?;
        let decoded_filename = format!("/tmp/inputs/{}_decoded.wav", name.replace(" ", "_"));
        write_wav(&decoded_filename, &decoded, sample_rate, channels)?;
        println!("Wrote decoded: {}", decoded_filename);
        
        // Calculate SNR
        let min_len = original.len().min(decoded.len());
        if min_len > 1000 
        {
            let start_idx = 1000;  // Skip initial transient
            let end_idx = min_len.min(original.len() - 1000);
            
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
                let snr = 10.0 * (signal_power / noise_power).log10();
                println!("SNR: {:.2} dB", snr);
            }
        }
        
        println!("Length: original={}, decoded={}", original.len(), decoded.len());
    }
    
    println!("\nTest files exported to /tmp/inputs/");
    Ok(())
}
