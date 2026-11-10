//! Environnement d'éclairage sur le GPU Vulkan (D-33) : pyramide préfiltrée RGBA16F et son jeu de descripteurs
//! (ensemble 0, liaison 0, lu par les nuanceurs de la scène et du fond).

use obv_gpu::{Buffer, Gpu, Image, MemoryLocation, Result, vk};

use crate::light::Environment;

pub struct EnvBinding {
    gpu: Gpu,
    pub layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    pub set: vk::DescriptorSet,
    sampler: vk::Sampler,
    image: Option<Image>,
}

impl EnvBinding {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let d = gpu.device();
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        let layout = unsafe {
            d.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings), None)?
        };
        let sizes = [vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 1 }];
        let pool = unsafe {
            d.create_descriptor_pool(&vk::DescriptorPoolCreateInfo::default().max_sets(1).pool_sizes(&sizes), None)?
        };
        let layouts = [layout];
        let set = unsafe {
            d.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts),
            )?[0]
        };
        // Longitude bouclée, latitude bornée ; niveaux interpolés (flou continu du fond).
        let sampler = unsafe {
            d.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::REPEAT)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .max_lod(vk::LOD_CLAMP_NONE),
                None,
            )?
        };
        let mut b = EnvBinding { gpu: gpu.clone(), layout, pool, set, sampler, image: None };
        // Avant le premier environnement : un pixel noir (pas de reflet, éclairage ambiant du bloc neutre).
        b.upload(&[(1, 1, vec![0u16, 0, 0, 0x3c00])])?;
        Ok(b)
    }

    /// Remplace la pyramide (l'appareil est attendu : l'ancienne image peut servir à des images en cours).
    pub fn set(&mut self, env: &Environment) -> Result<()> {
        self.upload(&env.levels)
    }

    fn upload(&mut self, levels: &[(u32, u32, Vec<u16>)]) -> Result<()> {
        let d = self.gpu.device();
        let (w, h) = (levels[0].0, levels[0].1);
        let image = Image::with_mips(
            &self.gpu,
            "environnement",
            vk::Format::R16G16B16A16_SFLOAT,
            vk::Extent2D { width: w, height: h },
            levels.len() as u32,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::ImageAspectFlags::COLOR,
        )?;
        let total: usize = levels.iter().map(|l| l.2.len() * 2).sum();
        let mut staging = Buffer::new(
            &self.gpu,
            "envoi de l'environnement",
            total as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        let mut regions = Vec::with_capacity(levels.len());
        {
            let map = staging.mapped_mut().expect("tampon d'envoi non projeté");
            let mut offset = 0usize;
            for (k, (lw, lh, data)) in levels.iter().enumerate() {
                let bytes: &[u8] = bytemuck::cast_slice(data);
                map[offset..offset + bytes.len()].copy_from_slice(bytes);
                regions.push(
                    vk::BufferImageCopy::default()
                        .buffer_offset(offset as u64)
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .mip_level(k as u32)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D { width: *lw, height: *lh, depth: 1 }),
                );
                offset += bytes.len();
            }
        }
        self.gpu.submit_and_wait(|cmd| {
            image.barrier(
                cmd,
                (vk::PipelineStageFlags2::TOP_OF_PIPE, vk::AccessFlags2::NONE, vk::ImageLayout::UNDEFINED),
                (
                    vk::PipelineStageFlags2::COPY,
                    vk::AccessFlags2::TRANSFER_WRITE,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                ),
            );
            unsafe {
                d.cmd_copy_buffer_to_image(cmd, staging.raw, image.raw, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &regions)
            };
            image.barrier(
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
        })?;
        // Le jeu de descripteurs peut être lié à des images encore en vol : on attend l'appareil.
        unsafe { d.device_wait_idle()? };
        let info = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(image.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        unsafe {
            d.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(self.set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&info)],
                &[],
            )
        };
        self.image = Some(image);
        Ok(())
    }
}

impl Drop for EnvBinding {
    fn drop(&mut self) {
        let d = self.gpu.device();
        self.image = None;
        unsafe {
            d.destroy_sampler(self.sampler, None);
            d.destroy_descriptor_pool(self.pool, None);
            d.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}
