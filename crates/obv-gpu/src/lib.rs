//! Couche Vulkan d'Obview (D-03) : appareil, mémoire, ressources, pipelines, rendu hors écran.
//!
//! Vulkan 1.3 minimum (rendu dynamique, synchronization2, adresses de tampons, indexation de descripteurs,
//! disposition scalaire). Les mesh shaders sont activés quand l'appareil les propose.

mod context;
mod error;
mod pipeline;
mod resources;
mod target;

pub use ash::vk;
pub use context::{Capabilities, Gpu, GpuDesc};
pub use error::{GpuError, Result};
pub use pipeline::{GraphicsPipeline, PipelineDesc, ShaderStages};
pub use resources::{Buffer, Image, MemoryLocation};
pub use target::RenderTarget;

/// Charge un SPIR-V compilé par `build.rs` depuis `shaders/` (nom complet, ex. `"mesh.frag"`).
#[macro_export]
macro_rules! spirv {
    ($name:literal) => {
        include_bytes!(concat!(env!("OUT_DIR"), "/", $name, ".spv"))
    };
}
