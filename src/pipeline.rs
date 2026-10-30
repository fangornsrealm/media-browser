//! Shared wgpu pipeline that blits BGRA frames (from mpv's software
//! renderer) onto the iced canvas.
//!
//! One [`MediaPrimitive`] is submitted per video tab via
//! `renderer.draw_primitive(bounds, primitive)`; all instances share a
//! single [`MediaPipeline`] (per renderer) that holds the wgpu state and
//! one texture slot per media id.
//!
//! Coordinate convention: `bounds` arriving in [`MediaPrimitive::prepare`]
//! is in logical pixels (the default layer transformation is the
//! identity), so `prepare` scales it to physical pixels and stores it on
//! the pipeline entry. The vertex shader emits a quad in physical pixel
//! coordinates and the render pass scissor (set in
//! [`MediaPipeline::draw`]) clips it to that physical rect.
//!
//! Texture orientation and Bgra8 channel order are not consistent across
//! GPU backends/drivers, so [`MediaPipeline`] probes the device once at
//! creation (see `probe_flags`) and compensates in the shader.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced_wgpu::core::{Point, Rectangle, Size};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive::{Pipeline, Primitive};
use iced_wgpu::wgpu;

const SHADER: &str = r#"
struct U {
    offset: vec2<f32>,
    size: vec2<f32>,
    flip_x: f32,
    flip_y: f32,
    swap: f32,
    _pad: f32,
    target_size: vec2<f32>,
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@group(0) @binding(2) var<uniform> u: U;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Triangle strip: TL, BL, TR, BR. Positions are computed in physical
// pixel space and projected to NDC with the target size (the same
// convention the iced geometry pipeline uses).
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VOut {
    let c = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let uv = c[i];
    var out: VOut;
    let px = uv * u.size + u.offset;
    out.pos = vec4<f32>(
        2.0 * px.x / u.target_size.x - 1.0,
        1.0 - 2.0 * px.y / u.target_size.y,
        0.0, 1.0
    );
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // Texture axis orientation and Bgra8 channel order differ between
    // GPU backends/drivers; the pipeline probes the device once at
    // creation and compensates with these flags.
    var uv = in.uv;
    if u.flip_x > 0.5 {
        uv.x = 1.0 - uv.x;
    }
    if u.flip_y > 0.5 {
        uv.y = 1.0 - uv.y;
    }
    let c = textureSample(tex, s, uv);
    if u.swap > 0.5 {
        return vec4<f32>(c.b, c.g, c.r, 1.0);
    }
    return vec4<f32>(c.r, c.g, c.b, 1.0);
}
"#;

/// Per-entry shader uniform (must match the WGSL `U` layout).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    offset: [f32; 2],
    size: [f32; 2],
    flip_x: f32,
    flip_y: f32,
    swap: f32,
    _pad: f32,
    target: [f32; 2],
}

/// One texture slot in the pipeline.
#[derive(Debug)]
struct MediaEntry {
    texture: wgpu::Texture,
    bg0: wgpu::BindGroup,
    /// Shader uniform buffer (rect + device flags), updated each frame.
    uniforms: wgpu::Buffer,
    width: u32,
    height: u32,
    stride: u32,
    /// Destination rect in physical pixels (set by `prepare` every
    /// frame); used for the scissor in `draw`.
    rect: Rectangle,
    alive: Arc<AtomicBool>,
}

/// Shared per-renderer wgpu state.
#[derive(Debug)]
pub struct MediaPipeline {
    pipeline: wgpu::RenderPipeline,
    bg0_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Device sampling flags, probed once at creation:
    /// (flip_x, flip_y, swap) — 0.0 or 1.0.
    flags: (f32, f32, f32),
    entries: BTreeMap<u64, MediaEntry>,
}

