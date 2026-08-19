//! Fullscreen camera/person-matte compositor drawn *after* the avatar with
//! destination-over blending. Existing opaque avatar pixels win; the pass
//! fills only transparent scene pixels, which is equivalent to an underlay
//! without changing the Pocket renderer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};
use pocket3d::gpu::Gpu;

use crate::frame_share::VideoFrame;

const COMPOSITOR_WGSL: &str = r#"
struct Params {
    mode: u32,
    has_clean: u32,
    has_mask: u32,
    _pad0: u32,
    time: f32,
    mask_texel_x: f32,
    mask_texel_y: f32,
    _pad1: f32,
};

struct VsOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f };

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(i % 2u) * 4 - 1);
    let y = f32(i32(i / 2u) * 4 - 1);
    out.pos = vec4f(x, y, 0.0, 1.0);
    out.uv = vec2f((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@group(0) @binding(0) var camera_tex: texture_2d<f32>;
@group(0) @binding(1) var clean_tex: texture_2d<f32>;
@group(0) @binding(2) var mask_tex: texture_2d<f32>;
@group(0) @binding(3) var linear_sampler: sampler;
@group(0) @binding(4) var<uniform> params: Params;

fn comic_background(uv: vec2f) -> vec3f {
    let center = distance(uv, vec2f(0.5, 0.48));
    let pulse = 0.04 * sin(params.time * 0.8 + center * 18.0);
    let top = vec3f(0.035, 0.055, 0.11);
    let bottom = vec3f(0.22, 0.025, 0.07);
    var color = mix(top, bottom, clamp(uv.y + pulse, 0.0, 1.0));
    let grid = step(0.88, fract(uv.x * 80.0)) * step(0.88, fract(uv.y * 45.0));
    color += vec3f(0.12, 0.03, 0.08) * grid;
    return color;
}

fn feathered_mask(uv: vec2f) -> f32 {
    if params.has_mask == 0u { return 0.0; }
    let dx = vec2f(params.mask_texel_x, 0.0);
    let dy = vec2f(0.0, params.mask_texel_y);
    var mask = textureSample(mask_tex, linear_sampler, uv).r * 0.4;
    mask += textureSample(mask_tex, linear_sampler, uv + dx).r * 0.15;
    mask += textureSample(mask_tex, linear_sampler, uv - dx).r * 0.15;
    mask += textureSample(mask_tex, linear_sampler, uv + dy).r * 0.15;
    mask += textureSample(mask_tex, linear_sampler, uv - dy).r * 0.15;
    return smoothstep(0.12, 0.88, mask);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
    let camera = textureSample(camera_tex, linear_sampler, in.uv).rgb;
    let comic = comic_background(in.uv);
    let person = feathered_mask(in.uv);
    var color = comic;
    if params.mode == 1u {
        color = camera;
    } else if params.mode == 2u {
        // Keep the real room, replace the person with a deterministic local
        // comic background. This needs no clean plate and tolerates camera movement.
        color = mix(camera, comic, person);
    } else if params.mode == 3u {
        let replacement = select(comic, textureSample(clean_tex, linear_sampler, in.uv).rgb, params.has_clean != 0u);
        color = mix(camera, replacement, person);
    }
    return vec4f(color, 1.0);
}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BackgroundMode {
    Transparent = 0xffff_ffff,
    Virtual = 0,
    Camera = 1,
    MatteVirtual = 2,
    CleanPlate = 3,
}

impl BackgroundMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "transparent" => Some(Self::Transparent),
            "virtual" => Some(Self::Virtual),
            "camera" => Some(Self::Camera),
            "matte" => Some(Self::MatteVirtual),
            "clean" => Some(Self::CleanPlate),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CompositorConfig {
    pub mode: BackgroundMode,
    pub clean_plate_delay: Duration,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    mode: u32,
    has_clean: u32,
    has_mask: u32,
    _pad0: u32,
    time: f32,
    mask_texel_x: f32,
    mask_texel_y: f32,
    _pad1: f32,
}

struct TextureResource {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct Resources {
    camera: TextureResource,
    clean: TextureResource,
    mask: TextureResource,
    bind: wgpu::BindGroup,
    camera_size: (u32, u32),
    mask_size: (u32, u32),
}

pub struct VideoCompositor {
    cfg: CompositorConfig,
    started: Instant,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    resources: Resources,
    last_sequence: u64,
    clean_captured: bool,
}

impl VideoCompositor {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, cfg: CompositorConfig) -> Self {
        let layout = bind_group_layout(gpu);
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pocket-live compositor sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pocket-live compositor params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("pocket-live compositor"),
                source: wgpu::ShaderSource::Wgsl(COMPOSITOR_WGSL.into()),
            });
        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pocket-live compositor layout"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
        let destination_over = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("pocket-live compositor pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(destination_over),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
        let resources = create_resources(gpu, &layout, &sampler, &params, (1, 1), (1, 1));
        gpu.queue.write_texture(
            resources.camera.texture.as_image_copy(),
            &[0, 0, 0, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        Self {
            cfg,
            started: Instant::now(),
            pipeline,
            layout,
            sampler,
            params,
            resources,
            last_sequence: 0,
            clean_captured: false,
        }
    }

    pub fn update(&mut self, gpu: &Gpu, frame: Arc<VideoFrame>) {
        if frame.sequence <= self.last_sequence {
            return;
        }
        log::trace!(
            "compositor video sequence={} captured_at_ns={}",
            frame.sequence,
            frame.captured_at_ns
        );
        let camera_size = (frame.width, frame.height);
        let mask_size = if frame.person_mask.is_empty() {
            (1, 1)
        } else {
            (frame.mask_width, frame.mask_height)
        };
        if self.resources.camera_size != camera_size || self.resources.mask_size != mask_size {
            self.resources = create_resources(
                gpu,
                &self.layout,
                &self.sampler,
                &self.params,
                camera_size,
                mask_size,
            );
            self.clean_captured = false;
        }
        upload(
            gpu,
            &self.resources.camera.texture,
            camera_size,
            4,
            &frame.bgra,
        );
        if !frame.person_mask.is_empty() {
            upload(
                gpu,
                &self.resources.mask.texture,
                mask_size,
                1,
                &frame.person_mask,
            );
        }
        if self.cfg.mode == BackgroundMode::CleanPlate
            && !self.clean_captured
            && self.started.elapsed() >= self.cfg.clean_plate_delay
        {
            upload(
                gpu,
                &self.resources.clean.texture,
                camera_size,
                4,
                &frame.bgra,
            );
            self.clean_captured = true;
            log::info!(
                "clean plate captured from video sequence {}",
                frame.sequence
            );
        }
        self.last_sequence = frame.sequence;
    }

    pub fn draw(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        time: f32,
    ) {
        let params = Params {
            mode: self.cfg.mode as u32,
            has_clean: u32::from(self.clean_captured),
            has_mask: u32::from(self.resources.mask_size != (1, 1)),
            _pad0: 0,
            time,
            mask_texel_x: 1.0 / self.resources.mask_size.0 as f32,
            mask_texel_y: 1.0 / self.resources.mask_size.1 as f32,
            _pad1: 0.0,
        };
        gpu.queue
            .write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pocket-live destination-over compositor"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.resources.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn bind_group_layout(gpu: &Gpu) -> wgpu::BindGroupLayout {
    gpu.device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pocket-live compositor bgl"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                texture_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn create_resources(
    gpu: &Gpu,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    params: &wgpu::Buffer,
    camera_size: (u32, u32),
    mask_size: (u32, u32),
) -> Resources {
    let camera = create_texture(
        gpu,
        "pocket-live camera",
        camera_size,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let clean = create_texture(
        gpu,
        "pocket-live clean plate",
        camera_size,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let mask = create_texture(
        gpu,
        "pocket-live person mask",
        mask_size,
        wgpu::TextureFormat::R8Unorm,
    );
    let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pocket-live compositor bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&camera.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&clean.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&mask.view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: params.as_entire_binding(),
            },
        ],
    });
    Resources {
        camera,
        clean,
        mask,
        bind,
        camera_size,
        mask_size,
    }
}

fn create_texture(
    gpu: &Gpu,
    label: &str,
    size: (u32, u32),
    format: wgpu::TextureFormat,
) -> TextureResource {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    TextureResource { texture, view }
}

fn upload(gpu: &Gpu, texture: &wgpu::Texture, size: (u32, u32), channels: u32, bytes: &[u8]) {
    debug_assert_eq!(bytes.len(), (size.0 * size.1 * channels) as usize);
    gpu.queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size.0 * channels),
            rows_per_image: Some(size.1),
        },
        wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_public_background_mode() {
        assert_eq!(
            BackgroundMode::parse("transparent"),
            Some(BackgroundMode::Transparent)
        );
        assert_eq!(
            BackgroundMode::parse("virtual"),
            Some(BackgroundMode::Virtual)
        );
        assert_eq!(
            BackgroundMode::parse("camera"),
            Some(BackgroundMode::Camera)
        );
        assert_eq!(
            BackgroundMode::parse("matte"),
            Some(BackgroundMode::MatteVirtual)
        );
        assert_eq!(
            BackgroundMode::parse("clean"),
            Some(BackgroundMode::CleanPlate)
        );
        assert_eq!(BackgroundMode::parse("remote"), None);
    }
}
