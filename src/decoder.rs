use ffmpeg_next::{
    codec, format, frame::{Audio, Video},
    media::Type,
    software::resampling,
    util::format::sample::Sample,
};
use std::error::Error;

pub fn decode_combined_streams(video_path: &str, audio_path: &str) -> Result<(), Box<dyn Error>> {
    ffmpeg_next::init()?;

    // --- Video Stream Setup ---
    let mut video_ictx = format::input(&video_path)?;
    let video_input = video_ictx
        .streams()
        .best(Type::Video)
        .ok_or("Could not find video stream in video input")?;
    let video_stream_index = video_input.index();
    let mut video_decoder = codec::context::Context::from_parameters(video_input.parameters())?.decoder().video()?;
    
    let mut video_frame_index = 0;
    let mut receive_video_frame = Video::empty();

    // --- Audio Stream Setup ---
    let mut audio_ictx = format::input(&audio_path)?;
    let audio_input = audio_ictx
        .streams()
        .best(Type::Audio)
        .ok_or("Could not find audio stream in audio input")?;
    let audio_stream_index = audio_input.index();
    let mut audio_decoder = codec::context::Context::from_parameters(audio_input.parameters())?.decoder().audio()?;

    let mut resampler = resampling::Context::get(
        audio_decoder.format(),
        audio_decoder.channel_layout(),
        audio_decoder.rate(),
        Sample::I16(ffmpeg_next::util::format::sample::Type::Packed),
        audio_decoder.channel_layout(),
        audio_decoder.rate(),
    )?;

    let mut decoded_audio = Audio::empty();
    let mut converted_audio = Audio::empty();

    // --- Combined Packet Processing Loop ---
    // This part is simplified for now: process all video packets, then all audio packets.
    // For true combined playback, a more complex interleaving logic is needed.

    // Process video packets
    for (_stream, packet) in video_ictx.packets() {
        if packet.stream() == video_stream_index {
            video_decoder.send_packet(&packet)?;
            while video_decoder.receive_frame(&mut receive_video_frame).is_ok() {
                println!(
                    "Decoded video frame {}: {}x{}",
                    video_frame_index,
                    receive_video_frame.width(),
                    receive_video_frame.height()
                );
                video_frame_index += 1;
            }
        }
    }
    video_decoder.send_eof()?;
    while video_decoder.receive_frame(&mut receive_video_frame).is_ok() {
        println!(
            "Decoded final video frame {}: {}x{}",
            video_frame_index,
            receive_video_frame.width(),
            receive_video_frame.height()
        );
        video_frame_index += 1;
    }
    println!("Video decoding finished.");


    // Process audio packets
    for (_stream, packet) in audio_ictx.packets() {
        if packet.stream() == audio_stream_index {
            audio_decoder.send_packet(&packet)?;
            while audio_decoder.receive_frame(&mut decoded_audio).is_ok() {
                resampler.run(&decoded_audio, &mut converted_audio)?;
                println!(
                    "Decoded audio frame: {} samples x {} channels",
                    converted_audio.samples(),
                    converted_audio.channels()
                );
            }
        }
    }
    audio_decoder.send_eof()?;
    while audio_decoder.receive_frame(&mut decoded_audio).is_ok() {
        resampler.run(&decoded_audio, &mut converted_audio)?;
        println!(
            "Final audio frame: {} samples x {} channels",
            converted_audio.samples(),
            converted_audio.channels()
        );
    }
    println!("Audio decoding finished.");

    Ok(())
}