impl MediaPipeline {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("media-browser media blit shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bg0_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("media-browser media blit bind group 0 layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("media-browser media blit pipeline layout"),
            bind_group_layouts: &[&bg0_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("media-browser media blit pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("media-browser media blit sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 1.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        let flags = Self::probe_flags(device, queue, &pipeline, &bg0_layout, &sampler, format);

        Self {
            pipeline,
            bg0_layout,
            sampler,
            flags,
            entries: BTreeMap::new(),
        }
    }

    /// Render a 2x2 source texture (distinct colored texels) onto an 8x8
    /// target with the neutral flags, read the quadrants back, and derive
    /// the flip/swap compensation for this device.
    fn probe_flags(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::RenderPipeline,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        format: wgpu::TextureFormat,
    ) -> (f32, f32, f32) {
        // 2x2 BGRA source: R, G / B, W (row 0 = top).
        let src = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("media-browser probe source"),
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let pixels: [u8; 512] = {
            let mut p = [0u8; 512];
            // 2x2 BGRA source: R, G / B, W (row 0 = top), 256-byte pitch.
            let rows: [[u8; 8]; 2] = [
                [0, 0, 255, 255, 0, 255, 0, 255], // R, G
                [255, 0, 0, 255, 255, 255, 255, 255], // B, W
            ];
            for (y, row) in rows.iter().enumerate() {
                p[y * 256..y * 256 + 8].copy_from_slice(row);
            }
            p
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &src,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
            wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
        );

        let dst = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("media-browser probe target"),
            size: wgpu::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let dst_view = dst.create_view(&Default::default());

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("media-browser probe uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniforms, 0, bytemuck::bytes_of(&Uniforms {
            offset: [0.0, 0.0],
            size: [8.0, 8.0],
            flip_x: 0.0,
            flip_y: 0.0,
            swap: 0.0,
            _pad: 0.0,
            target: [8.0, 8.0],
        }));
        let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("media-browser probe bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src.create_view(&Default::default())),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniforms,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });

        {
            let mut enc = device.create_command_encoder(&Default::default());
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.5,
                            b: 0.5,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(0.0, 0.0, 8.0, 8.0, 0.0, 1.0);
            pass.set_scissor_rect(0, 0, 8, 8);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg0, &[]);
            pass.draw(0..4, 0..1);
            drop(pass);
            queue.submit([enc.finish()]);
        }
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("device poll");

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 8 * 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &dst,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(8),
                },
            },
            wgpu::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);
        staging.map_async(wgpu::MapMode::Read, .., |res| {
            res.expect("map probe buffer")
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("device poll");
        let data = staging.get_mapped_range(..);

        let px = |x: u32, y: u32| {
            let o = (y * 256 + (x * 4)) as usize;
            (data[o], data[o + 1], data[o + 2])
        };
        let quadrants: [(u8, u8, u8); 4] = [px(2, 2), px(6, 2), px(2, 6), px(6, 6)];
        // Quadrant positions: 0 = TL, 1 = TR, 2 = BL, 3 = BR.

        // W (all channels high) is unambiguous; the R/B/G texels each
        // light exactly one channel. The W texel starts at the
        // bottom-right of the source (1,1), so its observed quadrant
        // position directly encodes the flips.
        let is_w = |c: (u8, u8, u8)| c.0 > 200 && c.1 > 200 && c.2 > 200;
        let Some(w_pos) = quadrants.iter().position(|c| is_w(*c)) else {
            log::warn!("[media] probe failed: W texel not found; using defaults");
            return (0.0, 0.0, 1.0);
        };
        // The probe ran with neutral flags, so the observed position of
        // W (source bottom-right) encodes the device's natural flips.
        // The shader flips that compensate:
        //   W at BR (3) -> none;  W at BL (2) -> flip_x;
        //   W at TR (1) -> flip_y; W at TL (0) -> both.
        let flip_x = if w_pos % 2 == 0 { 1.0 } else { 0.0 };
        let flip_y = if w_pos < 2 { 1.0 } else { 0.0 };

        // Channel order: locate the R texel (top-left of the source).
        let r_pos = (if flip_x > 0.5 { 1 } else { 0 })
            + (if flip_y > 0.5 { 2 } else { 0 });
        let r_quad = quadrants[r_pos];
        // R source bytes are (b=0, g=0, r=255). If the device presents
        // the red byte in .r already, no swap; if it lands in .b, swap.
        let swap = if r_quad.2 > 200 { 1.0 } else { 0.0 };

        log::info!(
            "[media] probed device sampling: flip_x={flip_x} flip_y={flip_y} swap={swap}"
        );
        (flip_x, flip_y, swap)
    }

    /// Ensure a texture slot exists for `id` with the given size/stride.
    /// Rebuilds the slot (texture + view + bind group) when the size changes.
    fn entry_for(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: u64,
        alive: &Arc<AtomicBool>,
        size: (u32, u32, u32),
    ) -> &MediaEntry {
        if let Some(e) = self.entries.get(&id) {
            if (e.width, e.height, e.stride) != size {
                let old = self.entries.remove(&id).expect("checked above");
                old.texture.destroy();
            }
        }
        if let Entry::Vacant(entry) = self.entries.entry(id) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("media-browser media texture"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("media-browser media texture view"),
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
                usage: None,
            });
            let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("media-browser media uniforms"),
                size: std::mem::size_of::<Uniforms>() as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniforms, 0, bytemuck::bytes_of(&Uniforms {
                offset: [0.0; 2],
                size: [1.0; 2],
                flip_x: self.flags.0,
                flip_y: self.flags.1,
                swap: self.flags.2,
                _pad: 0.0,
                target: [1.0, 1.0],
            }));
            let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("media-browser media blit bind group"),
                layout: &self.bg0_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniforms,
                            offset: 0,
                            size: None,
                        }),
                    },
                ],
            });
            entry.insert(MediaEntry {
                texture: texture.clone(),
                bg0,
                uniforms,
                width: size.0,
                height: size.1,
                stride: size.2,
                rect: Rectangle::new(Point::ORIGIN, Size::ZERO),
                alive: Arc::clone(alive),
            });
        }
        self.entries.get_mut(&id).expect("entry present")
    }

    /// Upload a BGRA frame (bytes per row = `stride`) into `id`'s texture.
    fn upload(
        &mut self,
        queue: &wgpu::Queue,
        id: u64,
        (width, height, stride): (u32, u32, u32),
        bytes: &[u8],
    ) {
        let e = self.entries.get(&id).expect("entry created in entry_for");
        let needed = (stride as u64 * height as u64) as usize;
        if bytes.len() < needed {
            return;
        }
        // wgpu requires a 256-byte row pitch for texture copies; repack
        // frames whose native stride is not aligned.
        let pitch = stride.div_ceil(256) * 256;
        let repacked = if pitch != stride {
            let mut buf = vec![0u8; (pitch as u64 * height as u64) as usize];
            for y in 0..height {
                let s = (y as usize) * stride as usize;
                let d = (y as usize) * pitch as usize;
                buf[d..d + stride as usize]
                    .copy_from_slice(&bytes[s..s + stride as usize]);
            }
            Some(buf)
        } else {
            None
        };
        let (src, pitch) = match repacked.as_ref() {
            Some(buf) => (buf.as_slice(), pitch),
            None => (bytes, stride),
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &e.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            src,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Drop entries whose owner signalled it dead.
    fn trim_entries(&mut self) {
        let dead: Vec<u64> = self
            .entries
            .iter()
            .filter(|(_, e)| !e.alive.load(Ordering::SeqCst))
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            if let Some(e) = self.entries.remove(&id) {
                e.texture.destroy();
            }
        }
    }

    /// Store the destination rect (physical pixels) and the target size
    /// for `id` in the shader uniform; remember the rect for the scissor.
    fn set_rect(
        &mut self,
        queue: &wgpu::Queue,
        id: u64,
        rect: Rectangle,
        target: (f32, f32),
    ) {
        if let Some(e) = self.entries.get_mut(&id) {
            e.rect = rect;
            queue.write_buffer(&e.uniforms, 0, bytemuck::bytes_of(&Uniforms {
                offset: [rect.x, rect.y],
                size: [rect.width, rect.height],
                flip_x: self.flags.0,
                flip_y: self.flags.1,
                swap: self.flags.2,
                _pad: 0.0,
                target: [target.0, target.1],
            }));
        }
    }

    /// Blit `id`'s texture into the target view, placed at the entry's
    /// `rect` (physical pixels) via clip-space vertex positions and the
    /// scissor test; the viewport spans the whole target.
    fn draw(&self, id: u64, target: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder) {
        let Some(e) = self.entries.get(&id) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("media-browser media blit render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // Vertex positions are emitted in physical pixel coordinates
        // (see the shader); the viewport maps them 1:1 onto the target.
        let (x, y, w, h) = (
            e.rect.x.round().max(0.0),
            e.rect.y.round().max(0.0),
            e.rect.width.max(1.0),
            e.rect.height.max(1.0),
        );
        let t = target.texture();
        pass.set_viewport(0.0, 0.0, t.width() as f32, t.height() as f32, 0.0, 1.0);
        pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &e.bg0, &[]);
        pass.draw(0..4, 0..1);
    }
}

