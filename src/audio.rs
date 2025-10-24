use anyhow::{anyhow, Result};
use std::path::Path;
use hound;
use claxon;

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

