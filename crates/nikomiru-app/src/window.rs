// SPDX-License-Identifier: GPL-3.0-only

//! The window and its OpenGL context, and the GL work of each frame around
//! mpv's render (SPECIFICATION §5.2.2, ADR 0006).
//!
//! glutin creates the context with EGL first, also on X11: mpv imports VA-API
//! surfaces only through EGL (zero-copy decoding, §5.2.2 #2); GLX is the
//! fallback. `unsafe` is allowed in this module (ARC-011): glutin's context and
//! surface creation, OpenGL calls, and mpv's render context.

use std::num::NonZeroU32;
use std::sync::Arc;

use glow::HasContext;
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext,
};
use glutin::display::{DisplayApiPreference, GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::GlWindow as _;
use nikomiru_player::render::{NativeDisplay, VideoRenderer};
use nikomiru_player::{Player, PlayerError};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{ActivationToken, Window};

/// The window with its OpenGL surface and context. Fields drop in order: the
/// surface and the context before the window they draw into.
pub struct GlWindow {
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    /// The OpenGL functions, shared with egui's painter.
    pub gl: Arc<glow::Context>,
    gles: bool,
    /// The window.
    pub window: Window,
}

impl std::fmt::Debug for GlWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlWindow")
            .field("id", &self.window.id())
            .finish_non_exhaustive()
    }
}

impl GlWindow {
    /// Creates the window, hidden until its first frame, and makes its context
    /// current on this thread. `token` comes from the launcher (startup
    /// notification).
    pub fn new(
        event_loop: &ActiveEventLoop,
        title: &str,
        token: Option<String>,
    ) -> Result<Self, String> {
        use winit::platform::startup_notify::WindowAttributesExtStartupNotify;
        let mut attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_visible(false);
        // the Wayland app id and the X11 class: the `.desktop` file's name (APP-002)
        attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(
            attributes, "nikomiru", "nikomiru",
        );
        attributes = winit::platform::x11::WindowAttributesExtX11::with_name(
            attributes, "nikomiru", "nikomiru",
        );
        if let Some(token) = token {
            attributes = attributes.with_activation_token(ActivationToken::from_raw(token));
        }
        let raw_display = event_loop
            .display_handle()
            .map_err(|e| format!("display handle: {e}"))?
            .as_raw();
        // EGL first, GLX if there is no EGL (X11 only)
        let preference = DisplayApiPreference::EglThenGlx(Box::new(
            winit::platform::x11::register_xlib_error_hook,
        ));
        // SAFETY: the handle is the event loop's display, which outlives this window
        // and its context (they are dropped when the loop exits).
        let display = unsafe { glutin::display::Display::new(raw_display, preference) }
            .map_err(|e| format!("OpenGL display: {e}"))?;
        let template = ConfigTemplateBuilder::new()
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false)
            .build();
        // SAFETY: a template for this display, built above.
        let configs = unsafe { display.find_configs(template) }
            .map_err(|e| format!("OpenGL configuration: {e}"))?;
        // egui and mpv need no multisampling
        let config = configs
            .min_by_key(GlConfig::num_samples)
            .ok_or_else(|| "no OpenGL configuration for this display".to_string())?;
        let window = glutin_winit::finalize_window(event_loop, attributes, &config)
            .map_err(|e| format!("window: {e}"))?;
        let api = match display {
            glutin::display::Display::Egl(_) => "EGL",
            glutin::display::Display::Glx(_) => "GLX",
        };
        tracing::info!("OpenGL through {api}");
        let raw_window = window
            .window_handle()
            .map_err(|e| format!("window handle: {e}"))?
            .as_raw();
        let desktop = ContextAttributesBuilder::new().build(Some(raw_window));
        let gles = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(raw_window));
        // SAFETY: the configuration and the window handle come from the same display.
        let desktop = unsafe { display.create_context(&config, &desktop) };
        let (context, is_gles) = match desktop {
            Ok(c) => (c, false),
            Err(_) => {
                // SAFETY: as above.
                let gles = unsafe { display.create_context(&config, &gles) };
                (gles.map_err(|e| format!("OpenGL context: {e}"))?, true)
            }
        };
        let surface_attributes = window
            .build_surface_attributes(Default::default())
            .map_err(|e| format!("surface: {e}"))?;
        // SAFETY: the window outlives the surface (field order of `GlWindow`).
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .map_err(|e| format!("surface: {e}"))?;
        let context = context
            .make_current(&surface)
            .map_err(|e| format!("making the context current: {e}"))?;
        // vsync: one frame per refresh (the video's timing comes from mpv)
        if let Err(e) = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN)) {
            tracing::warn!("vsync: {e}");
        }
        // SAFETY: the context is current on this thread; the loader resolves through
        // the same display.
        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name).cast())
        };
        Ok(Self {
            surface,
            context,
            gl: Arc::new(gl),
            gles: is_gles,
            window,
        })
    }

    /// mpv's render context for `player`, drawing into this window. `on_update`
    /// runs on mpv's threads when a frame is due: it must only ask for a redraw.
    pub fn video_renderer(
        &self,
        player: &Player,
        on_update: impl Fn() + Send + Sync + 'static,
    ) -> Result<VideoRenderer, PlayerError> {
        let display = match self.window.display_handle().map(|h| h.as_raw()) {
            Ok(RawDisplayHandle::Wayland(h)) => NativeDisplay::Wayland(h.display.as_ptr()),
            Ok(RawDisplayHandle::Xlib(h)) => h
                .display
                .map_or(NativeDisplay::Unknown, |d| NativeDisplay::X11(d.as_ptr())),
            _ => NativeDisplay::Unknown,
        };
        let gl_display = self.context.display();
        let resolve = Box::new(move |name: &std::ffi::CStr| gl_display.get_proc_address(name));
        // SAFETY: this window's context is current on the UI thread for the whole
        // life of the renderer (`Shell` uses and drops it there, before this window);
        // the display is the window's own; the renderer is dropped before the player
        // shuts down (`main::run`).
        unsafe { VideoRenderer::new(player, display, resolve, on_update) }
    }

    /// Resizes the surface with the window.
    pub fn resize(&self, size: PhysicalSize<u32>) {
        if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
            self.surface.resize(&self.context, w, h);
        }
    }

    /// The framebuffer's size in physical pixels.
    pub fn size(&self) -> (i32, i32) {
        let size = self.window.inner_size();
        (
            i32::try_from(size.width).unwrap_or(i32::MAX),
            i32::try_from(size.height).unwrap_or(i32::MAX),
        )
    }

    /// Puts the OpenGL state where mpv expects it before it draws into the
    /// window's framebuffer (`<mpv/render_gl.h>`: blending and scissor off, no
    /// program, vertex array, buffer or texture bound). egui sets its own state
    /// again when it paints.
    pub fn prepare_video(&self) {
        let (w, h) = self.size();
        let gl = &self.gl;
        // SAFETY: the context is current on this thread; plain state calls.
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, w, h);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            if !self.gles {
                gl.disable(glow::FRAMEBUFFER_SRGB);
            }
            gl.use_program(None);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// A black frame, while there is no video to draw.
    pub fn clear(&self) {
        let (w, h) = self.size();
        // SAFETY: the context is current on this thread; plain state calls.
        unsafe {
            self.gl.viewport(0, 0, w, h);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    /// Presents the frame.
    pub fn swap(&self) -> Result<(), String> {
        self.surface
            .swap_buffers(&self.context)
            .map_err(|e| format!("swap: {e}"))
    }
}