impl Pipeline for MediaPipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self::new(device, queue, format)
    }

    fn trim(&mut self) {
        self.trim_entries();
    }
}

/// A blit of one BGRA frame at `bounds` (letterbox rect computed by the
/// widget). Carries the frame payload so the pipeline can upload it.
#[derive(Debug, Clone)]
pub struct MediaPrimitive {
    /// Owner id (unique per video tab / player instance).
    pub id: u64,
    /// Set to `false` to free the pipeline slot (e.g. on tab close).
    pub alive: Arc<AtomicBool>,
    /// BGRA pixel data, `stride * height` bytes.
    pub frame: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    /// Bytes per row of `frame`.
    pub stride: u32,
    /// `true` if this frame has not been uploaded yet.
    pub new_frame: bool,
}

impl Primitive for MediaPrimitive {
    type Pipeline = MediaPipeline;

    fn prepare(
        &self,
        pipeline: &mut MediaPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let size = (self.width, self.height, self.stride);
        pipeline.entry_for(device, queue, self.id, &self.alive, size);
        if self.new_frame {
            pipeline.upload(queue, self.id, size, &self.frame);
            log::debug!(
                "[media] prepare: id={} uploaded {}x{} ({} bytes)",
                self.id,
                self.width,
                self.height,
                self.frame.len()
            );
        }
        // `bounds` is in logical pixels; the shader works in physical
        // pixels, so map through the scale factor.
        let scale = viewport.scale_factor() as f32;
        let phys = viewport.physical_size();
        pipeline.set_rect(
            queue,
            self.id,
            Rectangle::new(
                Point::new(bounds.x * scale, bounds.y * scale),
                Size::new(bounds.width * scale, bounds.height * scale),
            ),
            (phys.width as f32, phys.height as f32),
        );
        pipeline.trim_entries();
    }

