// src/main.rs - FIXED VERSION
mod error;
mod decoder;
mod renderer;
mod audio;
mod player;
mod mp4_parser;

use crate::{error::Result, player::MediaPlayer};
use std::io::{self, Write};
use log::{error, info};

fn main() -> Result<()> {
    // Initialize logging with better configuration
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)  // Change back to Info for better performance
        .format_timestamp_secs()
        .init();

    println!("🎬 Rust Video Streamer v0.1.0");
    println!("==============================");

    // Check for command line argument or get URL from user
    let args: Vec<String> = std::env::args().collect();
    let url = if args.len() > 1 {
        args[1].clone()
    } else {
        print!("Enter video URL: ");
        io::stdout().flush()?;
        let mut url_input = String::new();
        io::stdin().read_line(&mut url_input)?;
        url_input.trim().to_string()
    };

    if url.is_empty() {
        eprintln!("❌ No URL entered. Exiting.");
        return Ok(());
    }

    info!("🚀 Initializing media player...");
    
    // Create and configure media player
    let mut player = MediaPlayer::new();
    
    // Load the URL
    match player.load_url(&url) {
        Ok(()) => {
            println!("✅ Media loaded successfully!");
            print_controls();
            
            // Start playback
            match player.run() {
                Ok(()) => {
                    println!("✅ Playback completed successfully");
                }
                Err(e) => {
                    error!("❌ Playback error: {:?}", e);
                    print_troubleshooting();
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to load media: {:?}", e);
            print_troubleshooting();
            std::process::exit(1);
        }
    }

    println!("👋 Thanks for using Rust Video Streamer!");
    Ok(())
}

fn print_controls() {
    println!("\n🎮 Controls:");
    println!("   • SPACE    - Play/Pause");
    println!("   • UP       - Volume Up");
    println!("   • DOWN     - Volume Down"); 
    println!("   • M        - Mute/Unmute");
    println!("   • Q/ESC    - Quit");
    println!("   • Resize the window as needed");
    println!("   • Close window to exit");
    println!("\n▶ Starting playback... Press SPACE to pause.\n");
}

fn print_troubleshooting() {
    println!("\n🔧 Troubleshooting:");
    println!("💡 Make sure you have:");
    println!("   • yt-dlp installed and in PATH: `which yt-dlp`");
    println!("   • FFmpeg libraries installed (libavformat, libavcodec, etc.)");
    println!("   • SDL2 development libraries installed");
    println!("   • Audio system working (try `aplay` on Linux)");
    println!("   • A valid, accessible YouTube URL");
    println!("\n🔍 Debug commands:");
    println!("   • Test yt-dlp: `yt-dlp -j [URL]`");
    println!("   • Check audio: `pacmd list-sinks` (PulseAudio)");
    println!("   • Run with debug: `RUST_LOG=debug cargo run --release`");
    println!("   • Test with simple video: Use a short YouTube video");
    println!("\n📋 System check:");
    println!("   • GPU drivers updated");
    println!("   • Sufficient RAM available");
    println!("   • Network connection stable");
}