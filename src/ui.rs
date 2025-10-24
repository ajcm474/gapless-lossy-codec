use crate::codec::{Encoder, Decoder, EncodedAudio, save_encoded, load_encoded, Progress};
use crate::audio::load_audio_file;
use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use rodio::{Sink, OutputStream, OutputStreamHandle, Source, Decoder as RodioDecoder};
use std::time::{Duration, Instant};
use crossbeam_channel::{bounded, Sender, Receiver};
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use flac_bound::{FlacEncoder, WriteWrapper};
use hound;

pub struct CodecApp 
{
    selected_files: Vec<PathBuf>,
    encoded_files: Vec<(PathBuf, EncodedAudio)>,
    playlist: Vec<PathBuf>,
    status: Arc<Mutex<String>>,
    detailed_status: Arc<Mutex<String>>,
    is_playing: bool,
    is_testing: bool,
    current_track: usize,
    audio_sink: Option<Arc<Mutex<Sink>>>,
    test_sink: Option<Sink>,
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    
    // Progress tracking
    export_progress: Arc<Mutex<Option<f32>>>,
    encoding_progress: Arc<Mutex<Option<f32>>>,
    
    // Channels for background tasks
    progress_receiver: Option<Receiver<Progress>>,
    
    // Audio device testing
    test_file_path: Option<PathBuf>,
    available_devices: Vec<String>,
    selected_device: usize,
}

impl CodecApp 
{
    pub fn new() -> Self 
    {
        let (stream, stream_handle) = OutputStream::try_default().unwrap_or_else(|_| 
        {
            panic!("Failed to get default audio output device");
        });
        
        Self 
        {
            selected_files: Vec::new(),
            encoded_files: Vec::new(),
            playlist: Vec::new(),
            status: Arc::new(Mutex::new("Ready".to_string())),
            detailed_status: Arc::new(Mutex::new(String::new())),
            is_playing: false,
            is_testing: false,
            current_track: 0,
            audio_sink: None,
            test_sink: None,
            _stream: Some(stream),
            stream_handle: Some(stream_handle),
            export_progress: Arc::new(Mutex::new(None)),
            encoding_progress: Arc::new(Mutex::new(None)),
            progress_receiver: None,
            test_file_path: None,
            available_devices: vec!["Default".to_string()],
            selected_device: 0,
        }
    }
    
    fn update_status(&self, msg: String) 
    {
        *self.status.lock().unwrap() = msg;
    }
    
    fn update_detailed_status(&self, msg: String) 
    {
        *self.detailed_status.lock().unwrap() = msg;
    }
    
    fn encode_file_async(&mut self, input_path: PathBuf) 
    {
        let status = self.status.clone();
        let detailed_status = self.detailed_status.clone();
        let encoding_progress = self.encoding_progress.clone();
        
        thread::spawn(move || 
        {
            let start_time = Instant::now();
            *status.lock().unwrap() = format!("Loading: {:?}", input_path.file_name().unwrap());
            *encoding_progress.lock().unwrap() = Some(0.0);
            
            let result = (|| -> anyhow::Result<(PathBuf, EncodedAudio, f32)> 
            {
                let load_start = Instant::now();
                let (samples, sample_rate, channels) = load_audio_file(&input_path)?;
                *detailed_status.lock().unwrap() = format!(
                    "Loaded {} samples in {:.2}s", 
                    samples.len(), 
                    load_start.elapsed().as_secs_f32()
                );
                
                *encoding_progress.lock().unwrap() = Some(50.0);
                *status.lock().unwrap() = format!("Encoding: {:?}", input_path.file_name().unwrap());
                
                let encode_start = Instant::now();
                let mut encoder = Encoder::new();
                let encoded = encoder.encode(&samples, sample_rate, channels)?;
                *detailed_status.lock().unwrap() = format!(
                    "Encoded {} frames in {:.2}s", 
                    encoded.frames.len(), 
                    encode_start.elapsed().as_secs_f32()
                );
                
                *encoding_progress.lock().unwrap() = Some(90.0);
                let output_path = input_path.with_extension("glc");
                save_encoded(&encoded, &output_path)?;
                
                let original_size = std::fs::metadata(&input_path)?.len();
                let encoded_size = std::fs::metadata(&output_path)?.len();
                let ratio = original_size as f32 / encoded_size as f32;
                
                *encoding_progress.lock().unwrap() = Some(100.0);
                
                Ok((output_path, encoded, ratio))
            })();
            
            let total_time = start_time.elapsed();
            match result 
            {
                Ok((output_path, encoded, ratio)) => 
                {
                    *status.lock().unwrap() = format!(
                        "Encoded successfully! Ratio: {:.2}x, Time: {:.2}s", 
                        ratio, 
                        total_time.as_secs_f32()
                    );
                }
                Err(e) => 
                {
                    *status.lock().unwrap() = format!("Encoding error: {}", e);
                }
            }
            
            *encoding_progress.lock().unwrap() = None;
        });
    }
    
