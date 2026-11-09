use ash::vk;

use crate::{Buffer, Gpu, Image, MemoryLocation, Result};

/// Cible de rendu hors écran (couleur sRGB + profondeur inversée), relue sur le processeur.
/// Sert aux essais, aux captures et aux vignettes (D-06).
pub struct RenderTarget {
    gpu: Gpu,
    pub color: Image,
    pub depth: Image,
    readback: Buffer,
}

pub const COLOR_FORMAT: vk::Format = vk::Format::R8G8B8A8_SRGB;
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

impl RenderTarget {
    pub const COLOR_FORMAT: vk::Format = COLOR_FORMAT;
    pub const DEPTH_FORMAT: vk::Format = DEPTH_FORMAT;

    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Result<Self> {
        let extent = vk::Extent2D { width, height };
        let color = Image::new(
            gpu,
            "cible couleur",
            COLOR_FORMAT,
            extent,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::ImageAspectFlags::COLOR,
        )?;
        let depth = Image::new(
            gpu,
            "cible profondeur",
            DEPTH_FORMAT,
            extent,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH,
        )?;
        let readback = Buffer::new(
            gpu,
            "relecture",
            (width * height * 4) as u64,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
        )?;
        Ok(Self { gpu: gpu.clone(), color, depth, readback })
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.color.extent
    }

    /// Rend une image : efface (couleur en espace linéaire, profondeur à 0), règle la fenêtre d'affichage
    /// (axe Y vers le haut), exécute `draw`, puis copie le résultat pour [`Self::read_rgba8`].
    pub fn render(&self, clear: [f32; 4], draw: impl FnOnce(vk::CommandBuffer)) -> Result<()> {
        self.render_with(clear, |_| {}, draw)
    }

    /// Comme [`Self::render`], avec `prepare` enregistré avant la passe de rendu (envois de données).
    pub fn render_with(
        &self,
        clear: [f32; 4],
        prepare: impl FnOnce(vk::CommandBuffer),
        draw: impl FnOnce(vk::CommandBuffer),
    ) -> Result<()> {
        let d = self.gpu.device().clone();
        let ext = self.extent();
        self.gpu.submit_and_wait(|cmd| unsafe {
            prepare(cmd);
            let none = (vk::PipelineStageFlags2::TOP_OF_PIPE, vk::AccessFlags2::NONE, vk::ImageLayout::UNDEFINED);
            self.color.barrier(
                cmd,
                none,
                (
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
            );
            self.depth.barrier(
                cmd,
                none,
                (
                    vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
                    vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                ),
            );
            let color = [vk::RenderingAttachmentInfo::default()
                .image_view(self.color.view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: clear } })];
            let depth = vk::RenderingAttachmentInfo::default()
                .image_view(self.depth.view)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 } });
            let area = vk::Rect2D { offset: vk::Offset2D::default(), extent: ext };
            d.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(area)
                    .layer_count(1)
                    .color_attachments(&color)
                    .depth_attachment(&depth),
            );
            // Fenêtre retournée : repère Y vers le haut, comme les matrices de projection habituelles.
            d.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: ext.height as f32,
                    width: ext.width as f32,
                    height: -(ext.height as f32),
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            d.cmd_set_scissor(cmd, 0, &[area]);
            draw(cmd);
            d.cmd_end_rendering(cmd);
            self.color.barrier(
                cmd,
                (
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
                (vk::PipelineStageFlags2::COPY, vk::AccessFlags2::TRANSFER_READ, vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
            );
            d.cmd_copy_image_to_buffer(
                cmd,
                self.color.raw,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.readback.raw,
                &[vk::BufferImageCopy::default()
                    .image_subresource(
                        vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1),
                    )
                    .image_extent(vk::Extent3D { width: ext.width, height: ext.height, depth: 1 })],
            );
            let host = [vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::HOST)
                .dst_access_mask(vk::AccessFlags2::HOST_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.readback.raw)
                .size(vk::WHOLE_SIZE)];
            d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().buffer_memory_barriers(&host));
        })
    }

    /// Pixels RGBA 8 bits (encodés sRGB), ligne par ligne depuis le haut.
    pub fn read_rgba8(&self) -> Vec<u8> {
        let ext = self.extent();
        self.readback.mapped().expect("tampon de relecture non projeté")[..(ext.width * ext.height * 4) as usize]
            .to_vec()
    }
}
