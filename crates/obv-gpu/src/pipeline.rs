use std::io::Cursor;

use ash::vk;

use crate::{Gpu, GpuError, Result};

/// Étages du pipeline : chaîne classique (sommets) ou mesh shaders (avec étage de tâches facultatif).
pub enum ShaderStages<'a> {
    Vertex { vertex: &'a [u8], fragment: &'a [u8] },
    Mesh { task: Option<&'a [u8]>, mesh: &'a [u8], fragment: &'a [u8] },
}

pub struct PipelineDesc<'a> {
    pub stages: ShaderStages<'a>,
    pub color_formats: &'a [vk::Format],
    pub depth_format: Option<vk::Format>,
    /// Profondeur inversée (précision maximale sur de grandes scènes) : `GREATER_OR_EQUAL`, effacement à 0.
    pub depth_compare: vk::CompareOp,
    pub depth_write: bool,
    pub cull_mode: vk::CullModeFlags,
    pub alpha_blend: bool,
    pub push_constant_size: u32,
    pub set_layouts: &'a [vk::DescriptorSetLayout],
}

impl Default for PipelineDesc<'_> {
    fn default() -> Self {
        Self {
            stages: ShaderStages::Vertex { vertex: &[], fragment: &[] },
            color_formats: &[],
            depth_format: None,
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            depth_write: true,
            cull_mode: vk::CullModeFlags::BACK,
            alpha_blend: false,
            push_constant_size: 0,
            set_layouts: &[],
        }
    }
}

pub struct GraphicsPipeline {
    gpu: Gpu,
    pub raw: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub push_stages: vk::ShaderStageFlags,
}

fn module(gpu: &Gpu, spv: &[u8]) -> Result<vk::ShaderModule> {
    let words = ash::util::read_spv(&mut Cursor::new(spv)).map_err(GpuError::Spirv)?;
    Ok(unsafe { gpu.device().create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)? })
}

impl GraphicsPipeline {
    pub fn new(gpu: &Gpu, desc: &PipelineDesc<'_>) -> Result<Self> {
        let d = gpu.device();
        let mut modules = Vec::new();
        let mut stages = Vec::new();
        let mut push_stages = vk::ShaderStageFlags::FRAGMENT;
        let mut add = |spv: &[u8], stage: vk::ShaderStageFlags| -> Result<()> {
            let m = module(gpu, spv)?;
            modules.push(m);
            stages.push(vk::PipelineShaderStageCreateInfo::default().stage(stage).module(m).name(c"main"));
            Ok(())
        };
        let is_mesh = match desc.stages {
            ShaderStages::Vertex { vertex, fragment } => {
                add(vertex, vk::ShaderStageFlags::VERTEX)?;
                add(fragment, vk::ShaderStageFlags::FRAGMENT)?;
                push_stages |= vk::ShaderStageFlags::VERTEX;
                false
            }
            ShaderStages::Mesh { task, mesh, fragment } => {
                if let Some(t) = task {
                    add(t, vk::ShaderStageFlags::TASK_EXT)?;
                    push_stages |= vk::ShaderStageFlags::TASK_EXT;
                }
                add(mesh, vk::ShaderStageFlags::MESH_EXT)?;
                add(fragment, vk::ShaderStageFlags::FRAGMENT)?;
                push_stages |= vk::ShaderStageFlags::MESH_EXT;
                true
            }
        };
        let destroy_modules = |modules: &[vk::ShaderModule]| {
            for m in modules {
                unsafe { d.destroy_shader_module(*m, None) };
            }
        };
        let ranges = [vk::PushConstantRange::default().stage_flags(push_stages).size(desc.push_constant_size)];
        let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(desc.set_layouts);
        let layout_info =
            if desc.push_constant_size > 0 { layout_info.push_constant_ranges(&ranges) } else { layout_info };
        let layout = match unsafe { d.create_pipeline_layout(&layout_info, None) } {
            Ok(l) => l,
            Err(e) => {
                destroy_modules(&modules);
                return Err(e.into());
            }
        };

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let assembly =
            vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(desc.cull_mode)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let multisample =
            vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(desc.depth_format.is_some())
            .depth_write_enable(desc.depth_format.is_some() && desc.depth_write)
            .depth_compare_op(desc.depth_compare);
        let blend_att: Vec<_> = desc
            .color_formats
            .iter()
            .map(|_| {
                let a =
                    vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA);
                if desc.alpha_blend {
                    // Couleurs prémultipliées.
                    a.blend_enable(true)
                        .src_color_blend_factor(vk::BlendFactor::ONE)
                        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                        .color_blend_op(vk::BlendOp::ADD)
                        .src_alpha_blend_factor(vk::BlendFactor::ONE)
                        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                        .alpha_blend_op(vk::BlendOp::ADD)
                } else {
                    a
                }
            })
            .collect();
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_att);
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let mut rendering = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(desc.color_formats)
            .depth_attachment_format(desc.depth_format.unwrap_or(vk::Format::UNDEFINED));
        let mut info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(layout)
            .push_next(&mut rendering);
        if !is_mesh {
            info = info.vertex_input_state(&vertex_input).input_assembly_state(&assembly);
        }
        let result = unsafe { d.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) };
        destroy_modules(&modules);
        let raw = match result {
            Ok(p) => p[0],
            Err((_, e)) => {
                unsafe { d.destroy_pipeline_layout(layout, None) };
                return Err(e.into());
            }
        };
        Ok(Self { gpu: gpu.clone(), raw, layout, push_stages })
    }

    /// Envoie les constantes de poussée (structure `#[repr(C)]` alignée comme dans le shader).
    pub fn push<T: bytemuck::Pod>(&self, cmd: vk::CommandBuffer, value: &T) {
        unsafe {
            self.gpu.device().cmd_push_constants(cmd, self.layout, self.push_stages, 0, bytemuck::bytes_of(value))
        };
    }

    pub fn bind(&self, cmd: vk::CommandBuffer) {
        unsafe { self.gpu.device().cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.raw) };
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            self.gpu.device().destroy_pipeline(self.raw, None);
            self.gpu.device().destroy_pipeline_layout(self.layout, None);
        }
    }
}
