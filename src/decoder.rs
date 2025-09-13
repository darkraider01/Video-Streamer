// src/decoder.rs - FIXED VERSION
use crate::error::{PlayerError, Result};
use ffmpeg_next::{
    codec, format, frame::{Audio, Video},
    media::Type,
    software::{resampling, scaling::{self, flag::Flags}},
    util::format::{pixel::Pixel, sample::Sample},
};
use crossbeam_channel::Sender;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use log::{error, info, warn};

#[derive(Clone)]
pub struct VideoFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub timestamp: f64,
    pub frame_number: u64,
}

#[derive(Clone)]
pub struct AudioSample {
    pub data: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub timestamp: f64,
}

pub struct MediaDecoder {
    video_sender: Option<Sender<VideoFrame>>,
    audio_sender: Option<Sender<AudioSample>>,
    pub should_stop: Arc<AtomicBool>,
}

impl MediaDecoder {
    pub fn new(
        video_sender: Option<Sender<VideoFrame>>,
        audio_sender: Option<Sender<AudioSample>>,
    ) -> Self {
        Self {
            video_sender,
            audio_sender,
            should_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }

    pub fn decode_streams(&self, video_path: &str, audio_path: &str) -> Result<()> {
        ffmpeg_next::init()?;
        info!("Starting decode_streams for video: {}, audio: {}", video_path, audio_path);

        let video_handle = if let Some(sender) = &self.video_sender {
            let sender = sender.clone();
            let video_path = video_path.to_string();
            let should_stop = self.should_stop.clone();
            Some(std::thread::spawn(move || {
                Self::decode_video_stream(&video_path, sender, should_stop)
            }))
        } else {
            None
        };

        let audio_handle = if let Some(sender) = &self.audio_sender {
            let sender = sender.clone();
            let audio_path = audio_path.to_string();
            let should_stop = self.should_stop.clone();
            Some(std::thread::spawn(move || {
                Self::decode_audio_stream(&audio_path, sender, should_stop)
            }))
        } else {
            None
        };

        // Wait for both threads to complete
        if let Some(handle) = video_handle {
            if let Err(e) = handle.join().unwrap() {
                error!("Video decoding error: {:?}", e);
            }
        }

        if let Some(handle) = audio_handle {
            if let Err(e) = handle.join().unwrap() {
                error!("Audio decoding error: {:?}", e);
            }
        }

        Ok(())
    }

    fn decode_video_stream(
        video_path: &str,
        sender: Sender<VideoFrame>,
        should_stop: Arc<AtomicBool>,
    ) -> Result<()> {
        info!("Starting video decoding for: {}", video_path);
        
        let mut input_ctx = format::input(&video_path)
            .map_err(|e| {
                error!("Failed to open video input '{}': {:?}", video_path, e);
                PlayerError::FFmpeg(e)
            })?;

        let input_stream = input_ctx
            .streams()
            .best(Type::Video)
            .ok_or_else(|| {
                error!("Could not find video stream in {}", video_path);
                PlayerError::Video("Could not find video stream".to_string())
            })?;
        
        let video_stream_index = input_stream.index();
        let context = codec::context::Context::from_parameters(input_stream.parameters())?;
        let mut decoder = context.decoder().video()?;
        
        info!("Video decoder initialized: {}x{} @ {} fps", 
              decoder.width(), decoder.height(), 
              input_stream.avg_frame_rate().numerator() as f64 / input_stream.avg_frame_rate().denominator() as f64);

        // Create scaling context for YUV to RGB conversion
        let mut scaler = scaling::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::RGB24,
            decoder.width(),
            decoder.height(),
            Flags::BILINEAR,
        )?;

        let mut frame_number = 0u64;
        let time_base = input_stream.time_base();
        let mut decoded_frame = Video::empty();
        let mut rgb_frame = Video::empty();
        
        // Frame timing for proper playback speed
        let target_fps = 30.0; // Target 30 FPS
        let frame_duration = Duration::from_secs_f64(1.0 / target_fps);
        let mut last_frame_time = Instant::now();

        info!("Starting video packet processing...");

        for (stream, packet) in input_ctx.packets() {
            if should_stop.load(Ordering::SeqCst) {
                info!("Video decoding stopped by request");
                break;
            }

            if stream.index() == video_stream_index {
                if let Err(e) = decoder.send_packet(&packet) {
                    warn!("Failed to send video packet: {:?}", e);
                    continue;
                }
                
                while decoder.receive_frame(&mut decoded_frame).is_ok() {
                    if should_stop.load(Ordering::SeqCst) {
                        break;
                    }

                    // Convert YUV frame to RGB
                    if let Err(e) = scaler.run(&decoded_frame, &mut rgb_frame) {
                        warn!("Failed to scale video frame: {:?}", e);
                        continue;
                    }
                    
                    // Calculate timestamp
                    let timestamp = if decoded_frame.pts().is_some() {
                        decoded_frame.pts().unwrap() as f64 * f64::from(time_base)
                    } else {
                        frame_number as f64 / target_fps
                    };

                    // Copy RGB data
                    let rgb_data = rgb_frame.data(0).to_vec();
                    
                    let video_frame = VideoFrame {
                        data: rgb_data,
                        width: rgb_frame.width(),
                        height: rgb_frame.height(),
                        timestamp,
                        frame_number,
                    };

                    // Send frame
                    if sender.send(video_frame).is_err() {
                        info!("Video receiver disconnected, stopping video decoding");
                        return Ok(());
                    }

                    frame_number += 1;

                    // Frame rate limiting with proper timing
                    let elapsed = last_frame_time.elapsed();
                    if elapsed < frame_duration {
                        let sleep_duration = frame_duration - elapsed;
                        std::thread::sleep(sleep_duration);
                    }
                    last_frame_time = Instant::now();

                    // Progress logging every 300 frames (10 seconds at 30fps)
                    if frame_number % 300 == 0 {
                        info!("Decoded {} video frames, timestamp: {:.2}s", frame_number, timestamp);
                    }
                }
            }
        }

        // Flush decoder
        if let Err(e) = decoder.send_eof() {
            warn!("Failed to send EOF to video decoder: {:?}", e);
        } else {
            while decoder.receive_frame(&mut decoded_frame).is_ok() {
                if let Err(_) = scaler.run(&decoded_frame, &mut rgb_frame) {
                    continue;
                }
                
                let timestamp = if decoded_frame.pts().is_some() {
                    decoded_frame.pts().unwrap() as f64 * f64::from(time_base)
                } else {
                    frame_number as f64 / target_fps
                };

                let rgb_data = rgb_frame.data(0).to_vec();
                
                let video_frame = VideoFrame {
                    data: rgb_data,
                    width: rgb_frame.width(),
                    height: rgb_frame.height(),
                    timestamp,
                    frame_number,
                };

                if sender.send(video_frame).is_err() {
                    break;
                }
                frame_number += 1;
            }
        }

        info!("✅ Video decoding completed. Total frames: {}", frame_number);
        Ok(())
    }