    fn play_playlist_async(&mut self) 
    {
        if self.playlist.is_empty() 
        {
            self.update_status("Playlist is empty".to_string());
            return;
        }
        
        // Stop any existing playback first
        self.stop_playback();
        
        let playlist = self.playlist.clone();
        let status = self.status.clone();
        let detailed_status = self.detailed_status.clone();
        let stream_handle = self.stream_handle.as_ref().unwrap().clone();
        
        let sink = match Sink::try_new(&stream_handle) 
        {
            Ok(s) => Arc::new(Mutex::new(s)),
            Err(e) => 
            {
                self.update_status(format!("Failed to create audio sink: {}", e));
                return;
            }
        };
        
        self.audio_sink = Some(sink.clone());
        self.is_playing = true;
        
        let is_playing = Arc::new(Mutex::new(true));  // Add playing flag
        let is_playing_clone = is_playing.clone();
        
        thread::spawn(move || 
        {
            let start_time = Instant::now();
            *status.lock().unwrap() = "Creating audio sink...".to_string();
            
            let mut sample_rate = 44100;
            let mut channels = 2;
            
            // Stream decode and play each track
            for (idx, path) in playlist.iter().enumerate() 
            {
                // Check if we should stop
                if !*is_playing_clone.lock().unwrap() 
                {
                    break;
                }
                
                *status.lock().unwrap() = format!("Loading file {}/{}", idx + 1, playlist.len());
                
                match load_encoded(path) 
                {
                    Ok(encoded) => 
                    {
                        *detailed_status.lock().unwrap() = format!(
                            "Streaming {:?}: {} frames",
                            path.file_name().unwrap(),
                            encoded.frames.len()
                        );
                        
                        sample_rate = encoded.header.sample_rate;
                        channels = encoded.header.channels;
                        let mut decoder = Decoder::new(channels as usize, sample_rate);
                        let arc_encoded = Arc::new(encoded);
                        
                        let (tx, rx) = bounded(10);
                        let chunk_receiver = decoder.decode_streaming(arc_encoded, Some(tx));
                        
                        let mut first_chunk = true;
                        
                        while let Ok(chunk) = chunk_receiver.recv() 
                        {
                            // Check if we should stop
                            if !*is_playing_clone.lock().unwrap() 
                            {
                                break;
                            }
                            
                            while let Ok(progress) = rx.try_recv() 
                            {
                                match progress 
                                {
                                    Progress::Status(msg) => 
                                    {
                                        *detailed_status.lock().unwrap() = msg;
                                    }
                                    Progress::Decoding(p) => 
                                    {
                                        *status.lock().unwrap() = format!(
                                            "Playing track {}/{} ({:.0}%)", 
                                            idx + 1, 
                                            playlist.len(), 
                                            p
                                        );
                                    }
                                    _ => {}
                                }
                            }
                            
                            if first_chunk 
                            {
                                *status.lock().unwrap() = format!("Started playback of track {}/{}", idx + 1, playlist.len());
                                first_chunk = false;
                            }
                            
                            let source = SamplesSource::new(chunk.samples, sample_rate, channels);
                            sink.lock().unwrap().append(source);
                            
                            if chunk.is_last 
                            {
                                break;
                            }
                        }
                    }
                    Err(e) => 
                    {
                        *status.lock().unwrap() = format!("Error loading file: {}", e);
                        return;
                    }
                }
            }
            
            let total_time = start_time.elapsed();
            *status.lock().unwrap() = format!("Playing playlist (prepared in {:.2}s)", total_time.as_secs_f32());
            
            sink.lock().unwrap().sleep_until_end();
            
            *is_playing_clone.lock().unwrap() = false;
            *status.lock().unwrap() = "Playback finished".to_string();
        });
    }

