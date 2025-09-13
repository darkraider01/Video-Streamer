// src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlayerError {
    #[error("FFmpeg error: {0}")]
    FFmpeg(#[from] ffmpeg_next::Error),
    
    #[error("SDL error: {0}")]
    Sdl(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Audio error: {0}")]
    Audio(String),
    
    #[error("Video error: {0}")]
    Video(String),
    
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    
    #[error("Channel error: {0}")]
    Channel(String),
    
    #[error("Synchronization error: {0}")]
    Sync(String),
    
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}

pub type Result<T> = std::result::Result<T, PlayerError>;


impl<T> From<crossbeam_channel::SendError<T>> for PlayerError {
    fn from(_: crossbeam_channel::SendError<T>) -> Self {
        PlayerError::Channel("Failed to send through channel".to_string())
    }
}

impl From<crossbeam_channel::RecvError> for PlayerError {
    fn from(_: crossbeam_channel::RecvError) -> Self {
        PlayerError::Channel("Failed to receive from channel".to_string())
    }
}