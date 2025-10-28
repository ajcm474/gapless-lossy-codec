//! Lossy codec with MDCT, psychoacoustic masking, and gapless playback
//! - Precomputed cosine table
//! - Parallel encode and batch-parallel decode (rayon)
//! - Proper multichannel storage: per-frame, per-channel coeffs & scales
//! - Matching normalization on MDCT and IMDCT
//! - Preserves gapless playback via Overlap-Add
use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::f32::consts::PI;
use crossbeam_channel::{Sender, Receiver, bounded};
use std::time::Instant;
use std::sync::Arc;
use rayon::prelude::*;

const FRAME_SIZE: usize = 2048;  // 2N (samples per MDCT block)
const HOP_SIZE: usize = 1024;    // N (hop, 50% overlap)
const QUANTIZATION_BITS: u32 = 16;
const FRAMES_PER_CHUNK: usize = 500;
const DECODE_BATCH: usize = 32;  // how many frames to decode in parallel per batch

// Lossy compression parameters
const NOISE_FLOOR_DB: f32 = -48.0;
const QUALITY_FACTOR: f32 = 0.7;     // Lower = more aggressive compression (0.1-1.0)
const MIN_QUANTIZATION_BITS: u32 = 8;  // Use fewer bits for less important coefficients
const MAX_QUANTIZATION_BITS: u32 = 16;  // Full resolution for important coefficients

// Per-frame compression threshold
// If compressed frame would be >= this fraction of raw PCM size, use raw PCM
const COMPRESSION_THRESHOLD: f32 = 0.85;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncodedAudio
{
    pub header: AudioHeader,
    pub frames: Vec<EncodedFrame>, // time-ordered frames (empty if raw_pcm is used)
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
    /// Sparse representation: (index, value) pairs for non-zero coefficients
    /// Outer vec: channel index -> inner vec: sparse coefficient data
    /// Empty if raw_pcm is used
    pub sparse_coeffs_per_channel: Vec<Vec<(u16, i16)>>,
    /// scale factor per channel (empty if raw_pcm is used)
    pub scale_factors: Vec<f32>,
    /// Raw PCM data for this frame if compression is ineffective
    /// Stores interleaved i16 samples for all channels
    /// Length should be HOP_SIZE * channels
    pub raw_pcm: Option<Vec<i16>>,
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
    pub samples: Vec<f32>, // interleaved if multichannel
    pub is_last: bool,
}

//
// Lossy compression helpers
//

/// Precomputed perceptual weights (shared across all frames)
#[derive(Clone)]
struct PerceptualWeights
{
    weights: Arc<Vec<f32>>,
    critical_bands: Arc<Vec<usize>>,
    sample_rate: u32,
}

impl PerceptualWeights
{
    fn new(n: usize, sample_rate: u32) -> Self
    {
        let weights: Vec<f32> = (0..n).map(|k|
        {
            // Frequency in normalized units (0 to 0.5 = DC to Nyquist)
            let norm_freq = k as f32 / (2.0 * n as f32);
            let freq_hz = norm_freq * sample_rate as f32;

            let weight: f32 = if freq_hz < 100.0
            {
                0.3 + (freq_hz / 100.0) * 0.4  // Ramp up from DC
            }
            else if freq_hz < 200.0
            {
                0.7 + ((freq_hz - 100.0) / 100.0) * 0.3
            }
            else if freq_hz < 5000.0
            {
                1.0  // Peak sensitivity
            }
            else if freq_hz < 10000.0
            {
                1.0 - ((freq_hz - 5000.0) / 5000.0) * 0.3
            }
            else
            {
                0.7 - ((freq_hz - 10000.0) / 12000.0).min(1.0) * 0.5
            };

            // Don't assign any weights less than 0.2
            weight.max(0.2)
        }).collect();

        let critical_bands = Self::compute_critical_bands(n, sample_rate);

        Self
        {
            weights: Arc::new(weights),
            critical_bands: Arc::new(critical_bands),
            sample_rate,
        }
    }

    /// Compute approximate critical band edges (simplified Bark scale)
    fn compute_critical_bands(n: usize, sample_rate: u32) -> Vec<usize>
    {
        let mut bands = vec![0];
        let nyquist = sample_rate as f32 / 2.0;

        // Start with 100 Hz spacing at low frequencies, increase to ~1000 Hz at high frequencies
        let mut freq = 0.0f32;

        while freq < nyquist && bands.len() < 50  // Limit to reasonable number of bands
        {
            let bin = ((freq / nyquist) * n as f32) as usize;
            if bin > *bands.last().unwrap() && bin < n
            {
                bands.push(bin);
            }

            // Logarithmic spacing: wider bands at higher frequencies
            if freq < 500.0
            {
                freq += 50.0;   // 50 Hz bands below 500 Hz
            }
            else if freq < 2000.0
            {
                freq += 100.0;  // 100 Hz bands 500-2000 Hz
            }
            else if freq < 8000.0
            {
                freq += 250.0;  // 250 Hz bands 2000-8000 Hz
            }
            else
            {
                freq += 500.0;  // 500 Hz bands above 8000 Hz
            }
        }

        bands.push(n);
        bands
    }
}

