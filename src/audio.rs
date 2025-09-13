// src/audio.rs
use crate::{decoder::AudioSample, error::{PlayerError, Result}};
use crossbeam_channel::Receiver;
use rodio::{OutputStream, Sink, Source};
use std::{
    sync::{Arc, atomic::{AtomicBool, AtomicU8, Ordering}, Mutex},
    time::Duration,
};

pub struct AudioPlayer {
    _stream: OutputStream,
    sink: Arc<Mutex<Sink>>,
    audio_receiver: Receiver<AudioSample>,
    should_stop: Arc<AtomicBool>,
    volume: Arc<AtomicU8>, // 0-100
    is_paused: Arc<AtomicBool>,
    current_sample: Arc<Mutex<Option<AudioSample>>>,
}

impl AudioPlayer {
    pub fn new(audio_receiver: Receiver<AudioSample>) -> Result<Self> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| PlayerError::Audio(format!("Failed to create audio output stream: {}", e)))?;
        
        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| PlayerError::Audio(format!("Failed to create audio sink: {}", e)))?;

        Ok(AudioPlayer {
            _stream: stream,
            sink: Arc::new(Mutex::new(sink)),
            audio_receiver,
            should_stop: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(AtomicU8::new(100)), // Start at full volume
            is_paused: Arc::new(AtomicBool::new(false)),
            current_sample: Arc::new(Mutex::new(None)),
        })
    }

    pub fn start_playback(&self) -> Result<()> {
        let sink = self.sink.clone();
        let audio_receiver = self.audio_receiver.clone();
        let should_stop = self.should_stop.clone();
        let volume = self.volume.clone();
        let is_paused = self.is_paused.clone();
        let current_sample = self.current_sample.clone();

        std::thread::spawn(move || {
            while !should_stop.load(Ordering::SeqCst) {
                // Check if paused
                if is_paused.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }

                // Try to receive audio samples
                match audio_receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(sample) => {
                        // Update current sample for external access
                        {
                            let mut current = current_sample.lock().unwrap();
                            *current = Some(sample.clone());
                        }

                        // Create audio source from sample
                        let source = AudioSource::new(sample);
                        
                        // Apply volume
                        let vol = volume.load(Ordering::SeqCst) as f32 / 100.0;
                        let source_with_volume = source.amplify(vol);
                        
                        // Add to sink
                        if let Ok(sink_guard) = sink.lock() {
                            sink_guard.append(source_with_volume);
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        // No audio data available, continue
                        continue;
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        log::info!("Audio receiver disconnected");
                        break;
                    }
                }
            }
            log::info!("Audio playback thread terminated");
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::SeqCst);
        if let Ok(sink) = self.sink.lock() {
            sink.stop();
        }
    }

    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::SeqCst);
        if let Ok(sink) = self.sink.lock() {
            sink.pause();
        }
    }

    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::SeqCst);
        if let Ok(sink) = self.sink.lock() {
            sink.play();
        }
    }

    pub fn set_volume(&self, volume: u8) {
        let clamped_volume = volume.min(100);
        self.volume.store(clamped_volume, Ordering::SeqCst);
        
        // Apply volume to current sink
        if let Ok(sink) = self.sink.lock() {
            sink.set_volume(clamped_volume as f32 / 100.0);
        }
    }

    pub fn get_volume(&self) -> u8 {
        self.volume.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::SeqCst)
    }

    pub fn is_playing(&self) -> bool {
        !self.is_paused() && !self.sink.lock().unwrap().empty()
    }

    pub fn get_current_sample_info(&self) -> Option<(u32, u16, f64)> {
        self.current_sample.lock().unwrap().as_ref().map(|sample| {
            (sample.sample_rate, sample.channels, sample.timestamp)
        })
    }

    pub fn clear_buffer(&self) {
        if let Ok(sink) = self.sink.lock() {
            sink.stop();
        }
    }
}

// Custom audio source implementation for rodio
pub struct AudioSource {
    data: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    position: usize,
}

impl AudioSource {
    pub fn new(sample: AudioSample) -> Self {
        Self {
            data: sample.data,
            sample_rate: sample.sample_rate,
            channels: sample.channels,
            position: 0,
        }
    }
}

impl Source for AudioSource {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.data.len() - self.position)
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.data.len() as f32 / (self.sample_rate as f32 * self.channels as f32),
        ))
    }
}

impl Iterator for AudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position < self.data.len() {
            let sample = self.data[self.position];
            self.position += 1;
            Some(sample)
        } else {
            None
        }
    }
}