    fn export_playlist_async(&mut self, output_path: PathBuf)
    {
        let playlist = self.playlist.clone();
        let status = self.status.clone();
        let detailed_status = self.detailed_status.clone();
        let export_progress = self.export_progress.clone();

        std::thread::spawn(move ||
            {
                let start_time = Instant::now();
                *export_progress.lock().unwrap() = Some(0.0);
                *status.lock().unwrap() = "Starting export...".to_string();

                // Collect all decoded samples first, then write to FLAC at once
                let mut all_samples: Vec<f32> = Vec::new();
                let mut sample_rate = 0u32;
                let mut channels = 0u16;
                let total_files = playlist.len();

                for (file_idx, path) in playlist.iter().enumerate()
                {
                    let base_progress = (file_idx as f32 / total_files as f32) * 100.0;
                    *export_progress.lock().unwrap() = Some(base_progress);
                    *status.lock().unwrap() = format!("Loading file {}/{}", file_idx + 1, total_files);

                    match crate::codec::load_encoded(path)
                    {
                        Ok(encoded) =>
                            {
                                *detailed_status.lock().unwrap() = format!(
                                    "Processing {:?}: {} frames",
                                    path.file_name().unwrap(),
                                    encoded.frames.len()
                                );

                                // Get sample rate and channels from first file
                                if file_idx == 0
                                {
                                    sample_rate = encoded.header.sample_rate;
                                    channels = encoded.header.channels;
                                }

                                // Create decoder for this file using its native channels & sample_rate
                                let mut decoder = crate::codec::Decoder::new(
                                    encoded.header.channels as usize,
                                    encoded.header.sample_rate,
                                );

                                // Use synchronous decode convenience (it internally uses streaming)
                                match decoder.decode(&encoded, None)
                                {
                                    Ok(samples) =>
                                        {
                                            all_samples.extend_from_slice(&samples);
                                            *status.lock().unwrap() = format!(
                                                "Decoded file {}/{} ({} samples)",
                                                file_idx + 1,
                                                total_files,
                                                samples.len()
                                            );
                                        }
                                    Err(e) =>
                                        {
                                            *status.lock().unwrap() = format!("Decoding error: {}", e);
                                            *export_progress.lock().unwrap() = None;
                                            return;
                                        }
                                }
                            }
                        Err(e) =>
                            {
                                *status.lock().unwrap() = format!("Error loading file: {}", e);
                                *export_progress.lock().unwrap() = None;
                                return;
                            }
                    }
                }

                // Now export all samples to FLAC
                *status.lock().unwrap() = "Writing FLAC file...".to_string();
                *export_progress.lock().unwrap() = Some(95.0);

                match crate::audio::export_to_flac(&output_path, &all_samples, sample_rate, channels)
                {
                    Ok(()) =>
                        {
                            let elapsed = start_time.elapsed();
                            *status.lock().unwrap() = format!(
                                "Exported {} samples to {:?} in {:.2}s",
                                all_samples.len(),
                                output_path.file_name().unwrap(),
                                elapsed.as_secs_f32()
                            );
                        }
                    Err(e) =>
                        {
                            *status.lock().unwrap() = format!("Error exporting FLAC: {}", e);
                            *export_progress.lock().unwrap() = None;
                            return;
                        }
                }

                *export_progress.lock().unwrap() = None;
            });
    }


    
    fn test_audio_device(&mut self) 
    {
        if let Some(ref path) = self.test_file_path.clone() 
        {
            self.stop_test_playback();
            
            if let Some(ref stream_handle) = self.stream_handle 
            {
                match Sink::try_new(stream_handle) 
                {
                    Ok(sink) => 
                    {
                        // Try to play the test file
                        if let Ok(file) = File::open(&path) 
                        {
                            let source = match RodioDecoder::new(BufReader::new(file)) 
                            {
                                Ok(decoder) => decoder,
                                Err(e) => 
                                {
                                    self.update_status(format!("Failed to decode test file: {}", e));
                                    return;
                                }
                            };
                            
                            sink.append(source);
                            self.test_sink = Some(sink);
                            self.is_testing = true;
                            self.update_status(format!("Playing test file: {:?}", path.file_name().unwrap()));
                        } else 
                        {
                            self.update_status("Failed to open test file".to_string());
                        }
                    }
                    Err(e) => 
                    {
                        self.update_status(format!("Failed to create sink: {}", e));
                    }
                }
            }
        }
    }
    
