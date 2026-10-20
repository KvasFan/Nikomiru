// SPDX-License-Identifier: GPL-3.0-only

//! The video as an iced `shader` widget. wgpu runs on its OpenGL backend
//! (libmpv renders only through OpenGL): in `prepare`, with wgpu's EGL
//! context current, mpv renders into a GL texture that wgpu knows as one of
//! its own (`texture_from_raw`), and `draw` samples it in iced's render
//! pass. No copy, as in Qt's `QQuickFramebufferObject`.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use glow::HasContext;
use iced::wgpu;
use iced::widget::shader::{self, Viewport};
use iced::{Rectangle, mouse};
use s5_common::mpv::{Mpv, RenderStats, Renderer};

/// What the widget, its primitive and mpv's callbacks share.
pub struct Video {
    pub mpv: Arc<Mpv>,
    pub stats: Arc<RenderStats>,
    /// The render context exists: the file can be loaded.
    pub ready: AtomicBool,
    /// mpv has a new frame, or the video plays: keep redrawing.
    pub dirty: Arc<AtomicBool>,
    pub playing: AtomicBool,
}

impl std::fmt::Debug for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Video")
    }
}

/// The widget's program.
pub struct Program(pub Arc<Video>);

impl<Message> shader::Program<Message> for Program {
    type State = ();
    type Primitive = Primitive;

    fn update(
        &self,
        _state: &mut (),
        event: &iced::Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<shader::Action<Message>> {
        // a frame per screen refresh while the video plays or mpv has
        // something new (as Qt Quick updates the item)
        let redraw = matches!(
            event,
            iced::Event::Window(iced::window::Event::RedrawRequested(_))
        );
        let v = &self.0;
        (redraw && (v.playing.load(Ordering::Relaxed) || v.dirty.swap(false, Ordering::AcqRel)))
            .then(shader::Action::request_redraw)
    }

    fn draw(&self, _state: &(), _cursor: mouse::Cursor, _bounds: Rectangle) -> Primitive {
        Primitive(Arc::clone(&self.0))
    }
}

#[derive(Debug)]
pub struct Primitive(Arc<Video>);

struct Target {
    size: (u32, u32),
    texture: glow::NativeTexture,
    fbo: glow::NativeFramebuffer,
    bind_group: wgpu::BindGroup,
    _view: wgpu::TextureView,
    _texture: wgpu::Texture,
}

pub struct Pipeline {
    renderer: Option<Renderer>,
    target: Option<Target>,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

const SHADER: &str = r#"
struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@vertex fn vs(@builtin(vertex_index) i: u32) -> Out {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}
// mpv writes sRGB values: decode them when the target re-encodes
@fragment fn fs(i: Out) -> @location(0) vec4<f32> {
    let c = textureSample(tex, smp, i.uv);
    return vec4<f32>(DECODE, 1.0);
}
"#;

impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Pipeline {
        let decode = if format.is_srgb() {
            "pow(c.rgb, vec3<f32>(2.2))"
        } else {
            "c.rgb"
        };
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("s5 video"),
            source: wgpu::ShaderSource::Wgsl(SHADER.replace("DECODE", decode).into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("s5 video"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("s5 video"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("s5 video"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Pipeline {
            renderer: None,
            target: None,
            pipeline,
            layout,
            sampler,
        }
    }
}

impl shader::Primitive for Primitive {
    type Pipeline = Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Pipeline,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let scale = viewport.scale_factor();
        let size = (
            (bounds.width * scale).round() as u32,
            (bounds.height * scale).round() as u32,
        );
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        let Some(hal) = (unsafe { device.as_hal::<wgpu::hal::api::Gles>() }) else {
            eprintln!("[s5] wgpu is not on its OpenGL backend (WGPU_BACKEND=gl)");
            return;
        };
        // makes wgpu's EGL context current on this thread
        let gl = hal.context().lock();
        let v = &self.0;
        if pipeline.renderer.is_none() {
            let dirty = Arc::clone(&v.dirty);
            match unsafe {
                Renderer::new(Arc::clone(&v.mpv), Arc::clone(&v.stats), move || {
                    dirty.store(true, Ordering::Release)
                })
            } {
                Ok(r) => {
                    pipeline.renderer = Some(r);
                    v.ready.store(true, Ordering::Release);
                }
                Err(e) => {
                    eprintln!("[s5] mpv render context: {e}");
                    return;
                }
            }
        }
        if pipeline.target.as_ref().is_none_or(|t| t.size != size) {
            if let Some(old) = pipeline.target.take() {
                unsafe {
                    gl.delete_framebuffer(old.fbo);
                }
            }
            let (texture, fbo) = unsafe {
                let texture = gl.create_texture().unwrap();
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_storage_2d(
                    glow::TEXTURE_2D,
                    1,
                    glow::RGBA8,
                    size.0 as i32,
                    size.1 as i32,
                );
                gl.bind_texture(glow::TEXTURE_2D, None);
                let fbo = gl.create_framebuffer().unwrap();
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(texture),
                    0,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                (texture, fbo)
            };
            let extent = wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            };
            let hal_texture = unsafe {
                hal.texture_from_raw(
                    NonZeroU32::new(texture.0.get()).unwrap(),
                    &wgpu::hal::TextureDescriptor {
                        label: Some("s5 video"),
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUses::RESOURCE,
                        memory_flags: wgpu::hal::MemoryFlags::empty(),
                        view_formats: vec![],
                    },
                    None,
                )
            };
            let wgpu_texture = unsafe {
                device.create_texture_from_hal::<wgpu::hal::api::Gles>(
                    hal_texture,
                    &wgpu::TextureDescriptor {
                        label: Some("s5 video"),
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                )
            };
            let view = wgpu_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("s5 video"),
                layout: &pipeline.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                    },
                ],
            });
            pipeline.target = Some(Target {
                size,
                texture,
                fbo,
                bind_group,
                _view: view,
                _texture: wgpu_texture,
            });
        }
        let (Some(r), Some(t)) = (&pipeline.renderer, &pipeline.target) else {
            return;
        };
        // about the previous frame: iced has no hook after the swap
        if v.stats.renders.load(Ordering::Relaxed) > 0 {
            r.report_swap();
        }
        unsafe {
            // wgpu-hal binds some objects once and relies on them (its
            // vertex array): mpv's render changes them, so they come back
            let saved = GlState::save(&gl);
            // and mpv expects OpenGL's defaults, which wgpu changed
            for cap in [
                glow::FRAMEBUFFER_SRGB,
                glow::SCISSOR_TEST,
                glow::BLEND,
                glow::DEPTH_TEST,
                glow::STENCIL_TEST,
                glow::CULL_FACE,
                glow::RASTERIZER_DISCARD,
            ] {
                gl.disable(cap);
            }
            gl.color_mask(true, true, true, true);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            // wgpu's sampler objects override mpv's texture parameters
            for unit in 0..SAMPLER_UNITS {
                gl.bind_sampler(unit, None);
            }
            r.render(t.fbo.0.get() as i32, size.0 as i32, size.1 as i32, false);
            saved.restore(&gl);
        }
        let _ = t.texture;
    }

