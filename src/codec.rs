use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use std::f32::consts::PI;
use rayon::prelude::*;
use crossbeam_channel::{Sender, Receiver, bounded};
use std::time::Instant;
use std::sync::Arc;
use rustfft::{FftPlanner, num_complex::Complex};

const FRAME_SIZE: usize = 2048;
const OVERLAP_SIZE: usize = 256;
const QUANTIZATION_BITS: u32 = 16;
const FRAMES_PER_CHUNK: usize = 500;

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
    fft_planner: FftPlanner<f32>,
}

impl Clone for Encoder {
    fn clone(&self) -> Self {
        Encoder {
            window: self.window.clone(),
            fft_planner: FftPlanner::new(),
        }
    }
}

impl Encoder {
    pub fn new() -> Self {
        let window = Self::create_window(FRAME_SIZE);
        let fft_planner = FftPlanner::new();
        
        Encoder {
            window,
            fft_planner,
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
        println!("Encoding {} samples at {}Hz, {} channels", samples.len(), sample_rate, channels);
        let total_samples = samples.len() as u64;
        
        // For stereo, process each channel separately or interleaved
        // For simplicity, just encode as-is (interleaved if stereo)
        
        // Pad input to frame boundary
        let mut padded_samples = samples.to_vec();
        let stride = FRAME_SIZE - OVERLAP_SIZE;
        let remainder = padded_samples.len() % stride;
        if remainder != 0 {
            let padding = stride - remainder;
            padded_samples.extend(vec![0.0; padding]);
        }
        
        let encoder_delay = OVERLAP_SIZE as u32 / 2;
        let padding = (padded_samples.len() - samples.len()) as u32;
        
        // Process frames
        let chunks: Vec<_> = padded_samples
            .chunks(stride)
            .collect();
        
        println!("Processing {} chunks", chunks.len());
        
        let frames: Vec<EncodedFrame> = chunks
            .par_iter()
            .map(|chunk| {
                let mut encoder = self.clone();
                let mut frame_samples = vec![0.0; FRAME_SIZE];
                let copy_len = chunk.len().min(FRAME_SIZE);
                frame_samples[..copy_len].copy_from_slice(&chunk[..copy_len]);
                encoder.encode_frame(&frame_samples)
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
    
    fn encode_frame(&mut self, samples: &[f32]) -> EncodedFrame {
        // Apply window
        let windowed: Vec<f32> = samples.iter()
            .zip(self.window.iter())
            .map(|(s, w)| s * w)
            .collect();
        
        // MDCT via FFT
        let mut complex_input: Vec<Complex<f32>> = windowed.iter()
            .map(|&x| Complex { re: x, im: 0.0 })
            .collect();
        
        let fft = self.fft_planner.plan_fft_forward(FRAME_SIZE);
        fft.process(&mut complex_input);
        
        // Take magnitude of first half (MDCT-like)
        let half_size = FRAME_SIZE / 2;
        let mut mdct_coeffs: Vec<f32> = complex_input[..half_size]
            .iter()
            .map(|c| c.norm() / (FRAME_SIZE as f32).sqrt())
            .collect();
        
        // Find scale factor
        let max_val = mdct_coeffs.iter()
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);
        
        let scale_factor = if max_val > 0.0 { max_val } else { 1.0 };
        
        // Quantize with psychoacoustic masking
        let quantized: Vec<i16> = mdct_coeffs.iter()
            .enumerate()
            .map(|(i, &coeff)| {
                let freq_factor = if i < 20 {
                    1.0
                } else if i < 1000 {
                    1.0 - (i as f32 / 2000.0) * 0.3
                } else {
                    0.7
                };
        
                let masked_coeff = coeff * freq_factor;
                let normalized = masked_coeff / scale_factor;
                let quantized = (normalized * ((1 << QUANTIZATION_BITS) - 1) as f32) as i16;
        
                let max_val = (1i32 << (QUANTIZATION_BITS - 1)) - 1;
                let min_val = -max_val - 1;
                quantized.clamp(min_val as i16, max_val as i16)
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
    fft_planner: FftPlanner<f32>,
}

impl Decoder {
    pub fn new() -> Self {
        let window = Encoder::create_window(FRAME_SIZE);
        let fft_planner = FftPlanner::new();
        Decoder {
            window,
            overlap_buffer: vec![0.0; OVERLAP_SIZE],
            fft_planner,
        }
    }
    
    // Streaming decode - returns a receiver that will get chunks of audio
    pub fn decode_streaming(&mut self, 
                           encoded: Arc<EncodedAudio>, 
                           progress_sender: Option<Sender<Progress>>) 
                           -> Receiver<AudioChunk> {
        let (tx, rx) = bounded(5);
        let mut decoder = Decoder::new();
        
        std::thread::spawn(move || {
            let start_time = Instant::now();
            let total_frames = encoded.frames.len();
            
            println!("Starting streaming decode of {} frames", total_frames);
            
            let mut chunk_samples = Vec::with_capacity(FRAMES_PER_CHUNK * (FRAME_SIZE - OVERLAP_SIZE));
            let mut total_output_samples = 0usize;
            let mut frames_decoded = 0;
            let mut is_first_frame = true;
            
            for (idx, frame) in encoded.frames.iter().enumerate() {
                // Decode frame
                let frame_samples = decoder.decode_frame_simple(frame);
                
                if is_first_frame {
                    // First frame: output all samples
                    chunk_samples.extend_from_slice(&frame_samples);
                    is_first_frame = false;
                    
                    // Initialize overlap buffer with end of first frame
                    let start = frame_samples.len().saturating_sub(OVERLAP_SIZE);
                    decoder.overlap_buffer.copy_from_slice(&frame_samples[start..]);
                } else {
                    // Overlap-add with previous frame
                    for i in 0..OVERLAP_SIZE {
                        chunk_samples.push(decoder.overlap_buffer[i] + frame_samples[i]);
                    }
                    
                    // Add non-overlapping part
                    chunk_samples.extend_from_slice(&frame_samples[OVERLAP_SIZE..FRAME_SIZE - OVERLAP_SIZE]);
                    
                    // Update overlap buffer with end of current frame
                    let start = frame_samples.len().saturating_sub(OVERLAP_SIZE);
                    decoder.overlap_buffer.copy_from_slice(&frame_samples[start..]);
                }
                
                frames_decoded += 1;
                
                // Send chunk when we've decoded enough frames or reached the end
                if frames_decoded % FRAMES_PER_CHUNK == 0 || idx == total_frames - 1 {
                    let progress = (idx as f32 / total_frames as f32) * 100.0;
                    
                    // Apply gapless trimming if this is the last chunk
                    let mut samples_to_send = chunk_samples.clone();
                    let is_last = idx == total_frames - 1;
                    
                    if is_last {
                        // Calculate total samples we should have output
                        let expected_samples = encoded.gapless_info.original_length as usize;
                        let samples_before_this_chunk = total_output_samples;
                        
                        // Trim to exact length
                        if samples_before_this_chunk < expected_samples {
                            let samples_needed = expected_samples - samples_before_this_chunk;
                            samples_to_send.truncate(samples_needed);
                        }
                        
                        println!("Last chunk: trimmed to {} samples", samples_to_send.len());
                    }
                    
                    total_output_samples += samples_to_send.len();
                    
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
                }
            }
            
            println!("Streaming decode complete: output {} total samples", total_output_samples);
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
    
    fn decode_frame_simple(&mut self, frame: &EncodedFrame) -> Vec<f32> {
        let half_size = frame.mdct_coeffs.len();
        
        // Dequantize
        let mdct_coeffs: Vec<f32> = frame.mdct_coeffs.iter()
            .enumerate()
            .map(|(i, &q)| {
                let normalized = q as f32 / ((1 << QUANTIZATION_BITS) - 1) as f32;
                normalized * frame.scale_factor
            })
            .collect();
        
        // IMDCT via IFFT
        let mut complex_spectrum = vec![Complex { re: 0.0, im: 0.0 }; FRAME_SIZE];
        
        // Fill positive frequencies
        for (i, &coeff) in mdct_coeffs.iter().enumerate() {
            if i < half_size {
                complex_spectrum[i] = Complex { re: coeff, im: 0.0 };
                // Mirror for negative frequencies
                if i > 0 && i < half_size {
                    complex_spectrum[FRAME_SIZE - i] = Complex { re: coeff, im: 0.0 };
                }
            }
        }
        
        let ifft = self.fft_planner.plan_fft_inverse(FRAME_SIZE);
        ifft.process(&mut complex_spectrum);
        
        // Take real part and apply window
        let mut samples: Vec<f32> = complex_spectrum.iter()
            .zip(self.window.iter())
            .map(|(c, &w)| c.re * w)
            .collect();
        
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
