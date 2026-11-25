//! Environnement d'éclairage sur le GPU Vulkan (D-33) : pyramide préfiltrée RGBA16F (ensemble 0, liaison 0, lue
//! par les nuanceurs de la scène et du fond) ; textures des matériaux (D-37) en tableau RGBA 8 bits (liaison 1).
//! Un jeu de descripteurs par document ouvert (D-41) : l'environnement est partagé, les textures sont propres à
//! chaque document.

use std::collections::HashMap;

use obv_gpu::{Buffer, Gpu, Image, MemoryLocation, Result, vk};

use crate::{light::Environment, materials::TextureArray};

/// Documents au plus (jeux de descripteurs de la réserve).
const MAX_DOCS: u32 = 64;

pub struct EnvBinding {
    gpu: Gpu,
    pub layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sampler: vk::Sampler,
    image: Option<Image>,
    tex_sampler: vk::Sampler,
    /// Jeu de descripteurs et textures de chaque document.
    sets: HashMap<u64, (vk::DescriptorSet, Option<Image>)>,
}

impl EnvBinding {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let d = gpu.device();
        let bindings = [0, 1].map(|b| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(b)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        });
        let layout = unsafe {
            d.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings), None)?
        };
        let sizes =
            [vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 2 * MAX_DOCS }];
        let pool = unsafe {
            d.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
                    .max_sets(MAX_DOCS)
                    .pool_sizes(&sizes),
                None,
            )?
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
        // Textures des matériaux : répétées (convention glTF), niveaux interpolés.
        let tex_sampler = unsafe {
            d.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::REPEAT)
                    .address_mode_v(vk::SamplerAddressMode::REPEAT)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .max_lod(vk::LOD_CLAMP_NONE),
                None,
            )?
        };
        let mut b =
            EnvBinding { gpu: gpu.clone(), layout, pool, sampler, image: None, tex_sampler, sets: HashMap::new() };
        // Avant le premier environnement : un pixel noir (pas de reflet, éclairage ambiant du bloc neutre).
        b.upload(&[(1, 1, vec![0u16, 0, 0, 0x3c00])])?;
        Ok(b)
    }

    /// Jeu de descripteurs du document `doc`, créé s'il manque (environnement courant, textures vides).
    pub fn ensure(&mut self, doc: u64) -> Result<()> {
        if self.sets.contains_key(&doc) {
            return Ok(());
        }
        let layouts = [self.layout];
        let set = unsafe {
            self.gpu.device().allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default().descriptor_pool(self.pool).set_layouts(&layouts),
            )?[0]
        };
        self.sets.insert(doc, (set, None));
        if let Some(image) = &self.image {
            self.write(set, 0, self.sampler, image.view);
        }
        self.set_textures(doc, &TextureArray::empty())
    }

    /// Jeu de descripteurs du document `doc` (créé par [`Self::ensure`]).
    pub fn set_of(&self, doc: u64) -> Option<vk::DescriptorSet> {
        self.sets.get(&doc).map(|s| s.0)
    }

    /// Libère le jeu et les textures du document `doc` (l'appareil est attendu).
    pub fn remove(&mut self, doc: u64) -> Result<()> {
        if let Some((set, image)) = self.sets.remove(&doc) {
            let d = self.gpu.device();
            unsafe {
                d.device_wait_idle()?;
                d.free_descriptor_sets(self.pool, &[set])?;
            }
            drop(image);
        }
        Ok(())
    }

    fn write(&self, set: vk::DescriptorSet, binding: u32, sampler: vk::Sampler, view: vk::ImageView) {
        let info = [vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        unsafe {
            self.gpu.device().update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(binding)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&info)],
                &[],
            )
        };
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
        // Les jeux de descripteurs peuvent être liés à des images encore en vol : on attend l'appareil.
        unsafe { d.device_wait_idle()? };
        for (set, _) in self.sets.values() {
            self.write(*set, 0, self.sampler, image.view);
        }
        self.image = Some(image);
        Ok(())
    }
}

impl EnvBinding {
    /// Remplace le tableau des textures des matériaux du document `doc` (l'appareil est attendu, comme pour
    /// l'environnement).
    pub fn set_textures(&mut self, doc: u64, t: &TextureArray) -> Result<()> {
        let Some(&(set, _)) = self.sets.get(&doc) else { return Ok(()) };
        let d = self.gpu.device();
        let layers = t.layers.len().max(1) as u32;
        let image = Image::array(
            &self.gpu,
            "textures des matériaux",
            vk::Format::R8G8B8A8_UNORM,
            vk::Extent2D { width: t.size, height: t.size },
            t.levels,
            layers,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::ImageAspectFlags::COLOR,
        )?;
        let total: usize = t.layers.iter().flatten().map(Vec::len).sum();
        let mut staging = Buffer::new(
            &self.gpu,
            "envoi des textures",
            total.max(4) as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        let mut regions = Vec::new();
        {
            let map = staging.mapped_mut().expect("tampon d'envoi non projeté");
            let mut offset = 0usize;
            for (layer, levels) in t.layers.iter().enumerate() {
                let mut side = t.size;
                for (level, data) in levels.iter().enumerate() {
                    map[offset..offset + data.len()].copy_from_slice(data);
                    regions.push(
                        vk::BufferImageCopy::default()
                            .buffer_offset(offset as u64)
                            .image_subresource(
                                vk::ImageSubresourceLayers::default()
                                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                                    .mip_level(level as u32)
                                    .base_array_layer(layer as u32)
                                    .layer_count(1),
                            )
                            .image_extent(vk::Extent3D { width: side, height: side, depth: 1 }),
                    );
                    offset += data.len();
                    side = (side / 2).max(1);
                }
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
        unsafe { d.device_wait_idle()? };
        self.write(set, 1, self.tex_sampler, image.view);
        if let Some(e) = self.sets.get_mut(&doc) {
            e.1 = Some(image);
        }
        Ok(())
    }
}

impl Drop for EnvBinding {
    fn drop(&mut self) {
        let d = self.gpu.device();
        self.image = None;
        self.sets.clear();
        unsafe {
            d.destroy_sampler(self.tex_sampler, None);
            d.destroy_sampler(self.sampler, None);
            d.destroy_descriptor_pool(self.pool, None);
            d.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}