    fn draw(&self, pipeline: &Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some(t) = &pipeline.target else {
            return true;
        };
        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_bind_group(0, &t.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
        true
    }
}

/// Texture units whose sampler objects are unbound for mpv.
const SAMPLER_UNITS: u32 = 16;

/// The GL state that mpv's render changes and wgpu does not set again.
struct GlState {
    vertex_array: i32,
    program: i32,
    draw_framebuffer: i32,
    read_framebuffer: i32,
    array_buffer: i32,
    active_texture: i32,
    texture_2d: i32,
    unpack_buffer: i32,
    unpack_alignment: i32,
    unpack_row_length: i32,
    viewport: [i32; 4],
    scissor: [i32; 4],
    enabled: [(u32, bool); 6],
    samplers: [i32; SAMPLER_UNITS as usize],
    color_mask: [i32; 4],
}

impl GlState {
    unsafe fn save(gl: &glow::Context) -> GlState {
        unsafe {
            let int = |p| gl.get_parameter_i32(p);
            let mut viewport = [0; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            let mut scissor = [0; 4];
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor);
            let mut color_mask = [0i32; 4];
            gl.get_parameter_i32_slice(glow::COLOR_WRITEMASK, &mut color_mask);
            let active = int(glow::ACTIVE_TEXTURE);
            let mut samplers = [0; SAMPLER_UNITS as usize];
            for (unit, s) in samplers.iter_mut().enumerate() {
                gl.active_texture(glow::TEXTURE0 + unit as u32);
                *s = int(glow::SAMPLER_BINDING);
            }
            gl.active_texture(active as u32);
            GlState {
                samplers,
                vertex_array: int(glow::VERTEX_ARRAY_BINDING),
                program: int(glow::CURRENT_PROGRAM),
                draw_framebuffer: int(glow::DRAW_FRAMEBUFFER_BINDING),
                read_framebuffer: int(glow::READ_FRAMEBUFFER_BINDING),
                array_buffer: int(glow::ARRAY_BUFFER_BINDING),
                active_texture: int(glow::ACTIVE_TEXTURE),
                texture_2d: int(glow::TEXTURE_BINDING_2D),
                unpack_buffer: int(glow::PIXEL_UNPACK_BUFFER_BINDING),
                unpack_alignment: int(glow::UNPACK_ALIGNMENT),
                unpack_row_length: int(glow::UNPACK_ROW_LENGTH),
                viewport,
                scissor,
                enabled: [
                    glow::BLEND,
                    glow::SCISSOR_TEST,
                    glow::DEPTH_TEST,
                    glow::STENCIL_TEST,
                    glow::CULL_FACE,
                    glow::FRAMEBUFFER_SRGB,
                ]
                .map(|cap| (cap, gl.is_enabled(cap))),
                color_mask,
            }
        }
    }

