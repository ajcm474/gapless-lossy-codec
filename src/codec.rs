use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use std::f32::consts::PI;
use rayon::prelude::*;
use crossbeam_channel::{Sender, Receiver, bounded};
use std::time::Instant;
use std::sync::Arc;

const FRAME_SIZE: usize = 1024;
const OVERLAP_SIZE: usize = 128;
const QUANTIZATION_BITS: u32 = 8;
const FRAMES_PER_CHUNK: usize = 1000; // Decode in chunks of 1000 frames

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncodedAudio {
    pub header: AudioHeader,
    pub frames: Vec<EncodedFrame>,
    pub gapless_info: GaplessInfo,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AudioHeader {
    pub sample_rate: u32,
    pub channels: u16,
    pub total_samples: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GaplessInfo {
    pub encoder_delay: u32,
    pub padding: u32,
    pub original_length: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncodedFrame {
    pub mdct_coeffs: Vec<i16>,
    pub scale_factor: f32,
}

pub enum Progress {
    Encoding(f32),
    Decoding(f32),
    Exporting(f32),
    Complete(String),
    Error(String),
    Status(String),
}

pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub is_last: bool,
}

pub struct Encoder {
    window: Vec<f32>,
}

impl Encoder {
    pub fn new() -> Self {
        let window = Self::create_window(FRAME_SIZE);
        
        Encoder {
            window,
        }
    }
    
    fn create_window(size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| {
                let phase = PI * (i as f32 + 0.5) / size as f32;
                phase.sin()
            })
            .collect()
    }
    
    pub fn encode(&mut self, samples: &[f32], sample_rate: u32, channels: u16) -> Result<EncodedAudio> {
        println!("Encoding {} samples at {}Hz", samples.len(), sample_rate);
        let total_samples = samples.len() as u64;
        
        // Pad input to frame boundary
        let mut padded_samples = samples.to_vec();
        let remainder = padded_samples.len() % (FRAME_SIZE - OVERLAP_SIZE);
        if remainder != 0 {
            let padding = (FRAME_SIZE - OVERLAP_SIZE) - remainder;
            padded_samples.extend(vec![0.0; padding]);
        }
        
        let encoder_delay = OVERLAP_SIZE as u32 / 2;
        let padding = (padded_samples.len() - samples.len()) as u32;
        
        // Process frames
        let chunks: Vec<_> = padded_samples
            .chunks(FRAME_SIZE - OVERLAP_SIZE)
            .collect();
        
        println!("Processing {} chunks", chunks.len());
        
        let frames: Vec<EncodedFrame> = chunks
            .par_iter()
            .map(|chunk| {
                let mut frame_samples = vec![0.0; FRAME_SIZE];
                let copy_len = chunk.len().min(FRAME_SIZE);
                frame_samples[..copy_len].copy_from_slice(&chunk[..copy_len]);
                self.encode_frame(&frame_samples)
            })
            .collect();
        
        println!("Encoded {} frames", frames.len());
        
        Ok(EncodedAudio {
            header: AudioHeader {
                sample_rate,
                channels,
                total_samples,
            },
            frames,
            gapless_info: GaplessInfo {
                encoder_delay,
                padding,
                original_length: total_samples,
            },
        })
    }
    
    fn encode_frame(&self, samples: &[f32]) -> EncodedFrame {
        // Apply window
        let windowed: Vec<f32> = samples.iter()
            .zip(self.window.iter())
            .map(|(s, w)| s * w)
            .collect();
        
        // Simplified DCT
        let half_size = FRAME_SIZE / 2;
        let mut dct_coeffs = vec![0.0; half_size];
        
        for k in 0..half_size {
            let mut sum = 0.0;
            let step = if k < 64 { 1 } else { 4 };
            for (idx, &sample) in windowed.iter().enumerate().step_by(step) {
                let angle = PI * k as f32 * (idx as f32 + 0.5) / FRAME_SIZE as f32;
                sum += sample * angle.cos();
            }
            dct_coeffs[k] = sum * (2.0 / FRAME_SIZE as f32).sqrt();
        }
        
        let max_val = dct_coeffs.iter()
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);
        
        let scale_factor = if max_val > 0.0 {
            max_val
        } else {
            1.0
        };
        
        let quantized: Vec<i16> = dct_coeffs.iter()
            .enumerate()
            .map(|(i, &coeff)| {
                let freq_factor = 1.0 - (i as f32 / dct_coeffs.len() as f32) * 0.7;
                let masked_coeff = coeff * freq_factor;
                
                let normalized = masked_coeff / scale_factor;
                let quantized = (normalized * (1 << QUANTIZATION_BITS) as f32) as i16;
                quantized
            })
            .collect();
        
        EncodedFrame {
            mdct_coeffs: quantized,
            scale_factor,
        }
    }
}

pub struct Decoder {
    window: Vec<f32>,
    overlap_buffer: Vec<f32>,
}

impl Decoder {
    pub fn new() -> Self {
        let window = Encoder::create_window(FRAME_SIZE);
        Decoder {
            window,
            overlap_buffer: vec![0.0; OVERLAP_SIZE],
        }
    }
    
