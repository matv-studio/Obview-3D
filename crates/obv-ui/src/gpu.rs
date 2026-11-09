//! Dessin de la liste de primitives par Vulkan : un pipeline, un tracé instancié, un atlas 8 bits.

use obv_gpu::{Buffer, Gpu, GraphicsPipeline, Image, MemoryLocation, PipelineDesc, Result, ShaderStages, spirv, vk};

use crate::{Atlas, DrawList, paint::Prim};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Push {
    screen: [f32; 2],
    prims: u64,
}

struct Frame {
    prims: Option<Buffer>,
    staging: Vec<Buffer>,
    count: u32,
}

pub struct UiRenderer {
    gpu: Gpu,
    pipeline: GraphicsPipeline,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    sampler: vk::Sampler,
    atlas: Image,
    atlas_ready: bool,
    frames: Vec<Frame>,
    current: usize,
}

impl UiRenderer {
    /// `frames_in_flight` : nombre d'images préparées en avance (1 hors écran, 2 ou 3 avec une fenêtre).
    /// `depth_format` : format de profondeur de la passe où l'interface est dessinée (sans test de profondeur).
    pub fn new(
        gpu: &Gpu,
        color_format: vk::Format,
        depth_format: Option<vk::Format>,
        atlas_size: u32,
        frames_in_flight: usize,
    ) -> Result<Self> {
        let d = gpu.device();
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        let set_layout = unsafe {
            d.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings), None)?
        };
        let pipeline = GraphicsPipeline::new(
            gpu,
            &PipelineDesc {
                stages: ShaderStages::Vertex { vertex: spirv!("ui.vert"), fragment: spirv!("ui.frag") },
                color_formats: &[color_format],
                depth_format,
                depth_write: false,
                depth_compare: vk::CompareOp::ALWAYS,
                cull_mode: vk::CullModeFlags::NONE,
                alpha_blend: true,
                push_constant_size: std::mem::size_of::<Push>() as u32,
                set_layouts: &[set_layout],
            },
        )?;
        let sizes = [vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 1 }];
        let pool = unsafe {
            d.create_descriptor_pool(&vk::DescriptorPoolCreateInfo::default().max_sets(1).pool_sizes(&sizes), None)?
        };
        let layouts = [set_layout];
        let set = unsafe {
            d.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts),
            )?[0]
        };
        let sampler = unsafe {
            d.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )?
        };
        let atlas = Image::new(
            gpu,
            "atlas de l'interface",
            vk::Format::R8_UNORM,
            vk::Extent2D { width: atlas_size, height: atlas_size },
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::ImageAspectFlags::COLOR,
        )?;
        let info = [vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(atlas.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        unsafe {
            d.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&info)],
                &[],
            )
        };
        let frames =
            (0..frames_in_flight.max(1)).map(|_| Frame { prims: None, staging: Vec::new(), count: 0 }).collect();
        Ok(UiRenderer {
            gpu: gpu.clone(),
            pipeline,
            set_layout,
            pool,
            set,
            sampler,
            atlas,
            atlas_ready: false,
            frames,
            current: 0,
        })
    }

    /// À appeler hors d'une passe de rendu : envoie l'atlas modifié et les primitives de l'image `slot`.
    pub fn prepare(&mut self, cmd: vk::CommandBuffer, slot: usize, dl: &DrawList, atlas: &mut Atlas) -> Result<()> {
        self.current = slot % self.frames.len();
        let frame = &mut self.frames[self.current];
        frame.staging.clear();
        if let Some([x0, y0, x1, y1]) = atlas.dirty.take() {
            let (w, h) = (x1 - x0, y1 - y0);
            let mut data = Vec::with_capacity((w * h) as usize);
            for y in y0..y1 {
                let row = (y * atlas.size + x0) as usize;
                data.extend_from_slice(&atlas.pixels[row..row + w as usize]);
            }
            let staging = Buffer::with_data(&self.gpu, "envoi de l'atlas", &data, vk::BufferUsageFlags::TRANSFER_SRC)?;
            let from = if self.atlas_ready {
                (
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                )
            } else {
                (vk::PipelineStageFlags2::TOP_OF_PIPE, vk::AccessFlags2::NONE, vk::ImageLayout::UNDEFINED)
            };
            self.atlas.barrier(
                cmd,
                from,
                (
                    vk::PipelineStageFlags2::COPY,
                    vk::AccessFlags2::TRANSFER_WRITE,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                ),
            );
            if !self.atlas_ready {
                // Premier envoi : l'atlas entier, pour ne pas laisser de mémoire indéfinie.
                let full =
                    Buffer::with_data(&self.gpu, "atlas complet", &atlas.pixels, vk::BufferUsageFlags::TRANSFER_SRC)?;
                unsafe {
                    self.gpu.device().cmd_copy_buffer_to_image(
                        cmd,
                        full.raw,
                        self.atlas.raw,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[vk::BufferImageCopy::default()
                            .image_subresource(
                                vk::ImageSubresourceLayers::default()
                                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                                    .layer_count(1),
                            )
                            .image_extent(vk::Extent3D { width: atlas.size, height: atlas.size, depth: 1 })],
                    )
                };
                frame.staging.push(full);
            } else {
                unsafe {
                    self.gpu.device().cmd_copy_buffer_to_image(
                        cmd,
                        staging.raw,
                        self.atlas.raw,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[vk::BufferImageCopy::default()
                            .image_subresource(
                                vk::ImageSubresourceLayers::default()
                                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                                    .layer_count(1),
                            )
                            .image_offset(vk::Offset3D { x: x0 as i32, y: y0 as i32, z: 0 })
                            .image_extent(vk::Extent3D { width: w, height: h, depth: 1 })],
                    )
                };
            }
            frame.staging.push(staging);
            self.atlas.barrier(
                cmd,
                (
                    vk::PipelineStageFlags2::COPY,
                    vk::AccessFlags2::TRANSFER_WRITE,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                ),
                (
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ),
            );
            self.atlas_ready = true;
        }
        let bytes: &[u8] = bytemuck::cast_slice(&dl.prims);
        let needed = bytes.len().max(std::mem::size_of::<Prim>()) as u64;
        if frame.prims.as_ref().is_none_or(|b| b.size < needed) {
            let size = needed.next_power_of_two().max(64 * 1024);
            frame.prims = Some(Buffer::new(
                &self.gpu,
                "primitives de l'interface",
                size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::CpuToGpu,
            )?);
        }
        let buf = frame.prims.as_mut().unwrap();
        buf.mapped_mut().expect("tampon non projeté")[..bytes.len()].copy_from_slice(bytes);
        frame.count = dl.prims.len() as u32;
        Ok(())
    }

    /// À appeler dans la passe de rendu de la cible de taille `extent` (pixels de l'écran).
    pub fn draw(&self, cmd: vk::CommandBuffer, extent: vk::Extent2D) {
        let frame = &self.frames[self.current];
        if frame.count == 0 || !self.atlas_ready {
            return;
        }
        let d = self.gpu.device();
        unsafe {
            d.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            d.cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D::default(), extent }]);
            self.pipeline.bind(cmd);
            d.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline.layout, 0, &[self.set], &[]);
        }
        self.pipeline.push(
            cmd,
            &Push { screen: [extent.width as f32, extent.height as f32], prims: frame.prims.as_ref().unwrap().address },
        );
        unsafe { d.cmd_draw(cmd, 6, frame.count, 0, 0) };
    }
}

impl Drop for UiRenderer {
    fn drop(&mut self) {
        let d = self.gpu.device();
        unsafe {
            let _ = d.device_wait_idle();
            d.destroy_sampler(self.sampler, None);
            d.destroy_descriptor_pool(self.pool, None);
            d.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}
