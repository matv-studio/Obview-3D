use std::{
    ffi::{CStr, c_char, c_void},
    mem::ManuallyDrop,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use ash::{ext, vk};
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};

use crate::{GpuError, Result};

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

/// Options de création.
#[derive(Clone, Debug, Default)]
pub struct GpuDesc {
    /// Active la couche de validation Khronos si elle est installée (essais, débogage).
    pub validation: bool,
    /// Extensions d'instance demandées par la fenêtre (surface).
    pub instance_extensions: Vec<&'static CStr>,
    /// Demande l'extension de présentation (fenêtre).
    pub presentation: bool,
}

/// Fonctions optionnelles disponibles sur l'appareil choisi.
#[derive(Clone, Debug)]
pub struct Capabilities {
    pub device_name: String,
    pub driver: String,
    pub api_version: u32,
    pub mesh_shader: bool,
    pub max_mesh_output_vertices: u32,
    pub max_mesh_output_primitives: u32,
    pub validation: bool,
}

pub(crate) struct Inner {
    pub(crate) entry: ash::Entry,
    pub(crate) instance: ash::Instance,
    debug: Option<(ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    pub(crate) physical_device: vk::PhysicalDevice,
    pub(crate) device: ash::Device,
    pub(crate) queue_family: u32,
    pub(crate) queue: Mutex<vk::Queue>,
    pub(crate) mesh: Option<ext::mesh_shader::Device>,
    pub(crate) allocator: ManuallyDrop<Mutex<Allocator>>,
    pub(crate) caps: Capabilities,
    validation_errors: Box<AtomicUsize>,
}

/// Contexte Vulkan partagé. Se clone à coût nul ; les ressources gardent une référence et se libèrent seules.
#[derive(Clone)]
pub struct Gpu(pub(crate) Arc<Inner>);

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _ty: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user: *mut c_void,
) -> vk::Bool32 {
    // SAFETY: Vulkan passe un pointeur valide pendant l'appel ; `user` pointe sur le compteur de `Inner`.
    let msg = unsafe {
        let d = &*data;
        if d.p_message.is_null() { String::new() } else { CStr::from_ptr(d.p_message).to_string_lossy().into_owned() }
    };
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        log::error!("validation Vulkan : {msg}");
        eprintln!("[validation Vulkan] {msg}");
        if !user.is_null() {
            // SAFETY: voir plus haut.
            unsafe { &*(user as *const AtomicUsize) }.fetch_add(1, Ordering::Relaxed);
        }
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        log::warn!("validation Vulkan : {msg}");
    }
    vk::FALSE
}

fn score(props: &vk::PhysicalDeviceProperties) -> u32 {
    match props.device_type {
        vk::PhysicalDeviceType::DISCRETE_GPU => 4,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
        vk::PhysicalDeviceType::CPU => 1,
        _ => 0,
    }
}

impl Gpu {
    /// Crée l'instance et l'appareil. Choisit la carte la plus puissante qui offre Vulkan 1.3 et les fonctions
    /// requises ; la variable `OBV_GPU` (sous-chaîne du nom) force un appareil.
    pub fn new(desc: &GpuDesc) -> Result<Self> {
        // SAFETY: chargement de la bibliothèque Vulkan du système.
        let entry = unsafe { ash::Entry::load()? };

        let layers = unsafe { entry.enumerate_instance_layer_properties()? };
        let has_validation =
            desc.validation && layers.iter().any(|l| l.layer_name_as_c_str().is_ok_and(|n| n == VALIDATION_LAYER));
        let inst_exts = unsafe { entry.enumerate_instance_extension_properties(None)? };
        let has_debug_utils =
            inst_exts.iter().any(|e| e.extension_name_as_c_str().is_ok_and(|n| n == ext::debug_utils::NAME));

        let mut ext_ptrs: Vec<*const c_char> = desc.instance_extensions.iter().map(|e| e.as_ptr()).collect();
        if has_debug_utils && has_validation {
            ext_ptrs.push(ext::debug_utils::NAME.as_ptr());
        }
        let layer_ptrs: Vec<*const c_char> = if has_validation { vec![VALIDATION_LAYER.as_ptr()] } else { vec![] };

        let app = vk::ApplicationInfo::default()
            .application_name(c"Obview 3D")
            .engine_name(c"Obview")
            .api_version(vk::API_VERSION_1_3);
        let instance = unsafe {
            entry.create_instance(
                &vk::InstanceCreateInfo::default()
                    .application_info(&app)
                    .enabled_layer_names(&layer_ptrs)
                    .enabled_extension_names(&ext_ptrs),
                None,
            )?
        };

        let validation_errors = Box::new(AtomicUsize::new(0));
        let debug = if has_debug_utils && has_validation {
            let loader = ext::debug_utils::Instance::new(&entry, &instance);
            let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(debug_callback))
                .user_data(&*validation_errors as *const AtomicUsize as *mut c_void);
            let messenger = unsafe { loader.create_debug_utils_messenger(&info, None)? };
            Some((loader, messenger))
        } else {
            None
        };