    // Streaming decode - returns a receiver that will get chunks of audio
    pub fn decode_streaming(&mut self, 
                           encoded: Arc<EncodedAudio>, 
                           progress_sender: Option<Sender<Progress>>) 
                           -> Receiver<AudioChunk> {
        let (tx, rx) = bounded(5); // Small buffer of decoded chunks
        let mut decoder = Decoder::new(); // Create a fresh decoder for the thread
        
        std::thread::spawn(move || {
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
            let mut total_decoded_samples = 0usize;
            let mut frames_decoded = 0;
            
            for (idx, frame) in encoded.frames.iter().enumerate() {
                // Decode frame
                let frame_samples = decoder.decode_frame_simple(frame);
                
                // Handle overlap-add
                let overlap_len = decoder.overlap_buffer.len().min(frame_samples.len());
                for i in 0..overlap_len {
                    chunk_samples.push(decoder.overlap_buffer[i] + frame_samples[i]);
                }
                
                // Add non-overlapping part
                if frame_samples.len() > OVERLAP_SIZE {
                    let end = frame_samples.len().saturating_sub(OVERLAP_SIZE);
                    if OVERLAP_SIZE < end {
                        chunk_samples.extend_from_slice(&frame_samples[OVERLAP_SIZE..end]);
                    }
                    
                    // Update overlap buffer
                    let start = frame_samples.len().saturating_sub(OVERLAP_SIZE);
                    if start < frame_samples.len() && frame_samples.len() >= OVERLAP_SIZE {
                        decoder.overlap_buffer.copy_from_slice(&frame_samples[start..]);
                    }
                }
                
                frames_decoded += 1;
                total_decoded_samples += frame_samples.len();
                
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
                    let mut samples_to_send = chunk_samples.clone();
                    let is_last = idx == total_frames - 1;
                    
                    if is_last {
                        // Apply gapless trimming to the entire output
                        let start = encoded.gapless_info.encoder_delay as usize;
                        let original_length = encoded.gapless_info.original_length as usize;
                        
                        // Calculate how many samples we've sent so far (excluding this chunk)
                        let previously_sent = total_decoded_samples - samples_to_send.len();
                        
                        // Trim the current chunk if needed
                        if previously_sent < original_length {
                            let chunk_end = (original_length - previously_sent).min(samples_to_send.len());
                            if start > previously_sent {
                                let chunk_start = start - previously_sent;
                                if chunk_start < chunk_end {
                                    samples_to_send = samples_to_send[chunk_start..chunk_end].to_vec();
                                }
                            } else {
                                samples_to_send.truncate(chunk_end);
                            }
                        }
                        
                        println!("Last chunk: applied gapless trimming");
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
    pub fn decode(&mut self, encoded: &EncodedAudio, progress_sender: Option<Sender<Progress>>) -> Result<Vec<f32>> {
        let arc_encoded = Arc::new(encoded.clone());
        let rx = self.decode_streaming(arc_encoded, progress_sender);
        
        let mut all_samples = Vec::new();
        while let Ok(chunk) = rx.recv() {
            all_samples.extend(chunk.samples);
            if chunk.is_last {
                break;
            }
        }
        
        Ok(all_samples)
    }
    
    pub fn reset(&mut self) {
        self.overlap_buffer.fill(0.0);
    }
    
    fn decode_frame_simple(&self, frame: &EncodedFrame) -> Vec<f32> {
        let half_size = frame.mdct_coeffs.len();
        let full_size = half_size * 2;
        
        // Dequantize
        let dct_coeffs: Vec<f32> = frame.mdct_coeffs.iter()
            .map(|&q| {
                let normalized = q as f32 / (1 << QUANTIZATION_BITS) as f32;
                normalized * frame.scale_factor
            })
            .collect();
        
        // Simple IDCT
        let mut samples = vec![0.0; full_size];
        
        for n in 0..full_size {
            let mut sum = 0.0;
            let step = if n < 256 { 1 } else { 2 };
            
            for (k, &coeff) in dct_coeffs.iter().enumerate().step_by(step) {
                let angle = PI * k as f32 * (n as f32 + 0.5) / full_size as f32;
                sum += coeff * angle.cos();
            }
            samples[n] = sum * (2.0 / full_size as f32).sqrt();
        }
        
        // Apply window
        for (i, sample) in samples.iter_mut().enumerate() {
            if i < self.window.len() {
                *sample *= self.window[i];
            }
        }
        
        samples
    }
}

pub fn save_encoded(encoded: &EncodedAudio, path: &std::path::Path) -> Result<()> {
    let data = bincode::serialize(encoded)?;
    std::fs::write(path, data)?;
    Ok(())
}

pub fn load_encoded(path: &std::path::Path) -> Result<EncodedAudio> {
    println!("Loading encoded file: {:?}", path);
    let data = std::fs::read(path)?;
    let encoded: EncodedAudio = bincode::deserialize(&data)?;
    println!("Loaded: {} frames, {} samples", encoded.frames.len(), encoded.header.total_samples);
    Ok(encoded)
}
