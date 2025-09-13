// src/decoder.rs
use crate::error::{PlayerError, Result};
use ffmpeg_next::{
    codec, format, frame::{Audio, Video},
    media::Type,
    software::{resampling, scaling::{self, flag::Flags}},
    util::format::{pixel::Pixel, sample::Sample},
};
use crossbeam_channel::Sender;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

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
                log::error!("Video decoding error: {:?}", e);
            }
        }

        if let Some(handle) = audio_handle {
            if let Err(e) = handle.join().unwrap() {
                log::error!("Audio decoding error: {:?}", e);
            }
        }

        Ok(())
    }

    fn decode_video_stream(
        video_path: &str,
        sender: Sender<VideoFrame>,
        should_stop: Arc<AtomicBool>,
    ) -> Result<()> {
        log::debug!("Attempting to decode video stream for path: {}", video_path);
        ffmpeg_next::init()?;
        log::debug!("ffmpeg_next::init() successful in decode_video_stream.");

        let mut input_ctx = format::input(&video_path).map_err(|e| {
            log::error!("Failed to open input context for video path '{}': {:?}", video_path, e);
            PlayerError::FFmpeg(e)
        })?;
        log::debug!("Input context successfully opened for video path: {}", video_path);

        let input_stream = input_ctx
            .streams()
            .best(Type::Video)
            .ok_or_else(|| {
                log::error!("Could not find video stream in {}", video_path);
                PlayerError::Video("Could not find video stream".to_string())
            })?;
        log::debug!("Video stream found at index: {}", input_stream.index());
        log::debug!("Starting to process packets for video stream...");
        
        let video_stream_index = input_stream.index();
        let context = codec::context::Context::from_parameters(input_stream.parameters())?;
        let mut decoder = context.decoder().video()?;
        log::debug!("Video decoder initialized: {}x{}", decoder.width(), decoder.height());

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
        log::debug!("Scaler initialized.");

        let mut frame_number = 0u64;
        let time_base = input_stream.time_base();
        let mut decoded_frame = Video::empty();
        let mut rgb_frame = Video::empty();
        log::debug!("Starting video packet processing loop.");

        for (stream, packet) in input_ctx.packets() {
            log::debug!("Video stream: Received packet (PTS: {:?}, DTS: {:?})", packet.pts(), packet.dts());
            if should_stop.load(Ordering::SeqCst) {
                log::debug!("Video decoding stopped by request.");
                break;
            }

            if stream.index() == video_stream_index {
                log::debug!("Sending packet to video decoder (PTS: {:?})", packet.pts());
                if let Err(e) = decoder.send_packet(&packet) {
                    log::error!("Failed to send video packet: {:?}", e);
                    continue; // Continue to next packet or handle error
                }
                
                while decoder.receive_frame(&mut decoded_frame).is_ok() {
                    log::debug!("Received frame from video decoder (PTS: {:?})", decoded_frame.pts());
                    if should_stop.load(Ordering::SeqCst) {
                        log::debug!("Video decoding stopped by request during frame reception.");
                        break;
                    }

                    // Convert YUV frame to RGB
                    if let Err(e) = scaler.run(&decoded_frame, &mut rgb_frame) {
                        log::error!("Failed to scale video frame: {:?}", e);
                        continue; // Skip this frame and try next
                    }
                    
                    // Calculate timestamp
                    let timestamp = if decoded_frame.pts().is_some() {
                        decoded_frame.pts().unwrap() as f64 * f64::from(time_base)
                    } else {
                        frame_number as f64 / 30.0 // Fallback to 30fps assumption
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
                    log::debug!("Decoded video frame {} ({}x{}) at timestamp {}", frame_number, rgb_frame.width(), rgb_frame.height(), timestamp);

                    if sender.send(video_frame).is_err() {
                        log::warn!("Video receiver disconnected, stopping video decoding.");
                        return Ok(()); // Receiver disconnected, stop decoding
                    }

                    frame_number += 1;

                    // Simple frame rate limiting (30 FPS)
                    // This sleep might be too long and cause issues if frames are not coming fast enough
                    std::thread::sleep(std::time::Duration::from_millis(1)); // Reduced sleep for testing
                }
            }
        }

        // Flush decoder
        decoder.send_eof()?;
        while decoder.receive_frame(&mut decoded_frame).is_ok() {
            scaler.run(&decoded_frame, &mut rgb_frame)?;
            
            let timestamp = if decoded_frame.pts().is_some() {
                decoded_frame.pts().unwrap() as f64 * f64::from(time_base)
            } else {
                frame_number as f64 / 30.0
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

        println!("✅ Video decoding completed. Total frames: {}", frame_number);
        Ok(())
    }

    fn decode_audio_stream(
        audio_path: &str,
        sender: Sender<AudioSample>,
        should_stop: Arc<AtomicBool>,
    ) -> Result<()> {
        log::debug!("Attempting to decode audio stream for path: {}", audio_path);
        ffmpeg_next::init()?;
        log::debug!("ffmpeg_next::init() successful in decode_audio_stream.");
        let mut input_ctx = format::input(&audio_path).map_err(|e| {
            log::error!("Failed to open input context for audio path '{}': {:?}", audio_path, e);
            PlayerError::FFmpeg(e)
        })?;
        log::debug!("Input context successfully opened for audio path: {}", audio_path);

        let input_stream = input_ctx
            .streams()
            .best(Type::Audio)
            .ok_or(PlayerError::Audio("Could not find audio stream".to_string()))?;
        
        let audio_stream_index = input_stream.index();
        let context = codec::context::Context::from_parameters(input_stream.parameters())?;
        let mut decoder = context.decoder().audio()?;

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

        for (_stream, packet) in input_ctx.packets() {
            log::debug!("Audio stream: Received packet (PTS: {:?}, DTS: {:?})", packet.pts(), packet.dts());
            if should_stop.load(Ordering::SeqCst) {
                break;
            }

            if packet.stream() == audio_stream_index {
                decoder.send_packet(&packet)?;
                
                while decoder.receive_frame(&mut decoded_audio).is_ok() {
                    if should_stop.load(Ordering::SeqCst) {
                        break;
                    }

                    // Resample audio to f32
                    resampler.run(&decoded_audio, &mut resampled_audio)?;
                    
                    // Calculate timestamp
                    let timestamp = if decoded_audio.pts().is_some() {
                        decoded_audio.pts().unwrap() as f64 * f64::from(time_base)
                    } else {
                        0.0
                    };

                    // Convert audio data to Vec<f32>
                    let audio_data = Self::extract_f32_samples(&resampled_audio);
                    
                    let audio_sample = AudioSample {
                        data: audio_data,
                        sample_rate,
                        channels,
                        timestamp,
                    };

                    if sender.send(audio_sample).is_err() {
                        log::warn!("Audio receiver disconnected");
                        return Ok(());
                    }
                }
            }
        }

        // Flush decoder
        decoder.send_eof()?;
        while decoder.receive_frame(&mut decoded_audio).is_ok() {
            resampler.run(&decoded_audio, &mut resampled_audio)?;
            
            let timestamp = if decoded_audio.pts().is_some() {
                decoded_audio.pts().unwrap() as f64 * f64::from(time_base)
            } else {
                0.0
            };

            let audio_data = Self::extract_f32_samples(&resampled_audio);
            
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

        println!("✅ Audio decoding completed");
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