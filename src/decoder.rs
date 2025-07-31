use ffmpeg_next::{
    codec,
    format::context::Input,
    frame::Video,
    media::Type,
    software::scaling::{context::Context as Scaler, flag::Flags},
    util::format,
};

/// Decodes video packets from the input context and optionally scales the frames
pub fn decode_video_stream(mut ictx: Input) -> Result<(), ffmpeg_next::Error> {
    // Initialize the FFmpeg library
    ffmpeg_next::init().unwrap();

    // Find the best video stream (first one in most cases)
    let input_stream = ictx
        .streams()
        .best(Type::Video)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;

    let video_stream_index = input_stream.index();

    // Get decoder for the stream
    let context_decoder = codec::context::Context::from_parameters(input_stream.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    // Create a scaler to convert to RGB format (if needed)
    let mut scaler = Scaler::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        format::Pixel::RGB24,
        decoder.width(),
        decoder.height(),
        Flags::BILINEAR,
    )?;

    let mut decoded_frame = Video::empty();
    let mut rgb_frame = Video::empty();

    // Read all packets from the input video file
    for (stream, packet) in ictx.packets() {
        if stream.index() == video_stream_index {
            // Send packet to decoder
            decoder.send_packet(&packet)?;

            // Receive frames
            while decoder.receive_frame(&mut decoded_frame).is_ok() {
                // Convert the frame to RGB using the scaler
                scaler.run(&decoded_frame, &mut rgb_frame)?;

                // Here you could display, save, or process the frame
                println!(
                    "Decoded frame: {}x{}, format: {:?}",
                    rgb_frame.width(),
                    rgb_frame.height(),
                    rgb_frame.format()
                );
            }
        }
    }

    // Flush the decoder
    decoder.send_eof()?;
    while decoder.receive_frame(&mut decoded_frame).is_ok() {
        scaler.run(&decoded_frame, &mut rgb_frame)?;
        println!(
            "Flushed frame: {}x{}, format: {:?}",
            rgb_frame.width(),
            rgb_frame.height(),
            rgb_frame.format()
        );
    }

    Ok(())
}