    fn decode_audio_stream(
        audio_path: &str,
        sender: Sender<AudioSample>,
        should_stop: Arc<AtomicBool>,
    ) -> Result<()> {
        info!("Starting audio decoding for: {}", audio_path);
        
        let mut input_ctx = format::input(&audio_path)
            .map_err(|e| {
                error!("Failed to open audio input '{}': {:?}", audio_path, e);
                PlayerError::FFmpeg(e)
            })?;

        let input_stream = input_ctx
            .streams()
            .best(Type::Audio)
            .ok_or(PlayerError::Audio("Could not find audio stream".to_string()))?;
        
        let audio_stream_index = input_stream.index();
        let context = codec::context::Context::from_parameters(input_stream.parameters())?;
        let mut decoder = context.decoder().audio()?;

        info!("Audio decoder initialized: {} Hz, {} channels", 
              decoder.rate(), decoder.channels());

        // Create resampler to convert to f32 samples
        let mut resampler = resampling::Context::get(
            decoder.format(),
            decoder.channel_layout(),
            decoder.rate(),
            Sample::F32(ffmpeg_next::util::format::sample::Type::Packed),
            decoder.channel_layout(),
            decoder.rate(),
        )?;

        let time_base = input_stream.time_base();
        let mut decoded_audio = Audio::empty();
        let mut resampled_audio = Audio::empty();
        let sample_rate = decoder.rate();
        let channels = decoder.channels();
        let mut sample_count = 0usize;

        info!("Starting audio packet processing...");

        for (_stream, packet) in input_ctx.packets() {
            if should_stop.load(Ordering::SeqCst) {
                info!("Audio decoding stopped by request");
                break;
            }

            if packet.stream() == audio_stream_index {
                if let Err(e) = decoder.send_packet(&packet) {
                    warn!("Failed to send audio packet: {:?}", e);
                    continue;
                }
                
                while decoder.receive_frame(&mut decoded_audio).is_ok() {
                    if should_stop.load(Ordering::SeqCst) {
                        break;
                    }

                    // Resample audio to f32
                    if let Err(e) = resampler.run(&decoded_audio, &mut resampled_audio) {
                        warn!("Failed to resample audio: {:?}", e);
                        continue;
                    }
                    
                    // Calculate timestamp
                    let timestamp = if decoded_audio.pts().is_some() {
                        decoded_audio.pts().unwrap() as f64 * f64::from(time_base)
                    } else {
                        sample_count as f64 / sample_rate as f64
                    };

                    // Convert audio data to Vec<f32>
                    let audio_data = Self::extract_f32_samples(&resampled_audio);
                    sample_count += audio_data.len();
                    
                    let audio_sample = AudioSample {
                        data: audio_data,
                        sample_rate,
                        channels,
                        timestamp,
                    };

                    if sender.send(audio_sample).is_err() {
                        info!("Audio receiver disconnected");
                        return Ok(());
                    }
                }
            }
        }

        // Flush decoder
        if let Err(e) = decoder.send_eof() {
            warn!("Failed to send EOF to audio decoder: {:?}", e);
        } else {
            while decoder.receive_frame(&mut decoded_audio).is_ok() {
                if let Err(_) = resampler.run(&decoded_audio, &mut resampled_audio) {
                    continue;
                }
                
                let timestamp = if decoded_audio.pts().is_some() {
                    decoded_audio.pts().unwrap() as f64 * f64::from(time_base)
                } else {
                    sample_count as f64 / sample_rate as f64
                };

                let audio_data = Self::extract_f32_samples(&resampled_audio);
                sample_count += audio_data.len();
                
                let audio_sample = AudioSample {
                    data: audio_data,
                    sample_rate,
                    channels,
                    timestamp,
                };

                if sender.send(audio_sample).is_err() {
                    break;
                }
            }
        }

        info!("✅ Audio decoding completed. Total samples: {}", sample_count);
        Ok(())
    }

    fn extract_f32_samples(frame: &Audio) -> Vec<f32> {
        let data = frame.data(0);
        let sample_count = data.len() / 4; // f32 is 4 bytes
        let mut samples = Vec::with_capacity(sample_count);
        
        for i in 0..sample_count {
            let bytes = [
                data[i * 4],
                data[i * 4 + 1],
                data[i * 4 + 2],
                data[i * 4 + 3],
            ];
            let sample = f32::from_le_bytes(bytes);
            samples.push(sample);
        }
        
        samples
    }
}

// Legacy function for backwards compatibility
pub fn decode_combined_streams(video_path: &str, audio_path: &str) -> Result<()> {
    let decoder = MediaDecoder::new(None, None);
    decoder.decode_streams(video_path, audio_path)
}