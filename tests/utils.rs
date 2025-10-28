// Shared test utilities for waveform generation and analysis
use std::f32::consts::PI;

/// Generate a sine wave
pub fn generate_sine_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32>
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
pub fn generate_square_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32>
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
pub fn generate_sawtooth_wave(frequency: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32>
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
pub fn generate_frequency_sweep(start_freq: f32, end_freq: f32, sample_rate: u32, channels: u16, duration_seconds: f32) -> Vec<f32>
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

/// Generate white noise (random samples)
pub fn generate_white_noise(sample_rate: u32, channels: u16, duration_seconds: f32, seed: u64) -> Vec<f32>
{
    // Simple LCG pseudorandom number generator for deterministic noise
    let mut state = seed;
    let mut next_random = || -> f32
        {
            // LCG parameters from Numerical Recipes
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            // Convert to float in range [-0.3, 0.3]
            let normalized = (state as f32) / (u64::MAX as f32);
            (normalized - 0.5) * 0.6
        };

    let total_samples = (sample_rate as f32 * duration_seconds) as usize;
    let mut samples = Vec::with_capacity(total_samples * channels as usize);

    for _ in 0..total_samples
    {
        for _ in 0..channels
        {
            samples.push(next_random());
        }
    }

    samples
}

/// Calculate Signal-to-Noise Ratio between original and decoded audio
/// Skips initial and final transients to avoid edge effects
pub fn calculate_snr(original: &[f32], decoded: &[f32]) -> f32
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

/// Calculate SNR for a specific range of samples
pub fn calculate_snr_range(original: &[f32], decoded: &[f32], start_idx: usize, end_idx: usize) -> f32
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

