//! Fast MDCT codec (multi-channel corrected, amplitude & timing fixed).
//! - Precomputed cosine table
//! - Parallel encode and batch-parallel decode (rayon)
//! - Proper multi-channel storage: per-frame, per-channel coeffs & scales
//! - Matching normalization on MDCT and IMDCT
use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::f32::consts::PI;
use crossbeam_channel::{Sender, Receiver, bounded};
use std::time::Instant;
use std::sync::Arc;
use rayon::prelude::*;

const FRAME_SIZE: usize = 2048;  // 2N (samples per MDCT block)
const HOP_SIZE: usize = 1024;    // N (hop, 50% overlap)
const QUANTIZATION_BITS: u32 = 12;
const FRAMES_PER_CHUNK: usize = 500;
const DECODE_BATCH: usize = 32;  // how many frames to decode in parallel per batch

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncodedAudio 
{
    pub header: AudioHeader,
    pub frames: Vec<EncodedFrame>, // time-ordered frames
    pub gapless_info: GaplessInfo,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AudioHeader 
{
    pub sample_rate: u32,
    pub channels: u16,
    pub total_samples: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GaplessInfo 
{
    pub encoder_delay: u32,
    pub padding: u32,
    pub original_length: u64,
}

/// Per-timeframe, per-channel data
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncodedFrame 
{
    /// Outer vec: channel index -> inner vec: N i16 coefficients
    pub mdct_coeffs_per_channel: Vec<Vec<i16>>,
    /// scale factor per channel
    pub scale_factors: Vec<f32>,
}

pub enum Progress 
{
    Encoding(f32),
    Decoding(f32),
    Exporting(f32),
    Complete(String),
    Error(String),
    Status(String),
}

pub struct AudioChunk 
{
    pub samples: Vec<f32>, // interleaved
    pub is_last: bool,
}

//
// Precomputed tables and helpers
//
#[derive(Clone)]
struct MdctTables 
{
    cos_table: Arc<Vec<f32>>, // length = N * FRAME_SIZE
    window: Arc<Vec<f32>>,    // length = FRAME_SIZE
    n: usize,                 // HOP_SIZE
    norm: f32,                // normalization factor sqrt(2/N)
}

impl MdctTables 
{
    fn new(n: usize) -> Self 
    {
        let block = FRAME_SIZE;
        let mut table = Vec::with_capacity(n * block);
        for k in 0..n 
        {
            for i in 0..block 
            {
                let angle = PI / (n as f32) * (i as f32 + 0.5 + (n as f32) / 2.0) * (k as f32 + 0.5);
                table.push(angle.cos());
            }
        }

        let window = (0..block)
            .map(|i| (PI * (i as f32 + 0.5) / (block as f32)).sin())
            .collect();

        let norm = (2.0 / n as f32).sqrt();

        Self 
        {
            cos_table: Arc::new(table),
            window: Arc::new(window),
            n,
            norm,
        }
    }

    /// MDCT (block len FRAME_SIZE -> N coefficients)
    fn mdct_block(&self, block: &[f32], out: &mut [f32]) 
    {
        let n = self.n;
        let base = self.cos_table.as_ref();
        for k in 0..n 
        {
            let mut s = 0.0f32;
            let tb = &base[k * FRAME_SIZE .. k * FRAME_SIZE + FRAME_SIZE];
            for i in 0..FRAME_SIZE 
            {
                s += block[i] * tb[i];
            }
            // apply normalization here so encoder and decoder use same factor
            out[k] = s * self.norm;
        }
    }

    /// IMDCT: N coeffs -> FRAME_SIZE out
    fn imdct_block(&self, coeffs: &[f32], out: &mut [f32]) 
    {
        let base = self.cos_table.as_ref();
        for i in 0..FRAME_SIZE 
        {
            let mut s = 0.0f32;
            for k in 0..self.n 
            {
                s += coeffs[k] * base[k * FRAME_SIZE + i];
            }
            // apply same normalization (symmetric)
            out[i] = s * self.norm;
        }
    }
}

//
// Encoder: per-channel encoding, frames parallelized
//
pub struct Encoder 
{
    tables: Arc<MdctTables>,
    window: Arc<Vec<f32>>,
}

impl Encoder 
{
    pub fn new() -> Self 
    {
        let n = HOP_SIZE;
        let tables = Arc::new(MdctTables::new(n));
        Self 
        {
            window: tables.window.clone(),
            tables,
        }
    }

    /// samples: interleaved PCM
    pub fn encode(&mut self, samples: &[f32], sample_rate: u32, channels: u16) -> Result<EncodedAudio> 
    {
        let total_samples = samples.len() as u64;
        let ch = channels as usize;

        // deinterleave channels
        let mut per_chan: Vec<Vec<f32>> = vec![Vec::with_capacity(samples.len() / ch + 8); ch];
        for (i, &s) in samples.iter().enumerate() 
        {
            per_chan[i % ch].push(s);
        }

        // pad per-channel (half-hop at start and end)
        let mut padded: Vec<Vec<f32>> = Vec::with_capacity(ch);
        for c in 0..ch 
        {
            let mut v = Vec::with_capacity(per_chan[c].len() + HOP_SIZE);
            v.extend(std::iter::repeat(0.0f32).take(HOP_SIZE / 2));
            v.extend_from_slice(&per_chan[c]);
            let rem = v.len() % HOP_SIZE;
            if rem != 0 
            {
                v.extend(std::iter::repeat(0.0f32).take(HOP_SIZE - rem));
            }
            v.extend(std::iter::repeat(0.0f32).take(HOP_SIZE / 2));
            padded.push(v);
        }

        // number of frames
        let num_frames = if padded[0].len() < FRAME_SIZE 
        {
            1usize
        } else 
        {
            (padded[0].len() - FRAME_SIZE) / HOP_SIZE + 1
        };

        let tables = self.tables.clone();
        let window = self.window.clone();

        // For each frame index, compute per-channel coeffs in parallel across frames
        let frames: Vec<EncodedFrame> = (0..num_frames).into_par_iter().map(|fi| 
        {
            let mut mdct_coeffs_per_channel: Vec<Vec<i16>> = Vec::with_capacity(ch);
            let mut scale_factors: Vec<f32> = Vec::with_capacity(ch);

            for c in 0..ch 
            {
                let start = fi * HOP_SIZE;
                let slice = &padded[c][start .. start + FRAME_SIZE];

                // apply window to block
                let mut block = vec![0.0f32; FRAME_SIZE];
                for i in 0..FRAME_SIZE {
                    block[i] = slice[i] * window[i];
                }

                // compute MDCT
                let mut coeffs = vec![0.0f32; tables.n];
                tables.mdct_block(&block, &mut coeffs);

                // find per-channel scale
                let max_val = coeffs.iter().map(|x| x.abs()).fold(0.0f32, f32::max).max(1e-10);
                scale_factors.push(max_val);

                // quantize per-channel
                let mut qvec = vec![0i16; tables.n];
                for k in 0..tables.n 
                {
                    let normalized = coeffs[k] / max_val;
                    let quantized = (normalized * (1 << QUANTIZATION_BITS) as f32).round();
                    qvec[k] = quantized.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                }
                mdct_coeffs_per_channel.push(qvec);
            }

            EncodedFrame 
            {
                mdct_coeffs_per_channel,
                scale_factors,
            }
        }).collect();

        // compute padding metadata for gapless
        let padded_len = padded[0].len();
        let orig_len = per_chan[0].len();
        let padding = (padded_len - orig_len - (HOP_SIZE / 2)) as u32;
        let encoder_delay = (HOP_SIZE / 2) as u32;

        Ok(EncodedAudio 
        {
            header: AudioHeader 
            {
                sample_rate,
                channels,
                total_samples,
            },
            frames,
            gapless_info: GaplessInfo 
            {
                encoder_delay,
                padding,
                original_length: total_samples,
            },
        })
    }
}

//
// Decoder: per-channel overlap buffers, batch-parallel decode
//
pub struct Decoder 
{
    tables: Arc<MdctTables>,
    window: Arc<Vec<f32>>,
    overlap_buffers: Vec<Vec<f32>>, // per-channel overlap (len HOP_SIZE)
}

impl Decoder 
{
    pub fn new(channels: usize) -> Self 
    {
        let tables = Arc::new(MdctTables::new(HOP_SIZE));
        let window = tables.window.clone();
        let overlap_buffers = vec![vec![0.0f32; HOP_SIZE]; channels];
        Self 
        {
            tables,
            window,
            overlap_buffers,
        }
    }

    /// decode_streaming: decode frames in batch-parallel fashion, produce interleaved chunks
    pub fn decode_streaming(&mut self, encoded: Arc<EncodedAudio>, progress_sender: Option<Sender<Progress>>) -> Receiver<AudioChunk> 
    {
        let (tx, rx) = bounded(5);
        let channels = encoded.header.channels as usize;
        let tables = self.tables.clone();
        let window = self.window.clone();
        // local overlap buffers per-thread: start from current state
        let mut overlap = vec![vec![0.0f32; HOP_SIZE]; channels];

        std::thread::spawn(move || 
        {
            let start_time = Instant::now();
            let total_frames = encoded.frames.len();
            if let Some(ref s) = progress_sender 
            {
                let _ = s.send(Progress::Status(format!("Starting streaming decode of {} frames", total_frames)));
            }

            let mut chunk_samples: Vec<f32> = Vec::with_capacity(FRAMES_PER_CHUNK * HOP_SIZE * channels);
            let mut idx = 0usize;

            while idx < total_frames 
            {
                let batch_end = (idx + DECODE_BATCH).min(total_frames);

                // decode frames in parallel across the batch
                let batch_results: Vec<(usize, Vec<Vec<f32>>)> = (idx..batch_end).into_par_iter().map(|fi| 
                {
                    let frame = &encoded.frames[fi];
                    // per-channel out blocks
                    let mut per_channel_blocks: Vec<Vec<f32>> = Vec::with_capacity(channels);

                    for ch in 0..channels 
                    {
                        // dequantize using per-channel scale
                        let mut coeffs = vec![0.0f32; tables.n];
                        let qvec = &frame.mdct_coeffs_per_channel[ch];
                        let scale = frame.scale_factors[ch].max(1e-12);

                        for k in 0..tables.n 
                        {
                            coeffs[k] = (qvec[k] as f32 / (1 << QUANTIZATION_BITS) as f32) * scale;
                        }

                        // IMDCT to FRAME_SIZE
                        let mut out_block = vec![0.0f32; FRAME_SIZE];
                        tables.imdct_block(&coeffs, &mut out_block);

                        // apply window
                        for i in 0..FRAME_SIZE 
                        {
                            out_block[i] *= window[i];
                        }

                        per_channel_blocks.push(out_block);
                    }

                    (fi, per_channel_blocks)
                }).collect();

                // sort by frame index to preserve time order (par_iter may produce out-of-order)
                let mut batch_results = batch_results;
                batch_results.sort_unstable_by_key(|(fi, _)| *fi);

                for (_fi, per_channel_blocks) in batch_results.into_iter() 
                {
                    // For each frame, do per-channel overlap-add, produce HOP_SIZE samples per-channel, then interleave them
                    // accumulate interleaved HOP_SIZE * channels samples into chunk_samples
                    for i in 0..HOP_SIZE 
                    {
                        for ch in 0..channels 
                        {
                            let val = overlap[ch][i] + per_channel_blocks[ch][i];
                            chunk_samples.push(val);
                        }
                    }

                    // update overlap buffers with second half of each channel
                    for ch in 0..channels 
                    {
                        let second_half = &per_channel_blocks[ch][HOP_SIZE..FRAME_SIZE];
                        overlap[ch].copy_from_slice(second_half);
                    }

                    // periodically flush chunk
                    if chunk_samples.len() >= FRAMES_PER_CHUNK * HOP_SIZE * channels 
                    {
                        if let Some(ref s) = progress_sender 
                        {
                            let progress = (idx as f32) / (total_frames as f32) * 100.0;
                            let _ = s.send(Progress::Decoding(progress));
                        }
                        let _ = tx.send(AudioChunk { samples: chunk_samples.clone(), is_last: false });
                        chunk_samples.clear();
                    }
                    idx += 1;
                }
            }

            // after all frames, append final overlap tails interleaved
            for i in 0..HOP_SIZE 
            {
                for ch in 0..channels 
                {
                    chunk_samples.push(overlap[ch][i]);
                }
            }

            // send last chunk
            let _ = tx.send(AudioChunk { samples: chunk_samples.clone(), is_last: true });

            if let Some(ref s) = progress_sender 
            {
                let _ = s.send(Progress::Complete(format!("Decoded {} frames in {:.2}s", total_frames, start_time.elapsed().as_secs_f32())));
            }
        });

        rx
    }

    /// convenience decode (synchronous)
    pub fn decode(&mut self, encoded: &EncodedAudio, progress_sender: Option<Sender<Progress>>) -> Result<Vec<f32>> 
    {
        let arc = Arc::new(encoded.clone());
        let rx = self.decode_streaming(arc, progress_sender);
        let mut all = Vec::new();
        while let Ok(chunk) = rx.recv() 
        {
            all.extend(chunk.samples);
            if chunk.is_last { break; }
        }

        // gapless trimming
        let delay = encoded.gapless_info.encoder_delay as usize;
        let original_length = encoded.gapless_info.original_length as usize;
        if all.len() > delay 
        {
            all.drain(0..delay);
        }
        if all.len() > original_length 
        {
            all.truncate(original_length);
        }

        Ok(all)
    }
}

//
// Save / load binary
//
pub fn save_encoded(encoded: &EncodedAudio, path: &std::path::Path) -> Result<()> 
{
    let data = bincode::serialize(encoded)?;
    std::fs::write(path, data)?;
    Ok(())
}

pub fn load_encoded(path: &std::path::Path) -> Result<EncodedAudio> 
{
    let data = std::fs::read(path)?;
    let encoded: EncodedAudio = bincode::deserialize(&data)?;
    Ok(encoded)
}

