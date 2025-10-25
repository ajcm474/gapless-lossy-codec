use anyhow::{anyhow, Result};
use std::path::Path;
use hound;
use claxon;

#[cfg(feature = "flac-export")]
use flac_bound::{FlacEncoder, WriteWrapper};

// Helper function to convert f32 samples to i16
fn convert_f32_to_i16(samples: &[f32]) -> Vec<i16>
{
    samples.iter()
           .map(|&sample| (sample * 32767.0).clamp(-32768.0, 32767.0) as i16)
           .collect()
}

pub fn load_audio_file(path: &Path) -> Result<(Vec<f32>, u32, u16)> 
{
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| anyhow!("No file extension"))?
        .to_lowercase();

    match ext.as_str() 
    {
        "wav" => load_wav(path),
        "flac" => load_flac(path),
        _ => Err(anyhow!("Unsupported file format: {}", ext)),
    }
}

fn load_wav(path: &Path) -> Result<(Vec<f32>, u32, u16)> 
{
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format 
    {
        hound::SampleFormat::Float => 
        {
            reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?
        }
        hound::SampleFormat::Int => 
        {
            let bits = spec.bits_per_sample;
            let max = (1 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| Ok::<f32, hound::Error>(s? as f32 / max))
                .collect::<Result<Vec<_>, _>>()?
        }
    };

    Ok((samples, spec.sample_rate, spec.channels))
}

fn load_flac(path: &Path) -> Result<(Vec<f32>, u32, u16)> 
{
    let mut reader = claxon::FlacReader::open(path)?;
    let info = reader.streaminfo();
    let max_sample_value = (1 << (info.bits_per_sample - 1)) as f32;

    let mut samples = Vec::new();
    for sample in reader.samples() 
    {
        let s = sample?;
        samples.push(s as f32 / max_sample_value);
    }

    Ok((samples, info.sample_rate, info.channels as u16))
}

#[cfg(feature = "flac-export")]
pub fn export_to_flac(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<()>
{
    let mut file = std::fs::File::create(path)?;
    let mut write_wrapper = WriteWrapper(&mut file);

    let mut encoder = FlacEncoder::new()
        .ok_or_else(|| anyhow!("Failed to create FLAC encoder"))?
        .channels(channels as u32)
        .bits_per_sample(16)
        .sample_rate(sample_rate)
        .compression_level(5)
        .init_write(&mut write_wrapper)
        .map_err(|e| anyhow!("Failed to initialize FLAC encoder: {:?}", e))?;

    // Convert f32 samples (interleaved) to i32 samples
    // FLAC encoder expects samples in range appropriate for bits_per_sample
    let num_frames = samples.len() / channels as usize;

    // Deinterleave and convert to i32
    let mut channel_buffers: Vec<Vec<i32>> = vec![Vec::with_capacity(num_frames); channels as usize];
    for (i, &sample) in samples.iter().enumerate()
    {
        let channel = i % channels as usize;
        let s = (sample * 32767.0).clamp(-32768.0, 32767.0) as i32;
        channel_buffers[channel].push(s);
    }

    // Convert to slice references for process()
    let channel_refs: Vec<&[i32]> = channel_buffers.iter().map(|v| v.as_slice()).collect();

    // Process all frames at once
    encoder.process(&channel_refs)
           .map_err(|_| anyhow!("Failed to process FLAC frames"))?;

    encoder.finish()
           .map_err(|e| anyhow!("Failed to finish FLAC encoding: {:?}", e))?;

    Ok(())
}

pub fn export_to_wav(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<()>
{
    let spec = hound::WavSpec
    {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)?;

    let i16_samples = convert_f32_to_i16(samples);
    for sample in i16_samples
    {
        writer.write_sample(sample)?;
    }

    writer.finalize()?;
    Ok(())
}