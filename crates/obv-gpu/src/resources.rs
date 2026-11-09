use ash::vk;
pub use gpu_allocator::MemoryLocation;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};

use crate::{Gpu, Result};

/// Tampon GPU avec son adresse (lue par les shaders via `GL_EXT_buffer_reference`).
pub struct Buffer {
    gpu: Gpu,
    pub raw: vk::Buffer,
    allocation: Option<Allocation>,
    pub size: u64,
    pub address: vk::DeviceAddress,
}

impl Buffer {
    pub fn new(
        gpu: &Gpu,
        name: &str,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
    ) -> Result<Self> {
        let d = gpu.device();
        let usage = usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        let raw = unsafe {
            d.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size.max(4))
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?
        };
        let requirements = unsafe { d.get_buffer_memory_requirements(raw) };
        let allocation = gpu.0.allocator.lock().unwrap().allocate(&AllocationCreateDesc {
            name,
            requirements,
            location,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        });
        let allocation = match allocation {
            Ok(a) => a,
            Err(e) => {
                unsafe { d.destroy_buffer(raw, None) };
                return Err(e.into());
            }
        };
        unsafe { d.bind_buffer_memory(raw, allocation.memory(), allocation.offset())? };
        let address = unsafe { d.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(raw)) };
        Ok(Self { gpu: gpu.clone(), raw, allocation: Some(allocation), size, address })
    }

    /// Tampon visible par le processeur, rempli avec `data`.
    pub fn with_data(gpu: &Gpu, name: &str, data: &[u8], usage: vk::BufferUsageFlags) -> Result<Self> {
        let mut b = Self::new(gpu, name, data.len() as u64, usage, MemoryLocation::CpuToGpu)?;
        b.mapped_mut().expect("tampon CpuToGpu non projeté")[..data.len()].copy_from_slice(data);
        Ok(b)
    }

    pub fn mapped(&self) -> Option<&[u8]> {
        self.allocation.as_ref()?.mapped_slice()
    }

    pub fn mapped_mut(&mut self) -> Option<&mut [u8]> {
        self.allocation.as_mut()?.mapped_slice_mut()
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { self.gpu.device().destroy_buffer(self.raw, None) };
        if let Some(a) = self.allocation.take() {
            let _ = self.gpu.0.allocator.lock().unwrap().free(a);
        }
    }
}

/// Image 2D avec sa vue.
pub struct Image {
    gpu: Gpu,
    pub raw: vk::Image,
    pub view: vk::ImageView,
    allocation: Option<Allocation>,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub aspect: vk::ImageAspectFlags,
}

impl Image {
    pub fn new(
        gpu: &Gpu,
        name: &str,
        format: vk::Format,
        extent: vk::Extent2D,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
    ) -> Result<Self> {
        let d = gpu.device();
        let raw = unsafe {
            d.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D { width: extent.width, height: extent.height, depth: 1 })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )?
        };
        let requirements = unsafe { d.get_image_memory_requirements(raw) };
        let allocation = gpu.0.allocator.lock().unwrap().allocate(&AllocationCreateDesc {
            name,
            requirements,
            location: MemoryLocation::GpuOnly,
            linear: false,
            allocation_scheme: AllocationScheme::DedicatedImage(raw),
        });
        let allocation = match allocation {
            Ok(a) => a,
            Err(e) => {
                unsafe { d.destroy_image(raw, None) };
                return Err(e.into());
            }
        };
        unsafe { d.bind_image_memory(raw, allocation.memory(), allocation.offset())? };
        let view = unsafe {
            d.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(raw)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(subresource(aspect)),
                None,
            )?
        };
        Ok(Self { gpu: gpu.clone(), raw, view, allocation: Some(allocation), format, extent, aspect })
    }

    /// Barrière de changement de disposition (synchronization2).
    pub fn barrier(
        &self,
        cmd: vk::CommandBuffer,
        (src_stage, src_access, old): (vk::PipelineStageFlags2, vk::AccessFlags2, vk::ImageLayout),
        (dst_stage, dst_access, new): (vk::PipelineStageFlags2, vk::AccessFlags2, vk::ImageLayout),
    ) {
        let b = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(src_stage)
            .src_access_mask(src_access)
            .dst_stage_mask(dst_stage)
            .dst_access_mask(dst_access)
            .old_layout(old)
            .new_layout(new)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.raw)
            .subresource_range(subresource(self.aspect))];
        unsafe {
            self.gpu.device().cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&b))
        };
    }
}

pub(crate) fn subresource(aspect: vk::ImageAspectFlags) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default().aspect_mask(aspect).level_count(1).layer_count(1)
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.gpu.device().destroy_image_view(self.view, None);
            self.gpu.device().destroy_image(self.raw, None);
        }
        if let Some(a) = self.allocation.take() {
            let _ = self.gpu.0.allocator.lock().unwrap().free(a);
        }
    }
}
