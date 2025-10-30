//! Handles file I/O for mainstream lossless audio codecs (WAV and FLAC)
use anyhow::{anyhow, Result};
use std::path::Path;
use hound;
use claxon;
use crate::flac as pure_flac;


/// Helper function to convert f32 samples to i16
/// For each f32 sample, multiply by i16 max, then clamp to valid i16 range
fn convert_f32_to_i16(samples: &[f32]) -> Vec<i16>
{
    samples.iter()
           .map(|&sample| (sample * 32767.0).clamp(-32768.0, 32767.0) as i16)
           .collect()
}

/// Load audio file from `Path` (only supports WAV and FLAC)
/// Calls [`load_wav`] or [`load_flac`] depending on filetype
/// Returns the sample vector, sample rate, and number of channels
pub fn load_audio_file_lossless(path: &Path) -> Result<(Vec<f32>, u32, u16)>
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

/// Load WAV file from `Path`
/// Returns the sample vector, sample rate, and number of channels
fn load_wav(path: &Path) -> Result<(Vec<f32>, u32, u16)> 
{
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format 
    {
        hound::SampleFormat::Float => 
        {
            // Pass through f32 samples
            reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?
        }
        hound::SampleFormat::Int => 
        {
            // Divide by max sample value to convert i32 samples to f32
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

/// Load FLAC file from `Path`
/// Returns the sample vector, sample rate, and number of channels
fn load_flac(path: &Path) -> Result<(Vec<f32>, u32, u16)> 
{
    let mut reader = claxon::FlacReader::open(path)?;
    let info = reader.streaminfo();
    let max_sample_value = (1 << (info.bits_per_sample - 1)) as f32;

    let mut samples = Vec::new();
    for sample in reader.samples() 
    {
        // Divide by max sample value to convert i32 samples to f32
        let s = sample?;
        samples.push(s as f32 / max_sample_value);
    }

    Ok((samples, info.sample_rate, info.channels as u16))
}

/// Export `samples` to `Path` using FLAC encoding (pure Rust implementation)
/// Uses 16-bit depth and a compression level of 5
pub fn export_to_flac(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<()>
{
    // Use the pure Rust FLAC encoder
    pure_flac::export_to_flac(path, samples, sample_rate, channels)
}

/// Export `samples` to `Path` using WAV encoding (basically PCM with headers)
/// Uses 16-bit depth
pub fn export_to_wav(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<()>
{
    // Add WAV headers
    let spec = hound::WavSpec
    {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)?;

    // WAV files apparently expect integer-valued samples
    // See [http://tiny.systems/software/soundProgrammer/WavFormatDocs.pdf],
    // particularly this part:
    //
    //      8-bit samples are stored as unsigned bytes, ranging from 0 to 255.
    //      16-bit samples are stored as 2's-complement signed integers,
    //      ranging from -32768 to 32767.
    let i16_samples = convert_f32_to_i16(samples);
    for sample in i16_samples
    {
        writer.write_sample(sample)?;
    }

    writer.finalize()?;
    Ok(())
}