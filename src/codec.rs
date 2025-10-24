use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::f32::consts::PI;
use crossbeam_channel::{Sender, Receiver, bounded};
use std::time::Instant;
use std::sync::Arc;

const FRAME_SIZE: usize = 2048;   // 2N
const HOP_SIZE: usize = 1024;     // N
const QUANTIZATION_BITS: u32 = 12;
const FRAMES_PER_CHUNK: usize = 1000;

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
        Encoder { window }
    }

    fn create_window(size: usize) -> Vec<f32> {
        // Princen-Bradley sine window
        (0..size)
            .map(|i| (PI * (i as f32 + 0.5) / size as f32).sin())
            .collect()
    }

    pub fn encode(&mut self, samples: &[f32], sample_rate: u32, channels: u16) -> Result<EncodedAudio> {
        println!("Encoding {} samples at {}Hz, {} channels", samples.len(), sample_rate, channels);
        let total_samples = samples.len() as u64;

        let encoder_delay = (HOP_SIZE / 2) as u32; // Correct delay for sine window MDCT
        let mut padded_samples = vec![0.0; HOP_SIZE / 2];
        padded_samples.extend_from_slice(samples);

        // Pad to full frame boundary
        let remainder = padded_samples.len() % HOP_SIZE;
        if remainder != 0 {
            padded_samples.extend(vec![0.0; HOP_SIZE - remainder]);
        }
        padded_samples.extend(vec![0.0; HOP_SIZE / 2]);

        let padding = (padded_samples.len() - samples.len() - (HOP_SIZE / 2)) as u32;

        let mut frames = Vec::new();
        let mut pos = 0;
        while pos + FRAME_SIZE <= padded_samples.len() {
            frames.push(self.encode_frame(&padded_samples[pos..pos + FRAME_SIZE]));
            pos += HOP_SIZE;
        }

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
        assert_eq!(samples.len(), FRAME_SIZE);
        let windowed: Vec<f32> = samples.iter().zip(&self.window).map(|(s, w)| s * w).collect();

        let n = HOP_SIZE;
        let mut mdct_coeffs = vec![0.0f32; n];
        for k in 0..n {
            let mut sum = 0.0;
            for i in 0..FRAME_SIZE {
                let angle = PI / (n as f32) * (i as f32 + 0.5 + n as f32 / 2.0) * (k as f32 + 0.5);
                sum += windowed[i] * angle.cos();
            }
            mdct_coeffs[k] = sum * (2.0f32 / n as f32).sqrt(); // normalized
        }

        let max_val = mdct_coeffs.iter().map(|x| x.abs()).fold(0.0f32, f32::max).max(1e-10);
        let scale_factor = max_val;

        let quantized: Vec<i16> = mdct_coeffs
            .iter()
            .map(|&c| ((c / scale_factor) * (1 << QUANTIZATION_BITS) as f32)
                .round()
                .clamp(i16::MIN as f32, i16::MAX as f32) as i16)
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
        Decoder {
            window: Encoder::create_window(FRAME_SIZE),
            overlap_buffer: vec![0.0; HOP_SIZE],
        }
    }

    pub fn decode_streaming(
        &mut self,
        encoded: Arc<EncodedAudio>,
        progress_sender: Option<Sender<Progress>>,
    ) -> Receiver<AudioChunk> {
        let (tx, rx) = bounded(5);
        let mut decoder = Decoder::new();

        std::thread::spawn(move || {
            let start_time = Instant::now();
            let total_frames = encoded.frames.len();

            if let Some(ref s) = progress_sender {
                let _ = s.send(Progress::Status(format!("Decoding {} frames...", total_frames)));
            }

            let mut chunk_samples = Vec::with_capacity(FRAMES_PER_CHUNK * HOP_SIZE);

            for (idx, frame) in encoded.frames.iter().enumerate() {
                let frame_output = decoder.decode_frame(frame);

                // Correct overlap-add: sum first half of frame_output with overlap
                for i in 0..HOP_SIZE {
                    chunk_samples.push(decoder.overlap_buffer[i] + frame_output[i]);
                }
                decoder.overlap_buffer.copy_from_slice(&frame_output[HOP_SIZE..]);

                if (idx + 1) % FRAMES_PER_CHUNK == 0 || idx == total_frames - 1 {
                    let is_last = idx == total_frames - 1;
                    if is_last {
                        chunk_samples.extend_from_slice(&decoder.overlap_buffer);
                    }

                    if let Some(ref s) = progress_sender {
                        let _ = s.send(Progress::Decoding((idx + 1) as f32 / total_frames as f32 * 100.0));
                    }

                    let _ = tx.send(AudioChunk {
                        samples: chunk_samples.clone(),
                        is_last,
                    });

                    chunk_samples.clear();
                }
            }

            if let Some(ref s) = progress_sender {
                let _ = s.send(Progress::Complete(format!(
                    "Decoded {} frames in {:.2}s",
                    total_frames,
                    start_time.elapsed().as_secs_f32()
                )));
            }
        });

        rx
    }

    pub fn decode(&mut self, encoded: &EncodedAudio, progress: Option<Sender<Progress>>) -> Result<Vec<f32>> {
        let arc_encoded = Arc::new(encoded.clone());
        let rx = self.decode_streaming(arc_encoded, progress);

        let mut all_samples = Vec::new();
        while let Ok(chunk) = rx.recv() {
            all_samples.extend(chunk.samples);
            if chunk.is_last {
                break;
            }
        }

        // Correct gapless trimming
        let delay = encoded.gapless_info.encoder_delay as usize;
        let original_length = encoded.gapless_info.original_length as usize;
        if all_samples.len() > delay {
            all_samples.drain(0..delay);
        }
        if all_samples.len() > original_length {
            all_samples.truncate(original_length);
        }

        Ok(all_samples)
    }

    fn decode_frame(&self, frame: &EncodedFrame) -> Vec<f32> {
        let n = frame.mdct_coeffs.len();
        let coeffs: Vec<f32> = frame.mdct_coeffs
            .iter()
            .map(|&q| (q as f32 / (1 << QUANTIZATION_BITS) as f32) * frame.scale_factor)
            .collect();

        let mut out = vec![0.0f32; FRAME_SIZE];
        for i in 0..FRAME_SIZE {
            let mut sum = 0.0;
            for k in 0..n {
                let angle = PI / (n as f32) * (i as f32 + 0.5 + n as f32 / 2.0) * (k as f32 + 0.5);
                sum += coeffs[k] * angle.cos();
            }
            out[i] = sum * (2.0 / n as f32).sqrt();
        }

        // Apply synthesis window
        for i in 0..FRAME_SIZE {
            out[i] *= self.window[i];
        }

        out
    }
}

pub fn save_encoded(encoded: &EncodedAudio, path: &std::path::Path) -> Result<()> {
    std::fs::write(path, bincode::serialize(encoded)?)?;
    Ok(())
}

pub fn load_encoded(path: &std::path::Path) -> Result<EncodedAudio> {
    let data = std::fs::read(path)?;
    Ok(bincode::deserialize(&data)?)
}

