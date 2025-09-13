// src/main.rs
mod error;
mod decoder;
mod renderer;
mod audio;
mod player;
mod mp4_parser;

use crate::{error::Result, player::MediaPlayer};
use std::io::{self, Write};

fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug) // Set to Debug for more verbose output
        .init();

    println!("🎬 Rust Video Streamer v0.1.0");
    println!("==============================");

    let args: Vec<String> = std::env::args().collect();
    let url = if args.len() > 1 {
        args[1].as_str()
    } else {
        print!("Enter video URL: ");
        io::stdout().flush()?;
        let mut url_input = String::new();
        io::stdin().read_line(&mut url_input)?;
        url_input.trim().to_string().leak() // Convert to 'static str for MediaPlayer
    };

    if url.is_empty() {
        eprintln!("❌ No URL entered. Exiting.");
        return Ok(());
    }

    println!("\n🚀 Initializing media player...");
    
    // Create and configure media player
    let mut player = MediaPlayer::new();
    
    // Load the URL
    match player.load_url(url) {
        Ok(()) => {
            println!("✅ Media loaded successfully!");
            print_controls();
            
            // Start playback
            if let Err(e) = player.run() {
                eprintln!("❌ Playback error: {:?}", e);
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to load media: {:?}", e);
            eprintln!("\n💡 Make sure you have:");
            eprintln!("   • yt-dlp installed and in PATH");
            eprintln!("   • FFmpeg libraries installed");
            eprintln!("   • SDL2 libraries installed");
            eprintln!("   • A valid YouTube URL");
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