    unsafe fn restore(&self, gl: &glow::Context) {
        let name = |id: i32| NonZeroU32::new(id as u32);
        unsafe {
            gl.bind_vertex_array(name(self.vertex_array).map(glow::NativeVertexArray));
            gl.use_program(name(self.program).map(glow::NativeProgram));
            gl.bind_framebuffer(
                glow::DRAW_FRAMEBUFFER,
                name(self.draw_framebuffer).map(glow::NativeFramebuffer),
            );
            gl.bind_framebuffer(
                glow::READ_FRAMEBUFFER,
                name(self.read_framebuffer).map(glow::NativeFramebuffer),
            );
            gl.bind_buffer(
                glow::ARRAY_BUFFER,
                name(self.array_buffer).map(glow::NativeBuffer),
            );
            gl.bind_buffer(
                glow::PIXEL_UNPACK_BUFFER,
                name(self.unpack_buffer).map(glow::NativeBuffer),
            );
            for (unit, s) in self.samplers.iter().enumerate() {
                gl.bind_sampler(unit as u32, name(*s).map(glow::NativeSampler));
            }
            gl.active_texture(self.active_texture as u32);
            gl.bind_texture(
                glow::TEXTURE_2D,
                name(self.texture_2d).map(glow::NativeTexture),
            );
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, self.unpack_alignment);
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, self.unpack_row_length);
            let [x, y, w, h] = self.viewport;
            gl.viewport(x, y, w, h);
            let [x, y, w, h] = self.scissor;
            gl.scissor(x, y, w, h);
            for (cap, on) in self.enabled {
                if on { gl.enable(cap) } else { gl.disable(cap) }
            }
            let [r, g, b, a] = self.color_mask.map(|m| m != 0);
            gl.color_mask(r, g, b, a);
        }
    }
}

/// A 1×1 primitive that marks the frame in which it is first drawn (the
/// popup measurement).
pub struct ProbeProgram {
    pub probe: Arc<s5_common::probe::FrameProbe>,
    pub generation: u64,
}

#[derive(Debug)]
pub struct ProbePrimitive(Arc<s5_common::probe::FrameProbe>, u64);

pub struct NoPipeline;

impl shader::Pipeline for NoPipeline {
    fn new(_: &wgpu::Device, _: &wgpu::Queue, _: wgpu::TextureFormat) -> NoPipeline {
        NoPipeline
    }
}

impl shader::Primitive for ProbePrimitive {
    type Pipeline = NoPipeline;

    fn prepare(
        &self,
        _: &mut NoPipeline,
        _: &wgpu::Device,
        _: &wgpu::Queue,
        _: &Rectangle,
        _: &Viewport,
    ) {
    }

    // when the frame's render passes are recorded: after every layer of
    // the frame, text included, has been prepared
    fn draw(&self, _: &NoPipeline, _: &mut wgpu::RenderPass<'_>) -> bool {
        self.0.mark(self.1);
        true
    }
}

impl<Message> shader::Program<Message> for ProbeProgram {
    type State = ();
    type Primitive = ProbePrimitive;

    fn draw(&self, _state: &(), _cursor: mouse::Cursor, _bounds: Rectangle) -> ProbePrimitive {
        ProbePrimitive(Arc::clone(&self.probe), self.generation)
    }
}