/// Apply psychoacoustic masking to determine which coefficients can be discarded
/// Returns a threshold per coefficient based on perceptual importance
fn compute_masking_thresholds(
    coeffs: &[f32],
    quality: f32,
    perceptual: &PerceptualWeights,
) -> Vec<f32>
{
    let n = coeffs.len();
    let mut thresholds = vec![0.0f32; n];

    // Find global maximum for reference
    let global_max = coeffs.iter().map(|x| x.abs()).fold(0.0f32, f32::max).max(1e-10);

    let perceptual_weights = perceptual.weights.as_ref();
    let band_edges = perceptual.critical_bands.as_ref();

    // Process each critical band
    for band_idx in 0..band_edges.len().saturating_sub(1)
    {
        let start = band_edges[band_idx];
        let end = band_edges[band_idx + 1].min(n);

        if start >= end { continue; }

        // Compute band energy (RMS)
        let energy = (coeffs[start..end].iter()
                                        .map(|x| x * x)
                                        .sum::<f32>() / (end - start) as f32)
            .sqrt();

        // Average perceptual weight for this band
        let avg_weight = perceptual_weights[start..end].iter().sum::<f32>() / (end - start) as f32;

        // Masking threshold based on quality and perceptual importance
        let compression_factor = (1.0 - quality).max(0.01);
        let perceptual_factor = 1.0 / avg_weight.max(0.1);
        let base_threshold = energy * 0.01 * compression_factor * perceptual_factor;

        // Apply to all coefficients in band
        for i in start..end
        {
            let individual_factor = 1.0 / perceptual_weights[i].max(0.1);
            thresholds[i] = base_threshold * individual_factor;

            // Don't threshold away the largest peaks too aggressively
            if coeffs[i].abs() > global_max * 0.3
            {
                thresholds[i] = thresholds[i].min(global_max * 0.05);
            }
        }
    }

    thresholds
}

/// Determine quantization bits based on coefficient importance (fast version)
#[inline]
fn compute_quantization_bits_fast(
    abs_val: f32,
    threshold: f32,
    global_max: f32,
) -> u32
{
    if abs_val <= threshold
    {
        return 0;
    }

    // More important coefficients (higher above threshold) get more bits
    let importance = (abs_val / threshold).log2().max(0.0);
    let relative_magnitude = abs_val / global_max;

    // Combine importance and magnitude
    let score = importance * 0.3 + relative_magnitude * 0.7;

    // Map score to bit depth
    let bits = MIN_QUANTIZATION_BITS +
        ((score * (MAX_QUANTIZATION_BITS - MIN_QUANTIZATION_BITS) as f32) as u32);

    bits.clamp(MIN_QUANTIZATION_BITS, MAX_QUANTIZATION_BITS)
}

/// Apply noise floor and return sparse representation with fixed quantization denominator
fn compress_coefficients(
    coeffs: &[f32],
    scale: f32,
    thresholds: &[f32],
    noise_floor_db: f32,
) -> Vec<(u16, i16)>
{
    let noise_floor_linear = 10.0_f32.powf(noise_floor_db / 20.0) * scale;
    let global_max = coeffs.iter().map(|x| x.abs()).fold(0.0f32, f32::max).max(1e-10);

    // We use (1 << (QUANTIZATION_BITS-1)) to leave room for sign.
    let max_q = (1u32 << (QUANTIZATION_BITS - 1)) as f32;

    let mut sparse = Vec::with_capacity(coeffs.len() / 4);

    for (k, &coeff) in coeffs.iter().enumerate()
    {
        let abs_val = coeff.abs();
        let threshold = thresholds[k] * scale;

        // Keep coefficient if above noise floor AND above perceptual threshold
        if abs_val > noise_floor_linear && abs_val > threshold
        {
            let importance_bits = compute_quantization_bits_fast(abs_val, threshold, global_max);
            if importance_bits == 0
            {
                continue;
            }

            let normalized = coeff / scale;
            let quantized = (normalized * max_q).round();
            let q = quantized.clamp(i16::MIN as f32, i16::MAX as f32) as i16;

            if q != 0
            {
                sparse.push((k as u16, q));
            }
        }
    }

    sparse
}

