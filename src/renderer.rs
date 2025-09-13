// src/renderer.rs
use crate::{decoder::VideoFrame, error::{PlayerError, Result}};
use crossbeam_channel::Receiver;
use sdl2::{
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
    render::{Canvas, Texture, TextureCreator},
    video::{Window, WindowContext},
    EventPump, VideoSubsystem,
};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use log::debug;

pub struct VideoRenderer {
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    video_receiver: Receiver<VideoFrame>,
    should_stop: Arc<AtomicBool>,
    current_frame: Option<VideoFrame>,
}

impl VideoRenderer {
    pub fn new(
        canvas: Canvas<Window>,
        video_receiver: Receiver<VideoFrame>,
    ) -> Result<Self> {
        let texture_creator = canvas.texture_creator();

        Ok(VideoRenderer {
            canvas,
            texture_creator,
            video_receiver,
            should_stop: Arc::new(AtomicBool::new(false)),
            current_frame: None,
        })
    }

    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }

    pub fn update(&mut self) -> Result<bool> {
        if self.should_stop.load(Ordering::SeqCst) {
            return Ok(false);
        }

        // Try to get the latest frame
        let mut new_frame_received = false;
        while let Ok(frame) = self.video_receiver.try_recv() {
            self.current_frame = Some(frame);
            new_frame_received = true;
            debug!("Video frame received: {}x{} at timestamp {}", self.current_frame.as_ref().unwrap().width, self.current_frame.as_ref().unwrap().height, self.current_frame.as_ref().unwrap().timestamp);
        }

        // Render current frame if we have one
        self.render_frame()?;

        Ok(new_frame_received)
    }

    fn render_frame(&mut self) -> Result<()> {
        if let Some(frame) = &self.current_frame {
            debug!("Rendering frame: {}x{} (Frame num: {})", frame.width, frame.height, frame.frame_number);
            let mut texture = self.texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, frame.width, frame.height)
                .map_err(|e| PlayerError::Video(e.to_string()))?;
            
            texture
                .update(None, &frame.data, (frame.width * 3) as usize)
                .map_err(|e| PlayerError::Video(e.to_string()))?;

            // Clear canvas and copy texture
            self.canvas.set_draw_color(Color::RGB(0, 0, 0));
            self.canvas.clear();

            // Calculate scaling to fit window while maintaining aspect ratio
            let window_size = self.canvas.output_size().map_err(|e| PlayerError::Sdl(e.to_string()))?;
            let dst_rect = self.calculate_display_rect(
                frame.width,
                frame.height,
                window_size.0,
                window_size.1,
            );

            self.canvas
                .copy(&texture, None, Some(dst_rect))
                .map_err(|e| PlayerError::Video(e.to_string()))?;

            self.canvas.present();
        } else {
            debug!("No frame to render, clearing canvas.");
            // Clear with black if no frame
            self.canvas.set_draw_color(Color::RGB(0, 0, 0));
            self.canvas.clear();
            self.canvas.present();
        }

        Ok(())
    }

    fn calculate_display_rect(&self, src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Rect {
        let src_aspect = src_w as f32 / src_h as f32;
        let dst_aspect = dst_w as f32 / dst_h as f32;

        let (scaled_w, scaled_h) = if src_aspect > dst_aspect {
            // Video is wider than display
            (dst_w, (dst_w as f32 / src_aspect) as u32)
        } else {
            // Video is taller than display
            ((dst_h as f32 * src_aspect) as u32, dst_h)
        };

        let x = (dst_w - scaled_w) / 2;
        let y = (dst_h - scaled_h) / 2;

        Rect::new(x as i32, y as i32, scaled_w, scaled_h)
    }

    pub fn get_current_frame_info(&self) -> Option<(u32, u32, f64, u64)> {
        self.current_frame.as_ref().map(|frame| {
            (frame.width, frame.height, frame.timestamp, frame.frame_number)
        })
    }

    pub fn handle_window_resize(&mut self) -> Result<()> {
        // This is now handled by recreating the texture on every frame.
        Ok(())
    }
}

pub struct VideoWindow {
    pub sdl_context: sdl2::Sdl,
    pub video_subsystem: VideoSubsystem,
    pub event_pump: EventPump,
    pub renderer: VideoRenderer,
}

impl VideoWindow {
    pub fn new(
        video_receiver: Receiver<VideoFrame>,
        width: u32,
        height: u32,
        title: &str,
    ) -> Result<Self> {
        let sdl_context = sdl2::init().map_err(|e| PlayerError::Sdl(e.to_string()))?;
        let video_subsystem = sdl_context.video().map_err(|e| PlayerError::Sdl(e.to_string()))?;
        let event_pump = sdl_context.event_pump().map_err(|e| PlayerError::Sdl(e.to_string()))?;

        let window = video_subsystem
            .window(title, width, height)
            .position_centered()
            .resizable()
            .build()
            .map_err(|e| PlayerError::Sdl(e.to_string()))?;

        let canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .map_err(|e| PlayerError::Sdl(e.to_string()))?;

        let renderer = VideoRenderer::new(canvas, video_receiver)?;

        Ok(VideoWindow {
            sdl_context,
            video_subsystem,
            event_pump,
            renderer,
        })
    }
}