    fn render(
        &self,
        pipeline: &MediaPipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        _clip_bounds: &Rectangle<u32>,
    ) {
        log::debug!("[media] render: id={} into {}x{}",
            self.id,
            target.texture().width(),
            target.texture().height());
        pipeline.draw(self.id, target, encoder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_contract() {
        // 4-vertex triangle strip in physical pixel space.
        assert!(SHADER.contains("fn vs_main"));
        assert!(SHADER.contains("fn fs_main"));
        assert!(SHADER.contains("array<vec2<f32>, 4>"));
        assert!(SHADER.contains("uv * u.size + u.offset"));
        // Flag-driven orientation / channel compensation.
        assert!(SHADER.contains("u.flip_x"));
        assert!(SHADER.contains("u.flip_y"));
        assert!(SHADER.contains("u.swap"));
        assert!(SHADER.contains("c.b, c.g, c.r"));
    }

    #[test]
    fn primitive_clones_share_frame() {
        let frame = Arc::new(vec![0xABu8; 64]);
        let a = MediaPrimitive {
            id: 1,
            alive: Arc::new(AtomicBool::new(true)),
            frame: Arc::clone(&frame),
            width: 8,
            height: 1,
            stride: 32,
            new_frame: true,
        };
        let b = a.clone();
        assert!(Arc::ptr_eq(&a.frame, &b.frame));
        assert_eq!(a.stride, 32);
    }

    /// Headless round trip: upload a known 4x2 BGRA frame through
    /// [`MediaPrimitive::prepare`], blit it at a scaled destination rect,
    /// and read the target back. Verifies shader compilation, color order,
    /// orientation (via the probe), placement, and scissor clipping.
    #[tokio::test]
    async fn headless_blit_roundtrip() {
        for backend in [wgpu::Backends::VULKAN, wgpu::Backends::GL] {
            if let Err(e) = run_blit(backend).await {
                eprintln!("[media] backend {:?} unavailable or failing: {e:?}", backend);
            }
        }
    }

    async fn run_blit(backend: wgpu::Backends) -> Result<(), String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: backend,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| format!("no adapter for {backend:?}: {e}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| format!("device: {e}"))?;

        // 4x2 frame with distinct corners (exactly representable values):
        // TL red, TR blue, BL yellow, BR cyan.
        let (w, h) = (4u32, 2u32);
        let stride = w * 4;
        let mut frame = vec![0u8; (stride * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let o = (y * stride + x * 4) as usize;
                frame[o] = u8::from(x == 3) * 255; // b: right column
                frame[o + 1] = u8::from(y == 1) * 255; // g: bottom row
                frame[o + 2] = u8::from(x == 0) * 255; // r: left column
                frame[o + 3] = 255;
            }
        }

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&Default::default());

        // Clear the target to black so any miss is visible.
        {
            let mut enc = device.create_command_encoder(&Default::default());
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(0.0, 0.0, 128.0, 128.0, 0.0, 1.0);
            drop(pass);
            queue.submit([enc.finish()]);
        }

        // Destination rect (logical) 10,20 40x20 at scale 2.0 ->
        // physical 20,40 80x40. The 4x2 frame is stretched 20px per texel.
        let alive = Arc::new(AtomicBool::new(true));
        let prim = MediaPrimitive {
            id: 7,
            alive: Arc::clone(&alive),
            frame: Arc::new(frame),
            width: w,
            height: h,
            stride,
            new_frame: true,
        };
        let mut pipeline = MediaPipeline::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let viewport =
            Viewport::with_physical_size(Size::new(128, 128), 2.0);
        prim.prepare(
            &mut pipeline,
            &device,
            &queue,
            &Rectangle::new(Point::new(10.0, 20.0), Size::new(40.0, 20.0)),
            &viewport,
        );
        let mut enc = device.create_command_encoder(&Default::default());
        pipeline.draw(7, &target_view, &mut enc);
        queue.submit([enc.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("device poll");

        // Read the target back.
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 128 * 128 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(128 * 4),
                    rows_per_image: Some(128),
                },
            },
            wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);
        staging.map_async(
            wgpu::MapMode::Read,
            ..,
            |res| res.expect("map staging buffer"),
        );
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("device poll");
        let data = staging.get_mapped_range(..);
        let px = |x: u32, y: u32| {
            let o = ((y * 128 + x) * 4) as usize;
            (data[o], data[o + 1], data[o + 2], data[o + 3])
        };

        // Destination rect is physical (20,40) 80x40; the 4x2 frame
        // stretches to 20px per texel. Rasterization places vertices at
        // ~1px pixel offsets, so texel-center samples bleed a few units
        // into neighbours: assert identity (which channels are lit) and
        // near-exact values, while outside-rect stays exact.
        // Outside the rect: untouched black.
        assert_eq!(px(5, 5), (0, 0, 0, 255), "outside rect (top-left)");
        assert_eq!(px(120, 120), (0, 0, 0, 255), "outside rect (bottom-right)");
        assert_eq!(px(19, 60), (0, 0, 0, 255), "just left of rect");
        assert_eq!(px(101, 60), (0, 0, 0, 255), "just right of rect");
        assert_eq!(px(30, 39), (0, 0, 0, 255), "just above rect");
        assert_eq!(px(30, 81), (0, 0, 0, 255), "just below rect");

        // Frame TL red:
        let (r, g, b, _) = px(30, 50);
        assert!(r > 245, "TL red r={r}");
        assert!(g < 10 && b < 10, "TL bleed ({g},{b})");
        // Frame TR blue:
        let (r, g, b, _) = px(90, 50);
        assert!(b > 245, "TR blue b={b}");
        assert!(r < 10 && g < 10, "TR bleed ({r},{g})");
        // Frame BL yellow:
        let (r, g, b, _) = px(30, 70);
        assert!(r > 245 && g > 245, "BL yellow ({r},{g})");
        assert!(b < 10, "BL bleed b={b}");
        // Frame BR cyan:
        let (r, g, b, _) = px(90, 70);
        assert!(g > 245 && b > 245, "BR cyan ({g},{b})");
        assert!(r < 10, "BR bleed r={r}");
        Ok(())
    }
}