/// Pre-computed tables for Modified Discrete Cosine Transform (MDCT)
/// See [https://en.wikipedia.org/wiki/Modified_discrete_cosine_transform]
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
        // Pre-compute angles for cosine term
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

        // Use sine window function with FRAME_SIZE as the window length
        // (this avoids discontinuities at the frame boundaries)
        let window = (0..block)
            .map(|i| (PI * (i as f32 + 0.5) / (block as f32)).sin())
            .collect();

        // √(2/N) normalization factor for orthonormal scaling
        let norm = (2.0 / n as f32).sqrt();

        Self 
        {
            cos_table: Arc::new(table),
            window: Arc::new(window),
            n,
            norm,
        }
    }

    /// Modified Discrete Cosine Transform: block len FRAME_SIZE -> N coeffs
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

    /// Inverse Modified Discrete Cosine Transform: N coeffs -> FRAME_SIZE out
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
    perceptual: Arc<PerceptualWeights>,
    sample_rate: u32,
}

impl Encoder 
{
    pub fn new(sample_rate: u32) -> Self
    {
        let n = HOP_SIZE;
        let tables = Arc::new(MdctTables::new(n));
        let perceptual = Arc::new(PerceptualWeights::new(n, sample_rate));
        Self 
        {
            window: tables.window.clone(),
            tables,
            perceptual,
            sample_rate
        }
    }

