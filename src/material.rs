use crate::texture::Texture;
use wgpu;
use tobj;
use anyhow::Result;
use wgpu::util::DeviceExt;
use mint::Vector3;
use crevice::std140::{AsStd140, Std140};

pub struct Material {
    pub diffuse_texture: Texture,
    pub name: String,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl Material {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, material: &tobj::Material) -> Result<Self> {
        let path = format!("data/{}", material.diffuse_texture);
        let diffuse_texture = Texture::new(device, queue, &path,
                                           Some("diffuse_texture"))?;
        let name = material.name.as_str().to_string();
        let material_raw = MaterialRaw {
            ambient: Vector3::from_slice(&material.ambient),
            diffuse: Vector3::from_slice(&material.diffuse),
            specular: Vector3::from_slice(&material.specular),
            shininess: material.shininess,
        };
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(format!("{} uniform buffer", name).as_str()),
            contents: material_raw.as_std140().as_bytes(),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::UniformBuffer {
                            dynamic: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
                label: Some(format!("{} bind group layout", name).as_str()),
            }
        );
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(buffer.slice(..)),
            },],
            label: Some(format!("{} bind group", name).as_str()),
        });
        Ok(Self {
            diffuse_texture,
            name,
            bind_group_layout,
            bind_group,
        })
    }
}

#[derive(AsStd140)]
struct MaterialRaw {
    ambient: Vector3<f32>,
    diffuse: Vector3<f32>,
    specular: Vector3<f32>,
    shininess: f32,
}
