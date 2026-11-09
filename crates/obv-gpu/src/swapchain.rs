//! Présentation dans une fenêtre : surface, chaîne d'échange, images en vol, recréation au redimensionnement.

use std::ffi::CStr;

use ash::{khr, vk};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::{Gpu, GpuError, Image, Result};

/// Images préparées en parallèle par le processeur pendant que le GPU termine la précédente.
pub const FRAMES_IN_FLIGHT: usize = 2;

/// Extensions d'instance nécessaires pour créer une surface sur cet affichage.
pub fn required_instance_extensions(display: RawDisplayHandle) -> Result<Vec<&'static CStr>> {
    let exts = ash_window::enumerate_required_extensions(display)?;
    // SAFETY: ash_window renvoie des pointeurs vers des constantes statiques terminées par zéro.
    Ok(exts.iter().map(|p| unsafe { CStr::from_ptr(*p) }).collect())
}

struct FrameSync {
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    acquired: vk::Semaphore,
    fence: vk::Fence,
}

/// Mode de présentation souhaité.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentMode {
    /// Synchronisé sur l'écran, sans attente inutile si l'affichage le permet (MAILBOX), sinon FIFO.
    LowLatency,
    /// Synchronisé sur l'écran (FIFO) : économe en énergie.
    Vsync,
}

pub struct Swapchain {
    gpu: Gpu,
    surface_loader: khr::surface::Instance,
    surface: vk::SurfaceKHR,
    loader: khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    /// Un sémaphore « rendu terminé » par image de la chaîne (réutilisation sûre à la présentation).
    rendered: Vec<vk::Semaphore>,
    pub format: vk::Format,
    color_space: vk::ColorSpaceKHR,
    pub extent: vk::Extent2D,
    pub depth_format: vk::Format,
    depth: Option<Image>,
    frames: Vec<FrameSync>,
    frame: usize,
    mode: PresentMode,
    wanted: vk::Extent2D,
    stale: bool,
}