    /// Encode PCM `samples` (interleaved if multichannel) to our GLC format
    pub fn encode(&mut self, samples: &[f32], channels: u16) -> Result<EncodedAudio>
    {
        let total_samples = samples.len() as u64;
        let ch = channels as usize;

        // Deinterleave channels
        let mut per_chan: Vec<Vec<f32>> = vec![Vec::with_capacity(samples.len() / ch + 8); ch];
        for (i, &s) in samples.iter().enumerate()
        {
            per_chan[i % ch].push(s);
        }

        // Pad per-channel
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

        let num_frames = if padded[0].len() < FRAME_SIZE
        {
            1usize
        } else
        {
            (padded[0].len() - FRAME_SIZE) / HOP_SIZE + 1
        };

        let tables = self.tables.clone();
        let window = self.window.clone();
        let perceptual = self.perceptual.clone();

        // Encode frames in parallel, deciding per-frame whether to use compression
        let frames: Vec<EncodedFrame> = (0..num_frames).into_par_iter().map(|fi|
        {
            let mut sparse_coeffs_per_channel: Vec<Vec<(u16, i16)>> = Vec::with_capacity(ch);
            let mut scale_factors: Vec<f32> = Vec::with_capacity(ch);

            // Extract raw frame samples for fallback consideration
            // IMPORTANT: Store FRAME_SIZE samples to maintain overlap-add structure
            let mut raw_frame_samples: Vec<i16> = Vec::with_capacity(FRAME_SIZE * ch);

            for c in 0..ch
            {
                let start = fi * HOP_SIZE;
                let slice = &padded[c][start .. start + FRAME_SIZE];

                // Apply window
                let mut block = vec![0.0f32; FRAME_SIZE];
                for i in 0..FRAME_SIZE
                {
                    block[i] = slice[i] * window[i];
                }

                // Compute MDCT
                let mut coeffs = vec![0.0f32; tables.n];
                tables.mdct_block(&block, &mut coeffs);

                // Find per-channel scale
                let max_val = coeffs.iter().map(|x| x.abs()).fold(0.0f32, f32::max).max(1e-10);
                scale_factors.push(max_val);

                // Compute masking thresholds and compress
                let thresholds = compute_masking_thresholds(&coeffs, QUALITY_FACTOR, &perceptual);
                let sparse = compress_coefficients(&coeffs, max_val, &thresholds, NOISE_FLOOR_DB);
                sparse_coeffs_per_channel.push(sparse);

                // Collect raw samples for this channel (ENTIRE FRAME_SIZE with window applied)
                // This maintains the overlap-add structure
                for i in 0..FRAME_SIZE
                {
                    let sample = slice[i] * window[i];
                    raw_frame_samples.push((sample * 32767.0).clamp(-32768.0, 32767.0) as i16);
                }
            }

            // Estimate compressed size for this frame
            let mut compressed_size = 0usize;
            for sparse_channel in &sparse_coeffs_per_channel
            {
                // Vec length (8 bytes) + sparse entries (4 bytes each)
                compressed_size += 8 + sparse_channel.len() * 4;
            }
            // Add scale factors: Vec length + f32 per channel
            compressed_size += 8 + scale_factors.len() * 4;
            // Add frame overhead
            compressed_size += 64;

            // Raw PCM size for this frame (i16 samples, interleaved, FRAME_SIZE per channel)
            let raw_size = FRAME_SIZE * ch * 2; // 2 bytes per i16

            // Decide: use compression or raw PCM?
            if compressed_size as f32 >= (raw_size as f32 * COMPRESSION_THRESHOLD)
            {
                // Use raw PCM fallback for this frame
                EncodedFrame
                {
                    sparse_coeffs_per_channel: Vec::new(),
                    scale_factors: Vec::new(),
                    raw_pcm: Some(raw_frame_samples),
                }
            }
            else
            {
                // Use compression
                EncodedFrame
                {
                    sparse_coeffs_per_channel,
                    scale_factors,
                    raw_pcm: None,
                }
            }
        }).collect();

        // Compute padding metadata
        let padded_len = padded[0].len();
        let orig_len = per_chan[0].len();
        let padding = (padded_len - orig_len - (HOP_SIZE / 2)) as u32;
        let encoder_delay = (HOP_SIZE / 2) as u32;

        Ok(EncodedAudio
        {
            header: AudioHeader
            {
                sample_rate: self.sample_rate,
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
    sample_rate: u32, // informational (for playback)
    channels: usize,
}

impl Decoder 
{
    pub fn new(channels: usize, sample_rate: u32) -> Self
    {
        let tables = Arc::new(MdctTables::new(HOP_SIZE));
        let window = tables.window.clone();
        Self 
        {
            tables,
            window,
            sample_rate,
            channels,
        }
    }

    /// Decode frames in batch-parallel fashion, producing interleaved chunks
    pub fn decode_streaming(&mut self, encoded: Arc<EncodedAudio>, progress_sender: Option<Sender<Progress>>) -> Receiver<AudioChunk>
    {
        let (tx, rx) = bounded(5);
        let channels = encoded.header.channels as usize;
        let tables = self.tables.clone();
        let window = self.window.clone();
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

                // Decode frames in parallel
                let batch_results: Vec<(usize, Vec<Vec<f32>>)> = (idx..batch_end).into_par_iter().map(|fi|
                {
                    let frame = &encoded.frames[fi];
                    let mut per_channel_blocks: Vec<Vec<f32>> = Vec::with_capacity(channels);

                    // Check if this frame uses raw PCM
                    if let Some(ref raw_pcm) = frame.raw_pcm
                    {
                        // Decode raw PCM: deinterleave and convert i16 to f32
                        for ch in 0..channels
                        {
                            let mut channel_block = vec![0.0f32; FRAME_SIZE];
                            // Fill first FRAME_SIZE with decoded samples
                            for i in 0..FRAME_SIZE
                            {
                                let sample_idx = i * channels + ch;
                                if sample_idx < raw_pcm.len()
                                {
                                    channel_block[i] = raw_pcm[sample_idx] as f32 / 32767.0;
                                }
                            }

                            per_channel_blocks.push(channel_block);
                        }
                    }
                    else
                    {
                        // Decode using MDCT
                        for ch in 0..channels
                        {
                            // Reconstruct coefficients from sparse representation
                            let mut coeffs = vec![0.0f32; tables.n];
                            let sparse_data = &frame.sparse_coeffs_per_channel[ch];
                            let scale = frame.scale_factors[ch].max(1e-12);

                            // use same denominator as encoder
                            let max_q = (1u32 << (QUANTIZATION_BITS - 1)) as f32;

                            // Fill in non-zero coefficients
                            for &(index, quantized_val) in sparse_data
                            {
                                if (index as usize) < tables.n
                                {
                                    coeffs[index as usize] = (quantized_val as f32 / max_q) * scale;
                                }
                            }

                            // IMDCT to FRAME_SIZE
                            let mut out_block = vec![0.0f32; FRAME_SIZE];
                            tables.imdct_block(&coeffs, &mut out_block);

                            // Apply window
                            for i in 0..FRAME_SIZE
                            {
                                out_block[i] *= window[i];
                            }

                            per_channel_blocks.push(out_block);
                        }
                    }

                    (fi, per_channel_blocks)
                }).collect();

                // sort by frame index to preserve time order (par_iter may produce out-of-order)
                let mut batch_results = batch_results;
                batch_results.sort_unstable_by_key(|(fi, _)| *fi);

                for (_fi, per_channel_blocks) in batch_results.into_iter()
                {
                    // Overlap-add and interleave
                    for i in 0..HOP_SIZE
                    {
                        for ch in 0..channels
                        {
                            let val = overlap[ch][i] + per_channel_blocks[ch][i];
                            chunk_samples.push(val);
                        }
                    }

                    // Update overlap buffers
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

            // Final overlap
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

