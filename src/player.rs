// src/player.rs
use crate::{
    audio::AudioPlayer,
    decoder::MediaDecoder,
    error::{PlayerError, Result},
    renderer::VideoWindow,
};
use crossbeam_channel::unbounded;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use serde_json;
use log::{debug, error, info, warn};

pub struct MediaPlayer {
    video_window: Option<VideoWindow>,
    audio_player: Option<AudioPlayer>,
    decoder: Option<MediaDecoder>,
    is_running: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    current_url: Option<String>,
}

impl MediaPlayer {
    pub fn new() -> Self {
        Self {
            video_window: None,
            audio_player: None,
            decoder: None,
            is_running: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
            current_url: None,
        }
    }

    pub fn load_url(&mut self, url: &str) -> Result<()> {
        println!("🔄 Loading URL: {}", url);
        self.current_url = Some(url.to_string());

        // Extract video and audio URLs using yt-dlp
        let (video_url, audio_url) = self.extract_stream_urls(url)?;
        
        println!("✅ Got video stream URL");
        println!("✅ Got audio stream URL");

        // Create communication channels
        let (video_sender, video_receiver) = unbounded();
        let (audio_sender, audio_receiver) = unbounded();

        // Initialize video renderer
        self.video_window = Some(VideoWindow::new(
            video_receiver,
            1024,
            768,
            "Rust Video Streamer",
        )?);
        println!("✅ Video renderer initialized");

        // Initialize audio player
        let mut audio_player = AudioPlayer::new(audio_receiver)?;
        audio_player.start_playback()?;
        self.audio_player = Some(audio_player);
        println!("✅ Audio player initialized");

        // Initialize decoder
        let decoder = MediaDecoder::new(Some(video_sender), Some(audio_sender));
        self.decoder = Some(decoder);
        
        let video_url_for_thread = video_url.clone();
        let audio_url_for_thread = audio_url.clone();

        if let Some(main_decoder) = &self.decoder {
            let should_stop_clone = main_decoder.should_stop.clone();
            std::thread::spawn(move || {
                let mut thread_decoder = MediaDecoder::new(None, None); // This decoder is only for calling decode_streams
                thread_decoder.should_stop = should_stop_clone; // Transfer ownership of the Arc
                if let Err(e) = thread_decoder.decode_streams(&video_url_for_thread, &audio_url_for_thread) {
                    log::error!("Decoding error: {:?}", e);
                }
            });
        }
        println!("✅ Media decoder started");

        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        if self.video_window.is_none() {
            return Err(PlayerError::Video("No media loaded".to_string()));
        }

        self.is_running.store(true, Ordering::SeqCst);
        println!("🎬 Starting playback...");
        self.resume();

        let mut frame_count = 0;
        let start_time = std::time::Instant::now();

        'running: loop {
            debug!("MediaPlayer loop running...");
            if !self.is_running.load(Ordering::SeqCst) {
                break;
            }

            // Handle SDL events
            let mut keycode = None;
            if let Some(ref mut video_window) = self.video_window {
                for event in video_window.event_pump.poll_iter() {
                    match event {
                        Event::Quit { .. } => {
                            println!("👋 Quit requested");
                            self.is_running.store(false, Ordering::SeqCst);
                        }
                        Event::KeyDown {
                            keycode: Some(k),
                            ..
                        } => {
                           keycode = Some(k);
                        }
                        Event::Window { win_event, .. } => {
                            match win_event {
                                sdl2::event::WindowEvent::Resized(_, _) => {
                                    video_window.renderer.handle_window_resize()?;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Some(keycode) = keycode {
                self.handle_keypress(keycode);
            }

            // Update video renderer
            if let Some(ref mut video_window) = self.video_window {
                let frame_updated = video_window.renderer.update()?;
                if frame_updated {
                    frame_count += 1;

                    // Print stats every 100 frames
                    if frame_count % 100 == 0 {
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let fps = frame_count as f64 / elapsed;
                        
                        if let Some((width, height, timestamp, frame_num)) =
                            video_window.renderer.get_current_frame_info() {
                            println!(
                                "📊 Frame: {} | {}x{} | Time: {:.2}s | FPS: {:.1}",
                                frame_num, width, height, timestamp, fps
                            );
                        }
                    }
                }
            }

            // Small delay to prevent excessive CPU usage
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        self.stop();
        Ok(())
    }

    fn handle_keypress(&mut self, keycode: Keycode) {
        let is_paused = self.is_paused();
        match keycode {
             Keycode::Space => {
                 if is_paused {
                     println!("▶ Resuming playback");
                     self.resume();
                 } else {
                     println!("⏸ Pausing playback");
                     self.pause();
                 }
             }
             Keycode::Q | Keycode::Escape => {
                 println!("👋 Quit requested");
                 self.is_running.store(false, Ordering::SeqCst);
             }
             Keycode::Up => {
                 // Volume up
                 if let Some(ref audio_player) = self.audio_player {
                     let current_vol = audio_player.get_volume();
                     let new_vol = (current_vol + 10).min(100);
                     audio_player.set_volume(new_vol);
                     println!("🔊 Volume: {}%", new_vol);
                 }
             }
             Keycode::Down => {
                 // Volume down
                 if let Some(ref audio_player) = self.audio_player {
                     let current_vol = audio_player.get_volume();
                     let new_vol = current_vol.saturating_sub(10);
                     audio_player.set_volume(new_vol);
                     println!("🔉 Volume: {}%", new_vol);
                 }
             }
             Keycode::M => {
                 // Mute/unmute
                 if let Some(ref audio_player) = self.audio_player {
                     let current_vol = audio_player.get_volume();
                     if current_vol > 0 {
                         audio_player.set_volume(0);
                         println!("🔇 Muted");
                     } else {
                         audio_player.set_volume(50);
                         println!("🔊 Unmuted (50%)");
                     }
                 }
             }
             _ => {}
        }
    }

    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::SeqCst);
        if let Some(ref audio_player) = self.audio_player {
            audio_player.pause();
        }
    }

    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::SeqCst);
        if let Some(ref audio_player) = self.audio_player {
            audio_player.resume();
        }
    }

    pub fn stop(&self) {
        println!("⏹ Stopping playback...");
        self.is_running.store(false, Ordering::SeqCst);
        
        if let Some(ref audio_player) = self.audio_player {
            audio_player.stop();
        }
        
        if let Some(ref decoder) = self.decoder {
            decoder.stop();
        }
        
        if let Some(ref video_window) = self.video_window {
            video_window.renderer.stop();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::SeqCst)
    }

    pub fn is_playing(&self) -> bool {
        self.is_running.load(Ordering::SeqCst) && !self.is_paused()
    }

    fn extract_stream_urls(&self, url: &str) -> Result<(String, String)> {
        println!("🔍 Extracting stream URLs with yt-dlp...");

        let output = Command::new("yt-dlp")
            .args(["-j", "-f", "bestvideo[ext=mp4]/bestvideo[ext=webm]+bestaudio[ext=m4a]/bestaudio[ext=opus]/bestaudio[ext=aac]/bestaudio/best[height<=1080][ext!=webm][ext!=mhtml]/best[height<=720][ext!=webm][ext!=mhtml]/best[ext!=webm][ext!=mhtml]", url])
            .output()
            .map_err(PlayerError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlayerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("yt-dlp failed: {}", stderr),
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| PlayerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to parse yt-dlp JSON output: {}", e),
            )))?;

        let formats = json["formats"].as_array().ok_or_else(|| PlayerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No formats found in yt-dlp output",
        )))?;

        let get_best_url = |formats: &Vec<serde_json::Value>, is_video: bool| -> Option<String> {
            let mut best_format_url: Option<String> = None;
            let mut best_quality_score: u64 = 0;
            let mut fallback_url: Option<String> = None; // Added for fallback

            for format in formats {
                let url = format["url"].as_str();
                if url.is_none() {
                    debug!("Skipping format with no URL: {:?}", format);
                    continue;
                }

                let current_url = url.unwrap().to_string();
                let ext = format["ext"].as_str().unwrap_or("");
                let protocol = format["protocol"].as_str().unwrap_or("");
                let format_id = format["format_id"].as_str().unwrap_or("");

                // Skip formats that are not direct URLs or are HLS/DASH manifests
                if protocol.contains("m3u8") || protocol.contains("dash") || ext == "mpd" || format_id.contains("hls") || format_id.contains("dash") {
                    debug!("Skipping manifest/non-direct format: {:?}", format);
                    continue;
                }
                
                // Keep track of any valid direct URL for fallback
                if fallback_url.is_none() {
                    fallback_url = Some(current_url.clone());
                }

                if is_video {
                    let vcodec = format["vcodec"].as_str().unwrap_or("none");
                    let height = format["height"].as_u64().unwrap_or(0);
                    let filesize = format["filesize"].as_u64().unwrap_or(0);

                    // Prioritize formats with actual video codecs and higher resolution
                    let mut quality_score = height;
                    if vcodec != "none" && vcodec != "true" { // 'true' can sometimes be a placeholder
                        quality_score += 10000; // Boost score for actual video streams
                    }
                    if ext == "mp4" {
                        quality_score += 5000; // Prefer mp4
                    }
                    if filesize > 0 { // Prefer formats with known file size
                        quality_score += 1000;
                    }

                    debug!("Video format considered: {:?} with score {}", format, quality_score);

                    if quality_score > best_quality_score {
                        best_quality_score = quality_score;
                        best_format_url = Some(current_url.clone());
                        debug!("Found better video format: {:?} with score {}", format, quality_score);
                    }
                } else { // Audio
                    let acodec = format["acodec"].as_str().unwrap_or("none");
                    let abr = format["abr"].as_f64().unwrap_or(0.0) as u64; // Average Bitrate
                    let filesize = format["filesize"].as_u64().unwrap_or(0);

                    // Prioritize formats with actual audio codecs and higher bitrate
                    let mut quality_score = abr;
                    if acodec != "none" && acodec != "true" {
                        quality_score += 10000; // Boost score for actual audio streams
                    }
                    if ext == "m4a" || ext == "webm" { // Prefer m4a or webm for audio
                        quality_score += 5000;
                    }
                     if filesize > 0 { // Prefer formats with known file size
                        quality_score += 1000;
                    }

                    debug!("Audio format considered: {:?} with score {}", format, quality_score);

                    if quality_score > best_quality_score {
                        best_quality_score = quality_score;
                        best_format_url = Some(current_url.clone());
                        debug!("Found better audio format: {:?} with score {}", format, quality_score);
                    }
                }
            }
            best_format_url.or(fallback_url) // Use fallback if no best format found
        };

        let video_url = get_best_url(formats, true).ok_or_else(|| PlayerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No suitable video URL found in yt-dlp output",
        )))?;

        let audio_url = get_best_url(formats, false).ok_or_else(|| PlayerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No suitable audio URL found in yt-dlp output",
        )))?;

        Ok((video_url, audio_url))
    }

    pub fn get_status(&self) -> PlayerStatus {
        PlayerStatus {
            is_playing: self.is_playing(),
            is_paused: self.is_paused(),
            current_url: self.current_url.clone(),
            audio_info: self.audio_player.as_ref()
                .and_then(|ap| ap.get_current_sample_info()),
            video_info: self.video_window.as_ref()
                .and_then(|vw| vw.renderer.get_current_frame_info()),
        }
    }
}

impl Drop for MediaPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug)]
pub struct PlayerStatus {
    pub is_playing: bool,
    pub is_paused: bool,
    pub current_url: Option<String>,
    pub audio_info: Option<(u32, u16, f64)>, // sample_rate, channels, timestamp
    pub video_info: Option<(u32, u32, f64, u64)>, // width, height, timestamp, frame_number
}

impl Default for MediaPlayer {
    fn default() -> Self {
        Self::new()
    }
}