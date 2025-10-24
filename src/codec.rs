use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::f32::consts::PI;
use crossbeam_channel::{Sender, Receiver, bounded};
use std::time::Instant;
use std::sync::Arc;

const FRAME_SIZE: usize = 1024;
const OVERLAP_SIZE: usize = 128;
const QUANTIZATION_BITS: u32 = 12;  // Increased from 8 to reduce quantization noise
const FRAMES_PER_CHUNK: usize = 1000; // Decode in chunks of 1000 frames

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncodedAudio 
{
    pub header: AudioHeader,
    pub frames: Vec<EncodedFrame>,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncodedFrame 
{
    pub mdct_coeffs: Vec<i16>,
    pub scale_factor: f32,
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
    pub samples: Vec<f32>,
    pub is_last: bool,
}

pub struct Encoder 
{
    window: Vec<f32>,
}

impl Encoder 
{
    pub fn new() -> Self 
    {
        let window = Self::create_window(FRAME_SIZE);
        
        Encoder 
        {
            window,
        }
    }
    
    fn create_window(size: usize) -> Vec<f32> 
    {
        // Use sine window for MDCT (satisfies COLA constraint for 50% overlap)
        (0..size)
            .map(|i| 
            {
                let phase = PI * (i as f32 + 0.5) / size as f32;
                phase.sin()
            })
            .collect()
    }
    
    pub fn encode(&mut self, samples: &[f32], sample_rate: u32, channels: u16) -> Result<EncodedAudio> 
    {
        println!("Encoding {} samples at {}Hz, {} channels", samples.len(), sample_rate, channels);
        let total_samples = samples.len() as u64;
        
        // For stereo, process each channel separately or interleaved
        // For simplicity, just encode as-is (interleaved if stereo)
        
        // Pad input to frame boundary with proper stride calculation
        let mut padded_samples = samples.to_vec();
        let stride = FRAME_SIZE - OVERLAP_SIZE;  // 896 samples per frame advance
        
        // Add initial padding for the first frame overlap
        let initial_pad = vec![0.0; OVERLAP_SIZE];
        padded_samples.splice(0..0, initial_pad);
        
        // Pad at the end to frame boundary
        let remainder = padded_samples.len() % stride;
        if remainder != 0 
        {
            let padding = stride - remainder;
            padded_samples.extend(vec![0.0; padding]);
        }
        
        // Add final padding for the last frame
        padded_samples.extend(vec![0.0; OVERLAP_SIZE]);
        
        let encoder_delay = OVERLAP_SIZE as u32;  // Full overlap size, not half
        let padding = (padded_samples.len() - samples.len() - OVERLAP_SIZE) as u32;
        
        // Process frames with proper overlapping
        let mut frames = Vec::new();
        let mut pos = 0;
        
        while pos + FRAME_SIZE <= padded_samples.len() 
        {
            let frame_samples = &padded_samples[pos..pos + FRAME_SIZE];
            frames.push(self.encode_frame(frame_samples));
            pos += stride;
        }
        
        println!("Encoded {} frames from {} padded samples", frames.len(), padded_samples.len());
        
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
    
    fn encode_frame(&self, samples: &[f32]) -> EncodedFrame 
    {
        // Apply window
        let windowed: Vec<f32> = samples.iter()
            .zip(self.window.iter())
            .map(|(s, w)| s * w)
            .collect();
        
        // Fixed MDCT implementation using correct formula
        let n = FRAME_SIZE;
        let n2 = n / 2;
        let mut mdct_coeffs = vec![0.0f32; n2];
        
        for k in 0..n2 
        {
            let mut sum = 0.0f32;
            for i in 0..n 
            {
                // Correct MDCT formula: cos(π/N * (n + 0.5 + N/2) * (k + 0.5))
                let angle = PI / (n as f32) * (i as f32 + 0.5 + n as f32 / 2.0) * (k as f32 + 0.5);
                sum += windowed[i] * angle.cos();
            }
            // Proper MDCT normalization
            mdct_coeffs[k] = sum;
        }
        
        // Find max for normalization
        let max_val = mdct_coeffs.iter()
            .map(|x| x.abs())
            .fold(0.0f32, f32::max)
            .max(1e-10);
        
        let scale_factor = max_val;
        
        // Quantize with perceptual weighting
        let quantized: Vec<i16> = mdct_coeffs.iter()
            .enumerate()
            .map(|(i, &coeff)| 
            {
                // Less aggressive frequency masking
                let freq_factor = 1.0 - (i as f32 / mdct_coeffs.len() as f32) * 0.3;
                let masked_coeff = coeff * freq_factor;
                
                let normalized = masked_coeff / scale_factor;
                let quantized = (normalized * (1 << QUANTIZATION_BITS) as f32).round() as i16;
                quantized.clamp(i16::MIN, i16::MAX)
            })
            .collect();
        
        EncodedFrame 
        {
            mdct_coeffs: quantized,
            scale_factor,
        }
    }
}

pub struct Decoder 
{
    window: Vec<f32>,
    overlap_buffer: Vec<f32>,
}

impl Decoder 
{
    pub fn new() -> Self 
    {
        let window = Encoder::create_window(FRAME_SIZE);
        Decoder 
        {
            window,
            overlap_buffer: vec![0.0; OVERLAP_SIZE],
        }
    }
    
    // Streaming decode - returns a receiver that will get chunks of audio
    pub fn decode_streaming(&mut self, 
                           encoded: Arc<EncodedAudio>, 
                           progress_sender: Option<Sender<Progress>>) 
                           -> Receiver<AudioChunk> 
    {
        let (tx, rx) = bounded(5); // Small buffer of decoded chunks
        let mut decoder = Decoder::new(); // Create a fresh decoder for the thread
        
        std::thread::spawn(move || 
        {
            let start_time = Instant::now();
            let total_frames = encoded.frames.len();
            
            println!("Starting streaming decode of {} frames", total_frames);
            
            if let Some(ref sender) = progress_sender {
                let _ = sender.send(Progress::Status(format!(
                    "Starting streaming decode of {} frames", 
                    total_frames
                )));
            }
            
            let mut chunk_samples = Vec::with_capacity(FRAMES_PER_CHUNK * FRAME_SIZE);
            let mut _total_decoded_samples = 0usize;
            let mut frames_decoded = 0;
            
            for (idx, frame) in encoded.frames.iter().enumerate() 
            {
                // Decode frame
                let frame_samples = decoder.decode_frame(frame);
                
                println!("Frame {} decoded: {} samples", idx, frame_samples.len());
                
                // Simple overlap-add processing
                if idx == 0 
                {
                    // First frame - add the first part, store overlap
                    let stride = FRAME_SIZE - OVERLAP_SIZE;
                    chunk_samples.extend_from_slice(&frame_samples[0..stride]);
                    
                    // Store the overlap for next frame
                    if frame_samples.len() >= FRAME_SIZE 
                    {
                        for i in 0..OVERLAP_SIZE 
                        {
                            decoder.overlap_buffer[i] = frame_samples[stride + i];
                        }
                    }
                } else 
                {
                    // Subsequent frames - overlap-add, then continue
                    let overlap_len = OVERLAP_SIZE.min(frame_samples.len());
                    
                    // Add overlapped part
                    for i in 0..overlap_len 
                    {
                        chunk_samples.push(decoder.overlap_buffer[i] + frame_samples[i]);
                    }
                    
                    // Add middle part
                    let stride = FRAME_SIZE - OVERLAP_SIZE;
                    if frame_samples.len() > OVERLAP_SIZE 
                    {
                        let end_idx = (OVERLAP_SIZE + stride).min(frame_samples.len());
                        chunk_samples.extend_from_slice(&frame_samples[OVERLAP_SIZE..end_idx]);
                        
                        // Store overlap for next frame (if not last frame)
                        if idx < encoded.frames.len() - 1 && frame_samples.len() >= FRAME_SIZE 
                        {
                            for i in 0..OVERLAP_SIZE 
                            {
                                if OVERLAP_SIZE + stride + i < frame_samples.len() 
                                {
                                    decoder.overlap_buffer[i] = frame_samples[OVERLAP_SIZE + stride + i];
                                }
                            }
                        }
                    }
                }
                
                frames_decoded += 1;
                _total_decoded_samples += FRAME_SIZE - OVERLAP_SIZE;
                
                // Send chunk when we've decoded enough frames or reached the end
                if frames_decoded % FRAMES_PER_CHUNK == 0 || idx == total_frames - 1 {
                    let progress = (idx as f32 / total_frames as f32) * 100.0;
                    
                    println!("Sending chunk: frames {}-{} ({:.1}%), {} samples", 
                             idx.saturating_sub(FRAMES_PER_CHUNK - 1), idx, progress, chunk_samples.len());
                    
                    if let Some(ref sender) = progress_sender {
                        let _ = sender.send(Progress::Decoding(progress));
                        let _ = sender.send(Progress::Status(format!(
                            "Decoded {}/{} frames ({:.1}%)", 
                            idx + 1, 
                            total_frames, 
                            progress
                        )));
                    }
                    
                    // Apply gapless trimming if this is the last chunk
                    let samples_to_send = chunk_samples.clone();
                    let is_last = idx == total_frames - 1;
                    
                    if is_last 
                    {
                        // Apply gapless trimming - remove encoder delay from beginning and padding from end
                        // Note: We need to apply trimming to the entire output, not just per-chunk
                        
                        // For now, don't apply trimming per-chunk as this can cause issues
                        // Instead, the caller should handle trimming the complete output
                        
                        println!("Last chunk: {} samples (gapless info: delay={}, padding={}, original={})", 
                                samples_to_send.len(),
                                encoded.gapless_info.encoder_delay,
                                encoded.gapless_info.padding,
                                encoded.gapless_info.original_length);
                    }
                    
                    // Send the chunk
                    if let Err(e) = tx.send(AudioChunk { 
                        samples: samples_to_send, 
                        is_last 
                    }) {
                        println!("Failed to send audio chunk: {}", e);
                        break;
                    }
                    
                    // Clear chunk buffer for next batch
                    chunk_samples.clear();
                    chunk_samples.reserve(FRAMES_PER_CHUNK * FRAME_SIZE);
                }
            }
            
            let total_time = start_time.elapsed();
            println!("Streaming decode complete: {} frames in {:.2}s", total_frames, total_time.as_secs_f32());
            
            if let Some(ref sender) = progress_sender {
                let _ = sender.send(Progress::Complete(format!(
                    "Decode complete: {} frames in {:.2}s", 
                    total_frames,
                    total_time.as_secs_f32()
                )));
            }
        });
        
        rx
    }
    
    // Non-streaming decode for backwards compatibility
    pub fn decode(&mut self, encoded: &EncodedAudio, progress_sender: Option<Sender<Progress>>) -> Result<Vec<f32>> 
    {
        let arc_encoded = Arc::new(encoded.clone());
        let rx = self.decode_streaming(arc_encoded, progress_sender);
        
        let mut all_samples = Vec::new();
        while let Ok(chunk) = rx.recv() 
        {
            all_samples.extend(chunk.samples);
            if chunk.is_last 
            {
                break;
            }
        }
        
        // Apply gapless trimming to the complete output
        let delay = encoded.gapless_info.encoder_delay as usize;
        let original_length = encoded.gapless_info.original_length as usize;
        
        if all_samples.len() > delay 
        {
            // Remove encoder delay from the beginning
            all_samples.drain(0..delay);
            
            // Trim to original length if needed
            if all_samples.len() > original_length 
            {
                all_samples.truncate(original_length);
            }
        }
        
        Ok(all_samples)
    }
    
    pub fn reset(&mut self) 
    {
        self.overlap_buffer.fill(0.0);
    }
    
    fn decode_frame(&self, frame: &EncodedFrame) -> Vec<f32> 
    {
        let n2 = frame.mdct_coeffs.len();
        let n = n2 * 2;
        
        // Dequantize with inverse perceptual weighting
        let mdct_coeffs: Vec<f32> = frame.mdct_coeffs.iter()
            .enumerate()
            .map(|(i, &q)| 
            {
                let normalized = q as f32 / (1 << QUANTIZATION_BITS) as f32;
                let scaled = normalized * frame.scale_factor;
                
                // Inverse frequency masking
                let freq_factor = 1.0 - (i as f32 / frame.mdct_coeffs.len() as f32) * 0.3;
                scaled / freq_factor.max(0.1)
            })
            .collect();
        
        // Fixed inverse MDCT implementation using correct formula
        let mut samples = vec![0.0f32; n];
        
        for i in 0..n 
        {
            let mut sum = 0.0f32;
            for k in 0..n2 
            {
                // Correct inverse MDCT formula: same as forward MDCT
                let angle = PI / (n as f32) * (i as f32 + 0.5 + n as f32 / 2.0) * (k as f32 + 0.5);
                sum += mdct_coeffs[k] * angle.cos();
            }
            // Proper inverse MDCT normalization: 2/N
            samples[i] = sum * (2.0 / n as f32);
        }
        
        // Apply window
        for i in 0..n 
        {
            samples[i] *= self.window[i];
        }
        
        samples
    }
}

pub fn save_encoded(encoded: &EncodedAudio, path: &std::path::Path) -> Result<()> 
{
    let data = bincode::serialize(encoded)?;
    std::fs::write(path, data)?;
    Ok(())
}

pub fn load_encoded(path: &std::path::Path) -> Result<EncodedAudio> 
{
    println!("Loading encoded file: {:?}", path);
    let data = std::fs::read(path)?;
    let encoded: EncodedAudio = bincode::deserialize(&data)?;
    println!("Loaded: {} frames, {} samples", encoded.frames.len(), encoded.header.total_samples);
    Ok(encoded)
}
