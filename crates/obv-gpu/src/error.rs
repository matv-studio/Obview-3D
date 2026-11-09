use ash::vk;

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("bibliothèque Vulkan introuvable : {0}")]
    Loading(#[from] ash::LoadingError),
    #[error("Vulkan : {0}")]
    Vk(#[from] vk::Result),
    #[error("mémoire GPU : {0}")]
    Alloc(#[from] gpu_allocator::AllocationError),
    #[error("aucun appareil Vulkan 1.3 ne convient{0}")]
    NoDevice(String),
    #[error("SPIR-V invalide : {0}")]
    Spirv(std::io::Error),
}

pub type Result<T> = std::result::Result<T, GpuError>;
