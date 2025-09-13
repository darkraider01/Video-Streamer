// src/renderer.rs - FIXED VERSION
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

pub struct VideoRenderer<'a> {
    canvas: Canvas<Window>,
    pub texture_creator: TextureCreator<WindowContext>,
    current_texture: Option<Texture<'a>>,
    video_receiver: Receiver<VideoFrame>,
    should_stop: Arc<AtomicBool>,
    current_frame: Option<VideoFrame>,
    last_texture_width: u32,  // Track dimensions for texture reuse
    last_texture_height: u32,
}

impl<'a> VideoRenderer<'a> {
    pub fn new(
        canvas: Canvas<Window>,
        video_receiver: Receiver<VideoFrame>,
    ) -> Result<Self> {
        let texture_creator = canvas.texture_creator();
        Ok(VideoRenderer {
            canvas,
            texture_creator,
            current_texture: None,
            video_receiver,
            should_stop: Arc::new(AtomicBool::new(false)),
            current_frame: None,
            last_texture_width: 0,
            last_texture_height: 0,
        })
    }

    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }

    pub fn update(&mut self) -> Result<bool> {
        if self.should_stop.load(Ordering::SeqCst) {
            return Ok(false);
        }

        // Try to get the latest frame (drain all pending frames to avoid lag)
        let mut new_frame_received = false;
        let mut latest_frame = None;
        
        // Drain the queue to get the most recent frame
        while let Ok(frame) = self.video_receiver.try_recv() {
            latest_frame = Some(frame);
            new_frame_received = true;
        }
        
        if let Some(frame) = latest_frame {
            self.current_frame = Some(frame);
            debug!("Video frame received: {}x{} at timestamp {:.2}s", 
                   self.current_frame.as_ref().unwrap().width, 
                   self.current_frame.as_ref().unwrap().height, 
                   self.current_frame.as_ref().unwrap().timestamp);
        }

        // Always render (even if no new frame)
        self.render_frame()?;

        Ok(new_frame_received)
    }

    fn render_frame(&mut self) -> Result<()> {
        if let Some(frame) = &self.current_frame {
            // CRITICAL FIX: Only create new texture if dimensions changed
            let needs_new_texture = self.current_texture.is_none() || 
                                  self.last_texture_width != frame.width || 
                                  self.last_texture_height != frame.height;

            if needs_new_texture {
                debug!("Creating new texture: {}x{}", frame.width, frame.height);
                let texture = self
                    .texture_creator
                    .create_texture_streaming(PixelFormatEnum::RGB24, frame.width, frame.height)
                    .map_err(|e| PlayerError::Video(format!("Failed to create texture: {}", e)))?;
                
                // Unsafely extend the lifetime of the texture
                let texture: Texture<'a> = unsafe { std::mem::transmute(texture) };
                self.current_texture = Some(texture);
                self.last_texture_width = frame.width;
                self.last_texture_height = frame.height;
            }

            // Update texture with frame data
            let dst_rect = {
                let window_size = self.canvas.output_size()
                    .map_err(|e| PlayerError::Sdl(e.to_string()))?;
                self.calculate_display_rect(
                    frame.width,
                    frame.height,
                    window_size.0,
                    window_size.1,
                )
            };

            if let Some(ref mut texture) = self.current_texture {
                texture
                    .update(None, &frame.data, (frame.width * 3) as usize)
                    .map_err(|e| PlayerError::Video(format!("Failed to update texture: {}", e)))?;

                // Clear canvas
                self.canvas.set_draw_color(Color::RGB(0, 0, 0));
                self.canvas.clear();

                // Copy texture to canvas
                self.canvas
                    .copy(texture, None, Some(dst_rect))
                    .map_err(|e| PlayerError::Video(format!("Failed to copy texture: {}", e)))?;

                self.canvas.present();
            }
        } else {
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
        // Force texture recreation on next render by clearing current texture
        self.current_texture = None;
        Ok(())
    }
}

pub struct VideoWindow<'a> {
    pub event_pump: EventPump,
    pub renderer: VideoRenderer<'a>,
    _sdl_context: sdl2::Sdl,
    _video_subsystem: VideoSubsystem,
}

impl<'a> VideoWindow<'a> {
    pub fn new(
        video_receiver: Receiver<VideoFrame>,
        width: u32,
        height: u32,
        title: &str,
    ) -> Result<VideoWindow<'a>> {
        // Initialize SDL2
        let sdl_context = sdl2::init()
            .map_err(|e| PlayerError::Sdl(format!("Failed to initialize SDL2: {}", e)))?;
        
        let video_subsystem = sdl_context.video()
            .map_err(|e| PlayerError::Sdl(format!("Failed to initialize SDL2 video: {}", e)))?;
        
        let event_pump = sdl_context.event_pump()
            .map_err(|e| PlayerError::Sdl(format!("Failed to create event pump: {}", e)))?;

        // Create window
        let window = video_subsystem
            .window(title, width, height)
            .position_centered()
            .resizable()
            .build()
            .map_err(|e| PlayerError::Sdl(format!("Failed to create window: {}", e)))?;

        // Create accelerated canvas with vsync
        let canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .map_err(|e| PlayerError::Sdl(format!("Failed to create canvas: {}", e)))?;

        let renderer = VideoRenderer::new(canvas, video_receiver)?;

        println!("✅ SDL2 initialized successfully");

        Ok(VideoWindow {
            event_pump,
            renderer,
            _sdl_context: sdl_context,
            _video_subsystem: video_subsystem,
        })
    }
}