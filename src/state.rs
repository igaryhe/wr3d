use wgpu;
use wgpu::util::DeviceExt;
use winit::{
    event::*,
    window::*,
};
use anyhow::{Context, Result};
use bytemuck;
use tobj::*;
use crate::camera::Camera;
use glam::{vec3, Vec3};
use image;

pub struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    sc_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    pub size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    pos_buffer: wgpu::Buffer,
    norm_buffer: wgpu::Buffer,
    tex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    uniform_bind_group: wgpu::BindGroup,
    diffuse_bind_group: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
}

impl Uniforms {
    fn new(camera: &Camera) -> Self {
        Uniforms {
            view_proj: camera.build_view_projection_matrix().to_cols_array_2d()
        }
    }
}


impl State {
    pub async fn new(window: &Window) -> Result<Self> {
        let size = window.inner_size();

        // initializing GPU
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::Default,
                compatible_surface: Some(&surface),
            }
        ).await.context("Failed to create adapter")?;
        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
                shader_validation: true,
            },
            None,
        ).await?;
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };

        // load shaders
        let vs_module = device.create_shader_module(
            wgpu::include_spirv!("shader.vert.spv"));
        let fs_module = device.create_shader_module(
            wgpu::include_spirv!("shader.frag.spv"));

        // load models and materials
        let (models, materials) = load_obj("data/cube.obj", true)?;
        let mesh = &models[0].mesh;
        use image::io::Reader as ImageReader;
        use std::path::Path;
        let diffuse_image = ImageReader::open(Path::new("data/").join(materials[0].diffuse_texture.as_str()))?.decode()?;
        let diffuse_rgba = diffuse_image.into_rgba8();
        let dimensions = diffuse_rgba.dimensions();

        // creating textures
        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth: 1,
        };
        let diffuse_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
            label: Some("Diffuse Texture"),
        });
        queue.write_texture(
            wgpu::TextureCopyView {
                texture: &diffuse_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            &diffuse_rgba,
            wgpu::TextureDataLayout {
                offset: 0,
                bytes_per_row: 4 * dimensions.0,
                rows_per_image: 4 * dimensions.1,
            },
            texture_size,
        );
        let diffuse_texture_view = diffuse_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let texture_bind_group_layout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::SampledTexture {
                            multisampled: false,
                            dimension: wgpu::TextureViewDimension::D2,
                            component_type: wgpu::TextureComponentType::Uint,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            comparison: false,
                        },
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout")
            }
        );
        let diffuse_bind_group = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                layout: &texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                    }
                ],
                label: Some("texture_bind_group")
            }
        );
        
        // setting up camera
        let camera = Camera {
            eye: vec3(0.0, 1.0, 2.0),
            center: Vec3::zero(),
            up: Vec3::unit_y(),
            aspect_ratio: sc_desc.width as f32 / sc_desc.height as f32,
            fov: 0.7,
            z_near: 0.1,
            z_far: 100.0,
        };

        // setting up uniform buffer
        let uniforms = Uniforms::new(&camera);
        let uniform_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            }
        );
        let uniform_bind_group_layout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::VERTEX,
                        ty: wgpu::BindingType::UniformBuffer {
                            dynamic: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
                label: Some("Uniform Bind Group Layout")
            }
        );
        let uniform_bind_group = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                layout: &uniform_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(uniform_buffer.slice(..)),
                    },
                ],
                label: Some("Uniform Bind Group"),
            }
        );

        // render pipeline
        let render_pipeline_layout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &uniform_bind_group_layout,
                    &texture_bind_group_layout,
                ],
                push_constant_ranges: &[],
            }
        );

        let render_pipeline = device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex_stage: wgpu::ProgrammableStageDescriptor {
                    module: &vs_module,
                    entry_point: "main",
                },
                fragment_stage: Some(
                    wgpu::ProgrammableStageDescriptor {
                        module: &fs_module,
                        entry_point: "main",
                    }),
                rasterization_state: Some(
                    wgpu::RasterizationStateDescriptor {
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: wgpu::CullMode::Back,
                        depth_bias: 0,
                        depth_bias_slope_scale: 0.0,
                        depth_bias_clamp: 0.0,
                        clamp_depth: false,
                    }
                ),
                color_states: &[
                    wgpu::ColorStateDescriptor {
                        format: sc_desc.format,
                        color_blend: wgpu::BlendDescriptor::REPLACE,
                        alpha_blend: wgpu::BlendDescriptor::REPLACE,
                        write_mask: wgpu::ColorWrite::ALL,
                    }
                ],
                primitive_topology: wgpu::PrimitiveTopology::TriangleList,
                depth_stencil_state: None,
                vertex_state: wgpu::VertexStateDescriptor {
                    index_format: wgpu::IndexFormat::Uint32,
                    vertex_buffers: &[
                        wgpu::VertexBufferDescriptor {
                            stride: 12,
                            step_mode: wgpu::InputStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float3],
                        },
                        wgpu::VertexBufferDescriptor {
                            stride: 12,
                            step_mode: wgpu::InputStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![1 => Float3],
                        },
                        wgpu::VertexBufferDescriptor {
                            stride: 8,
                            step_mode: wgpu::InputStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![2 => Float2],
                        }
                    ],
                },
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            }
        );
        
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        // creating buffers;
        let pos_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Positions Buffer"),
                contents: bytemuck::cast_slice(&mesh.positions[..]),
                usage: wgpu::BufferUsage::VERTEX,
            }
        );
        let norm_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Normals Buffer"),
                contents: bytemuck::cast_slice(&mesh.normals[..]),
                usage: wgpu::BufferUsage::VERTEX,
            }
        );
        let tex_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("TexCoords Buffer"),
                contents: bytemuck::cast_slice(&mesh.texcoords[..]),
                usage: wgpu::BufferUsage::VERTEX,
            }
        );
        let index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&mesh.indices[..]),
                usage: wgpu::BufferUsage::INDEX,
            }
        );
        let num_indices = mesh.indices.len() as u32;
        Ok(Self {
            surface,
            device,
            queue,
            sc_desc,
            swap_chain,
            size,
            render_pipeline,
            pos_buffer,
            norm_buffer,
            tex_buffer,
            index_buffer,
            num_indices,
            uniform_bind_group,
            diffuse_bind_group,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        self.sc_desc.width = new_size.width;
        self.sc_desc.height = new_size.height;
        self.swap_chain = self.device.create_swap_chain(&self.surface, &self.sc_desc);
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        false
    }

    pub fn update(&mut self) {
        
    }

    pub fn render(&mut self) -> Result<(), wgpu::SwapChainError> {
        let frame = self.swap_chain.get_current_frame()?.output;
        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") },
        );
        let mut render_pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[
                    wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &frame.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.2,
                                b: 0.3,
                                a: 1.0,
                            }),
                            store: true,
                        },
                    },
                ],
                depth_stencil_attachment: None,
            });
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, &self.diffuse_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.pos_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.norm_buffer.slice(..));
        render_pass.set_vertex_buffer(2, self.tex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..));
        render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        drop(render_pass);
        self.queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }
}

