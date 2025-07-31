mod mp4_parser;
mod decoder;

use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};

fn main() -> std::io::Result<()> {
    // Path to your MP4 file
    let file_path = "/home/ishan/projects/Projects/rust-stream-player/media/Collection and Generics in Java.mp4";

    // -------------------------------
    // Step 1: Parse MP4 Box Structure
    // -------------------------------
    {
        let file = File::open(file_path)?;
        let mut reader = BufReader::new(file);

        // Seek to end to get file size
        let end = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;

        // Recursively parse all MP4 boxes
        mp4_parser::parse_mp4_boxes(&mut reader, 0, end)?;
    }

    // -------------------------------
    // Step 2: Decode Video Stream
    // -------------------------------
    {
        match ffmpeg_next::format::input(&file_path) {
            Ok(input_ctx) => match decoder::decode_video_stream(input_ctx) {
                Ok(_) => println!("✅ Video decoding finished successfully."),
                Err(e) => eprintln!("❌ Error during decoding: {:?}", e),
            },
            Err(e) => eprintln!("❌ Could not open input file: {:?}", e),
        }
    }

    Ok(())
}