        let forced = std::env::var("OBV_GPU").ok().map(|s| s.to_lowercase());
        let mut candidates = Vec::new();
        let mut rejected = Vec::new();
        for pd in unsafe { instance.enumerate_physical_devices()? } {
            let props = unsafe { instance.get_physical_device_properties(pd) };
            let name = props.device_name_as_c_str().map(|c| c.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some(f) = &forced
                && !name.to_lowercase().contains(f)
            {
                continue;
            }
            if props.api_version < vk::API_VERSION_1_3 {
                rejected.push(format!("{name} (Vulkan < 1.3)"));
                continue;
            }
            let families = unsafe { instance.get_physical_device_queue_family_properties(pd) };
            let Some(qf) = families
                .iter()
                .position(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE))
            else {
                rejected.push(format!("{name} (pas de file graphique)"));
                continue;
            };
            candidates.push((score(&props), pd, qf as u32, name));
        }
        candidates.sort_by_key(|c| std::cmp::Reverse(c.0));
        let Some((_, physical_device, queue_family, device_name)) = candidates.into_iter().next() else {
            let detail = if rejected.is_empty() { String::new() } else { format!(" : {}", rejected.join(", ")) };
            return Err(GpuError::NoDevice(detail));
        };

        // Fonctions disponibles.
        let dev_exts = unsafe { instance.enumerate_device_extension_properties(physical_device)? };
        let has_ext = |n: &CStr| dev_exts.iter().any(|e| e.extension_name_as_c_str().is_ok_and(|x| x == n));
        let mut s_mesh = vk::PhysicalDeviceMeshShaderFeaturesEXT::default();
        let mut s12 = vk::PhysicalDeviceVulkan12Features::default();
        let mut s13 = vk::PhysicalDeviceVulkan13Features::default();
        {
            let mut f2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut s12).push_next(&mut s13);
            if has_ext(ext::mesh_shader::NAME) {
                f2 = f2.push_next(&mut s_mesh);
            }
            unsafe { instance.get_physical_device_features2(physical_device, &mut f2) };
        }
        let missing: Vec<&str> = [
            (s12.buffer_device_address, "bufferDeviceAddress"),
            (s12.descriptor_indexing, "descriptorIndexing"),
            (s12.scalar_block_layout, "scalarBlockLayout"),
            (s12.timeline_semaphore, "timelineSemaphore"),
            (s13.dynamic_rendering, "dynamicRendering"),
            (s13.synchronization2, "synchronization2"),
            (s13.maintenance4, "maintenance4"),
        ]
        .into_iter()
        .filter(|(v, _)| *v == vk::FALSE)
        .map(|(_, n)| n)
        .collect();
        if !missing.is_empty() {
            return Err(GpuError::NoDevice(format!(" : {device_name} sans {}", missing.join(", "))));
        }
        let mesh_shader = s_mesh.mesh_shader == vk::TRUE && s_mesh.task_shader == vk::TRUE;

        let mut dev_ext_ptrs: Vec<*const c_char> = Vec::new();
        if mesh_shader {
            dev_ext_ptrs.push(ext::mesh_shader::NAME.as_ptr());
        }
        if desc.presentation {
            dev_ext_ptrs.push(ash::khr::swapchain::NAME.as_ptr());
        }

        let mut e12 = vk::PhysicalDeviceVulkan12Features::default()
            .buffer_device_address(true)
            .descriptor_indexing(true)
            .scalar_block_layout(true)
            .timeline_semaphore(true)
            .runtime_descriptor_array(s12.runtime_descriptor_array == vk::TRUE)
            .descriptor_binding_partially_bound(s12.descriptor_binding_partially_bound == vk::TRUE)
            .descriptor_binding_variable_descriptor_count(s12.descriptor_binding_variable_descriptor_count == vk::TRUE)
            .shader_sampled_image_array_non_uniform_indexing(
                s12.shader_sampled_image_array_non_uniform_indexing == vk::TRUE,
            )
            .draw_indirect_count(s12.draw_indirect_count == vk::TRUE);
        let mut e13 = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true)
            .maintenance4(true);
        let mut e_mesh = vk::PhysicalDeviceMeshShaderFeaturesEXT::default().mesh_shader(true).task_shader(true);
        let base = unsafe { instance.get_physical_device_features(physical_device) };
        let core = vk::PhysicalDeviceFeatures::default()
            .shader_int64(base.shader_int64 == vk::TRUE)
            .multi_draw_indirect(base.multi_draw_indirect == vk::TRUE)
            .fill_mode_non_solid(base.fill_mode_non_solid == vk::TRUE)
            .wide_lines(base.wide_lines == vk::TRUE)
            .sampler_anisotropy(base.sampler_anisotropy == vk::TRUE);
        let mut f2 = vk::PhysicalDeviceFeatures2::default().features(core).push_next(&mut e12).push_next(&mut e13);
        if mesh_shader {
            f2 = f2.push_next(&mut e_mesh);
        }
        let priorities = [1.0f32];
        let queues =
            [vk::DeviceQueueCreateInfo::default().queue_family_index(queue_family).queue_priorities(&priorities)];
        let device = unsafe {
            instance.create_device(
                physical_device,
                &vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queues)
                    .enabled_extension_names(&dev_ext_ptrs)
                    .push_next(&mut f2),
                None,
            )?
        };
        let queue = unsafe { device.get_device_queue(queue_family, 0) };

        let props = unsafe { instance.get_physical_device_properties(physical_device) };
        let (mut max_v, mut max_p) = (0, 0);
        if mesh_shader {
            let mut mp = vk::PhysicalDeviceMeshShaderPropertiesEXT::default();
            let mut p2 = vk::PhysicalDeviceProperties2::default().push_next(&mut mp);
            unsafe { instance.get_physical_device_properties2(physical_device, &mut p2) };
            max_v = mp.max_mesh_output_vertices;
            max_p = mp.max_mesh_output_primitives;
        }
        let driver = {
            let mut dp = vk::PhysicalDeviceDriverProperties::default();
            let mut p2 = vk::PhysicalDeviceProperties2::default().push_next(&mut dp);
            unsafe { instance.get_physical_device_properties2(physical_device, &mut p2) };
            dp.driver_info_as_c_str().map(|c| c.to_string_lossy().into_owned()).unwrap_or_default()
        };

        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings: Default::default(),
            buffer_device_address: true,
            allocation_sizes: Default::default(),
        })?;
        let mesh = mesh_shader.then(|| ext::mesh_shader::Device::new(&instance, &device));
        let caps = Capabilities {
            device_name,
            driver,
            api_version: props.api_version,
            mesh_shader,
            max_mesh_output_vertices: max_v,
            max_mesh_output_primitives: max_p,
            validation: has_validation,
        };
        log::info!("appareil Vulkan : {caps:?}");
        Ok(Gpu(Arc::new(Inner {
            entry,
            instance,
            debug,
            physical_device,
            device,
            queue_family,
            queue: Mutex::new(queue),
            mesh,
            allocator: ManuallyDrop::new(Mutex::new(allocator)),
            caps,
            validation_errors,
        })))
    }

    pub fn caps(&self) -> &Capabilities {
        &self.0.caps
    }

    pub fn device(&self) -> &ash::Device {
        &self.0.device
    }

    pub fn instance(&self) -> &ash::Instance {
        &self.0.instance
    }

    pub fn entry(&self) -> &ash::Entry {
        &self.0.entry
    }

    pub fn physical_device(&self) -> vk::PhysicalDevice {
        self.0.physical_device
    }

    pub fn queue_family(&self) -> u32 {
        self.0.queue_family
    }

    /// Fonctions des mesh shaders, si l'appareil les a.
    pub fn mesh_shader(&self) -> Option<&ext::mesh_shader::Device> {
        self.0.mesh.as_ref()
    }

    /// Nombre d'erreurs signalées par la couche de validation depuis la création.
    pub fn validation_errors(&self) -> usize {
        self.0.validation_errors.load(Ordering::Relaxed)
    }

    /// Enregistre des commandes, les soumet et attend la fin (transferts ponctuels, essais).
    pub fn submit_and_wait(&self, record: impl FnOnce(vk::CommandBuffer)) -> Result<()> {
        let d = &self.0.device;
        unsafe {
            let pool = d.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(self.0.queue_family)
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None,
            )?;
            let result = (|| {
                let cmd = d.allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )?[0];
                d.begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;
                record(cmd);
                d.end_command_buffer(cmd)?;
                let fence = d.create_fence(&vk::FenceCreateInfo::default(), None)?;
                let cmds = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
                let submit = [vk::SubmitInfo2::default().command_buffer_infos(&cmds)];
                let r = {
                    let q = self.0.queue.lock().unwrap();
                    d.queue_submit2(*q, &submit, fence)
                };
                let r = r.and_then(|_| d.wait_for_fences(&[fence], true, u64::MAX));
                d.destroy_fence(fence, None);
                r
            })();
            d.destroy_command_pool(pool, None);
            result?;
        }
        Ok(())
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);
            if let Some((loader, messenger)) = self.debug.take() {
                loader.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
