mod mp4_parser;
mod decoder;

use std::io::{self, Write};
use std::process::Command;

fn main() -> std::io::Result<()> {
    // -----------------------------------
    // Step 1: Ask for the YouTube URL
    // -----------------------------------
    print!("Enter video URL: ");
    io::stdout().flush()?; // Make sure prompt shows

    let mut url = String::new();
    io::stdin().read_line(&mut url)?;
    let url = url.trim(); // Remove newline

    if url.is_empty() {
        eprintln!("No URL entered. Exiting.");
        return Ok(());
    }

    // -----------------------------------
    // Step 2: Run yt-dlp to get direct stream URL
    // -----------------------------------
    println!("\nFetching direct stream URL with yt-dlp...");

    // Fetch video-only stream URL
    let video_output = Command::new("yt-dlp")
        .args(["-g", "-f", "bestvideo[ext=mp4]", url])
        .output()
        .expect("Failed to run yt-dlp for video");

    if !video_output.status.success() {
        eprintln!("yt-dlp video failed:\n{}", String::from_utf8_lossy(&video_output.stderr));
        return Ok(());
    }
    let video_url = String::from_utf8_lossy(&video_output.stdout)
        .lines()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "No video URL returned by yt-dlp"))?
        .to_string();
    println!("✅ Got video stream URL:\n{}", video_url);

    // Fetch audio-only stream URL
    let audio_output = Command::new("yt-dlp")
        .args(["-g", "-f", "bestaudio[ext=m4a]", url])
        .output()
        .expect("Failed to run yt-dlp for audio");

    if !audio_output.status.success() {
        eprintln!("yt-dlp audio failed:\n{}", String::from_utf8_lossy(&audio_output.stderr));
        return Ok(());
    }
    let audio_url = String::from_utf8_lossy(&audio_output.stdout)
        .lines()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "No audio URL returned by yt-dlp"))?
        .to_string();
    println!("✅ Got audio stream URL:\n{}", audio_url);

    // -----------------------------------
    // Step 3: Decode Video and Audio Streams
    // -----------------------------------
    println!("\n🔄 Starting video and audio decode...");
    match decoder::decode_combined_streams(&video_url, &audio_url) {
        Ok(_) => println!("✅ Video and audio decoding finished successfully."),
        Err(e) => eprintln!("❌ Error during combined decoding: {:?}", e),
    }

    Ok(())
}