impl Swapchain {
    /// Crée la surface et la chaîne d'échange.
    ///
    /// # Safety
    /// Les poignées doivent désigner une fenêtre vivante qui survit à la chaîne d'échange.
    pub unsafe fn new(
        gpu: &Gpu,
        display: RawDisplayHandle,
        window: RawWindowHandle,
        size: (u32, u32),
        mode: PresentMode,
    ) -> Result<Self> {
        let inner = &gpu.0;
        let surface = unsafe { ash_window::create_surface(&inner.entry, &inner.instance, display, window, None)? };
        let surface_loader = khr::surface::Instance::new(&inner.entry, &inner.instance);
        let supported = unsafe {
            surface_loader.get_physical_device_surface_support(inner.physical_device, inner.queue_family, surface)?
        };
        if !supported {
            unsafe { surface_loader.destroy_surface(surface, None) };
            return Err(GpuError::NoDevice(" : la file graphique ne présente pas dans cette fenêtre".into()));
        }
        let formats = unsafe { surface_loader.get_physical_device_surface_formats(inner.physical_device, surface)? };
        let pick = formats
            .iter()
            .find(|f| {
                matches!(f.format, vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB)
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .or_else(|| formats.first())
            .copied()
            .ok_or(GpuError::NoDevice(" : aucun format de surface".into()))?;
        let d = &inner.device;
        let mut frames = Vec::new();
        for _ in 0..FRAMES_IN_FLIGHT {
            unsafe {
                let pool = d.create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(inner.queue_family)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )?;
                let cmd = d.allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )?[0];
                frames.push(FrameSync {
                    pool,
                    cmd,
                    acquired: d.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
                    fence: d
                        .create_fence(&vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED), None)?,
                });
            }
        }
        let mut s = Swapchain {
            gpu: gpu.clone(),
            loader: khr::swapchain::Device::new(&inner.instance, d),
            surface_loader,
            surface,
            swapchain: vk::SwapchainKHR::null(),
            images: Vec::new(),
            views: Vec::new(),
            rendered: Vec::new(),
            format: pick.format,
            color_space: pick.color_space,
            extent: vk::Extent2D::default(),
            depth_format: crate::target::DEPTH_FORMAT,
            depth: None,
            frames,
            frame: 0,
            mode,
            wanted: vk::Extent2D { width: size.0, height: size.1 },
            stale: true,
        };
        s.recreate()?;
        Ok(s)
    }

    /// La fenêtre a changé de taille : la chaîne sera recréée avant la prochaine image.
    pub fn resize(&mut self, width: u32, height: u32) {
        let e = vk::Extent2D { width, height };
        if e != self.wanted {
            self.wanted = e;
            self.stale = true;
        }
    }

    pub fn set_mode(&mut self, mode: PresentMode) {
        if mode != self.mode {
            self.mode = mode;
            self.stale = true;
        }
    }

    fn recreate(&mut self) -> Result<()> {
        let gpu = self.gpu.clone();
        let inner = &gpu.0;
        let d = &inner.device;
        unsafe { d.device_wait_idle()? };
        let caps = unsafe {
            self.surface_loader.get_physical_device_surface_capabilities(inner.physical_device, self.surface)?
        };
        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            vk::Extent2D {
                width: self.wanted.width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: self.wanted.height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        if extent.width == 0 || extent.height == 0 {
            // Fenêtre réduite : rien à présenter.
            self.extent = extent;
            return Ok(());
        }
        let modes = unsafe {
            self.surface_loader.get_physical_device_surface_present_modes(inner.physical_device, self.surface)?
        };
        let present_mode = match self.mode {
            PresentMode::LowLatency if modes.contains(&vk::PresentModeKHR::MAILBOX) => vk::PresentModeKHR::MAILBOX,
            _ => vk::PresentModeKHR::FIFO,
        };
        let mut count = caps.min_image_count.max(2) + 1;
        if caps.max_image_count > 0 {
            count = count.min(caps.max_image_count);
        }
        let old = self.swapchain;
        let swapchain = unsafe {
            self.loader.create_swapchain(
                &vk::SwapchainCreateInfoKHR::default()
                    .surface(self.surface)
                    .min_image_count(count)
                    .image_format(self.format)
                    .image_color_space(self.color_space)
                    .image_extent(extent)
                    .image_array_layers(1)
                    .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
                    .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .pre_transform(caps.current_transform)
                    .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                    .present_mode(present_mode)
                    .clipped(true)
                    .old_swapchain(old),
                None,
            )?
        };
        self.destroy_images();
        if old != vk::SwapchainKHR::null() {
            unsafe { self.loader.destroy_swapchain(old, None) };
        }
        self.swapchain = swapchain;
        self.images = unsafe { self.loader.get_swapchain_images(swapchain)? };
        for &img in &self.images {
            unsafe {
                self.views.push(
                    d.create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(self.format)
                            .subresource_range(
                                vk::ImageSubresourceRange::default()
                                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                                    .level_count(1)
                                    .layer_count(1),
                            ),
                        None,
                    )?,
                );
                self.rendered.push(d.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?);
            }
        }
        self.depth = Some(Image::new(
            &self.gpu,
            "profondeur fenêtre",
            self.depth_format,
            extent,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH,
        )?);
        self.extent = extent;
        self.stale = false;
        log::info!("chaîne d'échange {}×{} {:?} {:?}", extent.width, extent.height, self.format, present_mode);
        Ok(())
    }

    fn destroy_images(&mut self) {
        let d = &self.gpu.0.device;
        unsafe {
            for v in self.views.drain(..) {
                d.destroy_image_view(v, None);
            }
            for s in self.rendered.drain(..) {
                d.destroy_semaphore(s, None);
            }
        }
        self.images.clear();
        self.depth = None;
    }

    /// Indice de l'emplacement de la prochaine image (0 .. FRAMES_IN_FLIGHT), pour les tampons par image.
    pub fn slot(&self) -> usize {
        self.frame
    }

    /// Rend et présente une image. `prepare` s'enregistre hors de la passe (envois), `draw` dans la passe
    /// (couleur effacée à `clear`, profondeur à 0, fenêtre d'affichage retournée : Y vers le haut).
    /// Renvoie `false` si rien n'a été présenté (fenêtre réduite).
    pub fn present(
        &mut self,
        clear: [f32; 4],
        prepare: impl FnOnce(vk::CommandBuffer, usize),
        draw: impl FnOnce(vk::CommandBuffer),
    ) -> Result<bool> {
        if self.stale {
            self.recreate()?;
        }
        if self.extent.width == 0 || self.extent.height == 0 {
            return Ok(false);
        }
        let inner = &self.gpu.0;
        let d = &inner.device;
        let slot = self.frame;
        let f = &self.frames[slot];
        unsafe { d.wait_for_fences(&[f.fence], true, u64::MAX)? };
        let index =
            match unsafe { self.loader.acquire_next_image(self.swapchain, u64::MAX, f.acquired, vk::Fence::null()) } {
                Ok((i, suboptimal)) => {
                    if suboptimal {
                        self.stale = true;
                    }
                    i as usize
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.stale = true;
                    return Ok(false);
                }
                Err(e) => return Err(e.into()),
            };
        let f = &self.frames[slot];
        let image = self.images[index];
        let view = self.views[index];
        let depth = self.depth.as_ref().expect("profondeur créée avec la chaîne");
        let ext = self.extent;
        unsafe {
            d.reset_fences(&[f.fence])?;
            d.reset_command_buffer(f.cmd, vk::CommandBufferResetFlags::empty())?;
            d.begin_command_buffer(
                f.cmd,
                &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            let cmd = f.cmd;
            prepare(cmd, slot);
            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1);
            let to_attachment = [vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)];
            d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&to_attachment));
            depth.barrier(
                cmd,
                (
                    vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    vk::ImageLayout::UNDEFINED,
                ),
                (
                    vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
                    vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                ),
            );
            let color = [vk::RenderingAttachmentInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: clear } })];
            let depth_att = vk::RenderingAttachmentInfo::default()
                .image_view(depth.view)
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
                    .depth_attachment(&depth_att),
            );
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
            let to_present = [vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::NONE)
                .dst_access_mask(vk::AccessFlags2::NONE)
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)];
            d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&to_present));
            d.end_command_buffer(cmd)?;

            let waits = [vk::SemaphoreSubmitInfo::default()
                .semaphore(f.acquired)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
            let signals = [vk::SemaphoreSubmitInfo::default()
                .semaphore(self.rendered[index])
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
            let cmds = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
            let submit = [vk::SubmitInfo2::default()
                .wait_semaphore_infos(&waits)
                .command_buffer_infos(&cmds)
                .signal_semaphore_infos(&signals)];
            let q = inner.queue.lock().unwrap();
            d.queue_submit2(*q, &submit, f.fence)?;
            let wait = [self.rendered[index]];
            let chains = [self.swapchain];
            let indices = [index as u32];
            match self.loader.queue_present(
                *q,
                &vk::PresentInfoKHR::default().wait_semaphores(&wait).swapchains(&chains).image_indices(&indices),
            ) {
                Ok(false) => {}
                Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.stale = true,
                Err(e) => return Err(e.into()),
            }
        }
        self.frame = (self.frame + 1) % FRAMES_IN_FLIGHT;
        Ok(true)
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        let gpu = self.gpu.clone();
        let d = &gpu.0.device;
        unsafe {
            let _ = d.device_wait_idle();
            self.destroy_images();
            for f in self.frames.drain(..) {
                d.destroy_fence(f.fence, None);
                d.destroy_semaphore(f.acquired, None);
                d.destroy_command_pool(f.pool, None);
            }
            if self.swapchain != vk::SwapchainKHR::null() {
                self.loader.destroy_swapchain(self.swapchain, None);
            }
            self.surface_loader.destroy_surface(self.surface, None);
        }
    }
}
