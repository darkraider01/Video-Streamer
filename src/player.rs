// src/player.rs - FIXED VERSION
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
use log::{error, info, warn};

pub struct MediaPlayer {
    video_window: Option<VideoWindow>,
    audio_player: Option<AudioPlayer>,
    decoder: Option<Arc<MediaDecoder>>,  // Use Arc for shared ownership
    decode_thread: Option<std::thread::JoinHandle<()>>,  // Track decode thread
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
            decode_thread: None,
            is_running: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
            current_url: None,
        }
    }

    pub fn load_url(&mut self, url: &str) -> Result<()> {
        info!("🔄 Loading URL: {}", url);
        self.current_url = Some(url.to_string());

        // Stop any existing playback
        self.stop_internal();

        // Extract video and audio URLs using yt-dlp
        let (video_url, audio_url) = self.extract_stream_urls(url)?;
        
        info!("✅ Got stream URLs - Video: {}..., Audio: {}...", 
              &video_url[..std::cmp::min(50, video_url.len())],
              &audio_url[..std::cmp::min(50, audio_url.len())]);

        // Create communication channels with larger capacity to prevent blocking
        let (video_sender, video_receiver) = unbounded();
        let (audio_sender, audio_receiver) = unbounded();

        // Initialize video renderer
        self.video_window = Some(VideoWindow::new(
            video_receiver,
            1280,  // Increased default size
            720,
            "Rust Video Streamer",
        )?);
        info!("✅ Video renderer initialized");

        // Initialize audio player
        let audio_player = AudioPlayer::new(audio_receiver)?;
        audio_player.start_playback()?;
        self.audio_player = Some(audio_player);
        info!("✅ Audio player initialized");

        // Initialize decoder with Arc for shared ownership
        let decoder = Arc::new(MediaDecoder::new(Some(video_sender), Some(audio_sender)));
        let decoder_clone = decoder.clone();
        
        // Start decoding in background thread
        let video_url_clone = video_url.clone();
        let audio_url_clone = audio_url.clone();
        
        let decode_handle = std::thread::spawn(move || {
            info!("🎬 Starting decoder thread");
            if let Err(e) = decoder_clone.decode_streams(&video_url_clone, &audio_url_clone) {
                error!("Decoding error: {:?}", e);
            } else {
                info!("🎬 Decoder thread completed successfully");
            }
        });
        
        self.decoder = Some(decoder);
        self.decode_thread = Some(decode_handle);
        info!("✅ Media decoder started");

        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        if self.video_window.is_none() {
            return Err(PlayerError::Video("No media loaded. Call load_url() first".to_string()));
        }

        self.is_running.store(true, Ordering::SeqCst);
        info!("🎬 Starting playback loop...");
        self.resume();

        let mut frame_count = 0;
        let start_time = std::time::Instant::now();
        let mut last_stats_time = start_time;

        'running: loop {
            if !self.is_running.load(Ordering::SeqCst) {
                info!("Main loop stopping...");
                break;
            }

            let mut events_to_process = vec![];
            if let Some(ref mut video_window) = self.video_window {
                for event in video_window.event_pump.poll_iter() {
                    events_to_process.push(event);
                }
            }

            for event in events_to_process {
                match event {
                    Event::Quit { .. } => {
                        info!("👋 Quit requested via window close");
                        self.is_running.store(false, Ordering::SeqCst);
                        break 'running;
                    }
                    Event::KeyDown {
                        keycode: Some(keycode),
                        ..
                    } => {
                        if self.handle_keypress(keycode) {
                            break 'running;
                        }
                    }
                    Event::Window { win_event, .. } => {
                        if let Some(ref mut video_window) = self.video_window {
                            match win_event {
                                sdl2::event::WindowEvent::Resized(_, _) => {
                                    if let Err(e) = video_window.renderer.handle_window_resize() {
                                        warn!("Failed to handle window resize: {:?}", e);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }

            if let Some(ref mut video_window) = self.video_window {
                // Update video renderer
                match video_window.renderer.update() {
                    Ok(frame_updated) => {
                        if frame_updated {
                            frame_count += 1;

                            // Print stats every 10 seconds
                            let now = std::time::Instant::now();
                            if now.duration_since(last_stats_time).as_secs() >= 10 {
                                let elapsed = start_time.elapsed().as_secs_f64();
                                let fps = frame_count as f64 / elapsed;
                                
                                if let Some((width, height, timestamp, frame_num)) =
                                    video_window.renderer.get_current_frame_info() {
                                    info!(
                                        "📊 Stats - Frame: {} | {}x{} | Time: {:.1}s | Avg FPS: {:.1}",
                                        frame_num, width, height, timestamp, fps
                                    );
                                }
                                last_stats_time = now;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Video renderer error: {:?}", e);
                        // Continue running despite renderer errors
                    }
                }
            }

            // Prevent excessive CPU usage
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        info!("🛑 Playback loop ended");
        self.stop_internal();
        Ok(())
    }

    fn handle_keypress(&mut self, keycode: Keycode) -> bool {
        let mut quit = false;
        match keycode {
            Keycode::Space => {
                if self.is_paused() {
                    info!("▶ Resuming playback");
                    self.resume();
                } else {
                    info!("⏸ Pausing playback");
                    self.pause();
                }
            }
            Keycode::Q | Keycode::Escape => {
                info!("👋 Quit requested via keyboard");
                self.is_running.store(false, Ordering::SeqCst);
                quit = true;
            }
            Keycode::Up => {
                // Volume up
                if let Some(ref audio_player) = self.audio_player {
                    let current_vol = audio_player.get_volume();
                    let new_vol = (current_vol + 10).min(100);
                    audio_player.set_volume(new_vol);
                    info!("🔊 Volume: {}%", new_vol);
                }
            }
            Keycode::Down => {
                // Volume down
                if let Some(ref audio_player) = self.audio_player {
                    let current_vol = audio_player.get_volume();
                    let new_vol = current_vol.saturating_sub(10);
                    audio_player.set_volume(new_vol);
                    info!("🔉 Volume: {}%", new_vol);
                }
            }
            Keycode::M => {
                // Mute/unmute
                if let Some(ref audio_player) = self.audio_player {
                    let current_vol = audio_player.get_volume();
                    if current_vol > 0 {
                        audio_player.set_volume(0);
                        info!("🔇 Muted");
                    } else {
                        audio_player.set_volume(50);
                        info!("🔊 Unmuted (50%)");
                    }
                }
            }
            _ => {}
        }
        quit
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

    fn stop_internal(&mut self) {
        info!("🛑 Stopping internal components...");
        self.is_running.store(false, Ordering::SeqCst);
        
        // Stop audio player
        if let Some(ref audio_player) = self.audio_player {
            audio_player.stop();
        }
        
        // Stop decoder
        if let Some(ref decoder) = self.decoder {
            decoder.stop();
        }
        
        // Stop video renderer
        if let Some(ref video_window) = self.video_window {
            video_window.renderer.stop();
        }

        // Wait for decode thread to finish
        if let Some(handle) = self.decode_thread.take() {
            info!("Waiting for decoder thread to finish...");
            let _ = handle.join();
            info!("Decoder thread finished");
        }
    }

    pub fn stop(&mut self) {
        info!("⏹ Stopping playback...");
        self.stop_internal();
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::SeqCst)
    }

    pub fn is_playing(&self) -> bool {
        self.is_running.load(Ordering::SeqCst) && !self.is_paused()
    }

    fn extract_stream_urls(&self, url: &str) -> Result<(String, String)> {
        info!("🔍 Extracting stream URLs with yt-dlp...");

        // Use simpler format selection for better compatibility
        let output = Command::new("yt-dlp")
            .args([
                "-j",
                "-f", "best[height<=1080]/best",
                "-t", "sleep",
                url
            ])
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

        // Try to get direct URL first
        if let Some(url_str) = json["url"].as_str() {
            info!("Using direct URL from yt-dlp");
            return Ok((url_str.to_string(), url_str.to_string()));
        }

        // Otherwise, parse formats array
        let formats = json["formats"].as_array().ok_or_else(|| PlayerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No formats found in yt-dlp output",
        )))?;

        // Find best video format
        let mut best_video_url: Option<String> = None;
        let mut best_video_quality = 0u64;

        // Find best audio format  
        let mut best_audio_url: Option<String> = None;
        let mut best_audio_bitrate = 0u64;

        for format in formats {
            let url_opt = format["url"].as_str();
            if url_opt.is_none() {
                continue;
            }
            let format_url = url_opt.unwrap().to_string();

            // Skip HLS/DASH manifests
            let protocol = format["protocol"].as_str().unwrap_or("");
            if protocol.contains("m3u8") || protocol.contains("dash") {
                continue;
            }

            // Check for video format
            if let Some(height) = format["height"].as_u64() {
                if height > best_video_quality {
                    best_video_quality = height;
                    best_video_url = Some(format_url.clone());
                }
            }

            // Check for audio format
            if let Some(abr) = format["abr"].as_u64() {
                if abr > best_audio_bitrate {
                    best_audio_bitrate = abr;
                    best_audio_url = Some(format_url.clone());
                }
            }

            // Fallback: use any format that has both audio and video
            if best_video_url.is_none() && best_audio_url.is_none() {
                if format["vcodec"].as_str().unwrap_or("none") != "none" && 
                   format["acodec"].as_str().unwrap_or("none") != "none" {
                    best_video_url = Some(format_url.clone());
                    best_audio_url = Some(format_url.clone());
                }
            }
        }

        let video_url = best_video_url.as_ref().or_else(|| best_audio_url.as_ref()).cloned()
            .ok_or_else(|| PlayerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No suitable video URL found in yt-dlp output",
            )))?;

        let audio_url = best_audio_url
            .or_else(|| best_video_url.as_ref().cloned())
            .ok_or_else(|| {
                PlayerError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "No suitable audio URL found in yt-dlp output",
                ))
            })?;

        info!("Selected video quality: {}p, audio bitrate: {}kbps", 
              best_video_quality, best_audio_bitrate);

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
        self.stop_internal();
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