    fn stop_test_playback(&mut self) 
    {
        if let Some(sink) = self.test_sink.take() 
        {
            sink.stop();
        }
        self.is_testing = false;
    }
    
    fn stop_playback(&mut self) 
    {
        if let Some(sink) = self.audio_sink.take() 
        {
            let sink_guard = sink.lock().unwrap();
            sink_guard.stop();
            drop(sink_guard);  // Explicitly drop to ensure cleanup
        }
        self.is_playing = false;
        self.update_status("Stopped".to_string());
    }
}

impl eframe::App for CodecApp 
{
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) 
    {
        // Request repaint for progress updates
        ctx.request_repaint_after(Duration::from_millis(100));
        
        egui::CentralPanel::default().show(ctx, |ui| 
        {
            ui.heading("Gapless Audio Codec");
            
            ui.separator();
            
            // Audio Device Testing Section
            ui.collapsing("Audio Device Testing", |ui| 
            {
                ui.horizontal(|ui| 
                {
                    if ui.button("Select FLAC Test File").clicked() 
                    {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("FLAC files", &["flac"])
                            .pick_file()
                        {
                            self.test_file_path = Some(path);
                        }
                    }
                    
                    if let Some(ref path) = self.test_file_path 
                    {
                        ui.label(format!("Test file: {:?}", path.file_name().unwrap()));
                    }
                });
                
                if self.test_file_path.is_some()
                {
                    ui.horizontal(|ui| 
                    {
                        if !self.is_testing 
                        {
                            if ui.button("▶ Test Audio Output").clicked()
                            {
                                self.test_audio_device();
                            }
                        } else 
                        {
                            if ui.button("⏹ Stop Test").clicked() 
                            {
                                self.stop_test_playback();
                                self.update_status("Test playback stopped".to_string());
                            }
                        }
                    });
                }
            });
            
            ui.separator();
            
            // File selection section
            ui.horizontal(|ui| {
                if ui.button("Select Audio Files (WAV/FLAC)").clicked() 
                {
                    if let Some(paths) = rfd::FileDialog::new()
                        .add_filter("Audio files", &["wav", "flac"])
                        .pick_files()
                    {
                        self.selected_files = paths;
                    }
                }
                
                if !self.selected_files.is_empty() 
                {
                    ui.label(format!("{} files selected", self.selected_files.len()));
                }
            });
            
            // Encode button
            if !self.selected_files.is_empty() 
            {
                ui.horizontal(|ui| 
                {
                    if ui.button("Encode Selected Files").clicked() 
                    {
                        for file in self.selected_files.clone() 
                        {
                            self.encode_file_async(file);
                        }
                    }
                    
                    // Show encoding progress
                    if let Some(progress) = *self.encoding_progress.lock().unwrap() 
                    {
                        ui.add(egui::ProgressBar::new(progress / 100.0)
                            .text(format!("{:.0}%", progress)));
                    }
                });
            }
            
            ui.separator();
            
            // Load encoded files
            if ui.button("Load Encoded Files (.glc)").clicked() 
            {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Encoded files", &["glc"])
                    .pick_files()
                {
                    for path in paths 
                    {
                        if let Ok(encoded) = load_encoded(&path) 
                        {
                            self.encoded_files.push((path, encoded));
                        }
                    }
                }
            }
            
            // Encoded files list - with unique ID
            ui.label("Encoded Files:");
            egui::ScrollArea::vertical()
                .id_source("encoded_files_scroll")
                .max_height(120.0)
                .show(ui, |ui| 
                {
                    let mut files_to_add = Vec::new();
                    for (path, _) in &self.encoded_files 
                    {
                        ui.horizontal(|ui| 
                        {
                            ui.label(format!("{:?}", path.file_name().unwrap()));
                            if ui.button(format!("Add##{:?}", path)).clicked() 
                            {
                                files_to_add.push(path.clone());
                            }
                        });
                    }
                    for path in files_to_add 
                    {
                        self.playlist.push(path);
                    }
                });
            
            ui.separator();
            
            // Playlist section - with unique ID
            ui.label("Playlist (for gapless playback test):");
            egui::ScrollArea::vertical()
                .id_source("playlist_scroll")
                .max_height(120.0)
                .show(ui, |ui| 
                {
                    let mut to_remove = None;
                    for (i, path) in self.playlist.iter().enumerate() 
                    {
                        ui.horizontal(|ui|
                        {
                            ui.label(format!("{}. {:?}", i + 1, path.file_name().unwrap()));
                            if ui.button(format!("Remove##{}", i)).clicked() 
                            {
                                to_remove = Some(i);
                            }
                        });
                    }
                    if let Some(idx) = to_remove 
                    {
                        self.playlist.remove(idx);
                    }
                });
            
            ui.horizontal(|ui| 
            {
                if !self.playlist.is_empty() 
                {
                    if ui.button("Clear Playlist").clicked() 
                    {
                        self.playlist.clear();
                    }
                }
            });
            
            ui.separator();
            
            // Playback controls
            ui.horizontal(|ui| 
            {
                if !self.is_playing 
                {
                    if ui.button("▶ Play Playlist (Gapless)").clicked() 
                    {
                        self.play_playlist_async();
                    }
                } else 
                {
                    if ui.button("⏹ Stop").clicked() 
                    {
                        self.stop_playback();
                    }
                }

                if ui.button("Export Playlist as FLAC").clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("output.flac")
                        .add_filter("FLAC", &["flac"])
                        .save_file()
                    {
                        self.export_playlist_async(path);
                    }
                }
            });
            
            // Export progress bar
            if let Some(progress) = *self.export_progress.lock().unwrap() 
            {
                ui.add(egui::ProgressBar::new(progress / 100.0)
                    .text(format!("Exporting: {:.0}%", progress)));
            }
            
            ui.separator();
            
            // Status bars
            ui.horizontal(|ui| 
            {
                ui.label("Status:");
                ui.label(self.status.lock().unwrap().as_str());
            });
            
            // Detailed status
            let detailed = self.detailed_status.lock().unwrap().clone();
            if !detailed.is_empty() 
            {
                ui.horizontal(|ui| {
                    ui.label("Details:");
                    ui.label(detailed);
                });
            }
        });
    }
}

// Custom audio source for rodio
struct SamplesSource 
{
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    position: usize,
}

impl SamplesSource 
{
    fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self 
    {
        Self 
        {
            samples,
            sample_rate,
            channels,
            position: 0,
        }
    }
}

impl Iterator for SamplesSource 
{
    type Item = f32;
    
    fn next(&mut self) -> Option<Self::Item> 
    {
        if self.position < self.samples.len() 
        {
            let sample = self.samples[self.position];
            self.position += 1;
            Some(sample)
        } else 
        {
            None
        }
    }
}

impl Source for SamplesSource 
{
    fn current_frame_len(&self) -> Option<usize> 
    {
        None
    }
    
    fn channels(&self) -> u16 
    {
        self.channels
    }
    
    fn sample_rate(&self) -> u32 
    {
        self.sample_rate
    }
    
    fn total_duration(&self) -> Option<Duration> 
    {
        Some(Duration::from_secs_f32(
            self.samples.len() as f32 / (self.sample_rate as f32 * self.channels as f32)
        ))
    }
}
