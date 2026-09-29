#[cfg(any(dx12, all(vulkan, windows)))]
pub(super) mod dxgi;

#[cfg(all(native, feature = "renderdoc"))]
pub(super) mod renderdoc;

pub mod db {
    pub mod amd {

        pub const VENDOR: u32 = 0x1002;
    }
    pub mod apple {

        pub const VENDOR: u32 = 0x106B;
    }
    pub mod arm {

        pub const VENDOR: u32 = 0x13B5;
    }
    pub mod broadcom {

        pub const VENDOR: u32 = 0x14E4;
    }
    pub mod imgtec {

        pub const VENDOR: u32 = 0x1010;
    }
    pub mod intel {

        pub const VENDOR: u32 = 0x8086;
        pub const DEVICE_KABY_LAKE_MASK: u32 = 0x5900;
        pub const DEVICE_SKY_LAKE_MASK: u32 = 0x1900;
    }
    pub mod mesa {

        pub const VENDOR: u32 = 0x10005;
    }
    pub mod nvidia {

        pub const VENDOR: u32 = 0x10DE;
    }
    pub mod qualcomm {

        pub const VENDOR: u32 = 0x5143;
    }
}

pub const MAX_I32_BINDING_SIZE: u32 = (1 << 31) - 1;

pub use wgpu_naga_bridge::map_naga_stage;

impl crate::CopyExtent {
    pub fn map_extent_to_copy_size(extent: &wgt::Extent3d, dim: wgt::TextureDimension) -> Self {
        Self {
            width: extent.width,
            height: extent.height,
            depth: match dim {
                wgt::TextureDimension::D1 | wgt::TextureDimension::D2 => 1,
                wgt::TextureDimension::D3 => extent.depth_or_array_layers,
            },
        }
    }

    pub fn min(&self, other: &Self) -> Self {
        Self {
            width: self.width.min(other.width),
            height: self.height.min(other.height),
            depth: self.depth.min(other.depth),
        }
    }

    pub fn at_mip_level(&self, level: u32) -> Self {
        Self {
            width: (self.width >> level).max(1),
            height: (self.height >> level).max(1),
            depth: (self.depth >> level).max(1),
        }
    }
}

impl crate::TextureCopyBase {
    pub fn max_copy_size(&self, full_size: &crate::CopyExtent) -> crate::CopyExtent {
        let mip = full_size.at_mip_level(self.mip_level);
        crate::CopyExtent {
            width: mip.width - self.origin.x,
            height: mip.height - self.origin.y,
            depth: mip.depth - self.origin.z,
        }
    }
}

impl crate::BufferTextureCopy {
    pub fn clamp_size_to_virtual(&mut self, full_size: &crate::CopyExtent) {
        let max_size = self.texture_base.max_copy_size(full_size);
        self.size = self.size.min(&max_size);
    }
}

impl crate::TextureCopy {
    pub fn clamp_size_to_virtual(
        &mut self,
        full_src_size: &crate::CopyExtent,
        full_dst_size: &crate::CopyExtent,
    ) {
        let max_src_size = self.src_base.max_copy_size(full_src_size);
        let max_dst_size = self.dst_base.max_copy_size(full_dst_size);
        self.size = self.size.min(&max_src_size).min(&max_dst_size);
    }
}

#[cfg_attr(not(any_backend), allow(dead_code))]
pub(crate) fn adjust_raw_limits(mut limits: wgt::Limits) -> wgt::Limits {

    limits.max_bind_groups = limits.max_bind_groups.min(crate::MAX_BIND_GROUPS as u32);
    limits.max_vertex_buffers = limits
        .max_vertex_buffers
        .min(crate::MAX_VERTEX_BUFFERS as u32);

    const { assert!(crate::MAX_BIND_GROUPS + crate::MAX_VERTEX_BUFFERS == 24) };
    limits.max_bind_groups_plus_vertex_buffers = limits.max_bind_groups_plus_vertex_buffers.min(24);
    limits.max_color_attachments = limits
        .max_color_attachments
        .min(crate::MAX_COLOR_ATTACHMENTS as u32);

    const MAX_SHADER_STAGES_PER_PIPELINE: u32 = 2;
    let max_per_stage_resources =
        limits.max_bindings_per_bind_group / MAX_SHADER_STAGES_PER_PIPELINE;

    cap_limits_to_be_under_the_sum_limit(
        [
            &mut limits.max_sampled_textures_per_shader_stage,
            &mut limits.max_uniform_buffers_per_shader_stage,
            &mut limits.max_storage_textures_per_shader_stage,
            &mut limits.max_storage_buffers_per_shader_stage,
            &mut limits.max_samplers_per_shader_stage,
            &mut limits.max_acceleration_structures_per_shader_stage,
        ],
        max_per_stage_resources,
    );

    limits.max_dynamic_uniform_buffers_per_pipeline_layout = limits
        .max_dynamic_uniform_buffers_per_pipeline_layout
        .min(limits.max_uniform_buffers_per_shader_stage);
    limits.max_dynamic_storage_buffers_per_pipeline_layout = limits
        .max_dynamic_storage_buffers_per_pipeline_layout
        .min(limits.max_storage_buffers_per_shader_stage);

    limits.min_uniform_buffer_offset_alignment = limits.min_uniform_buffer_offset_alignment.max(32);
    limits.min_storage_buffer_offset_alignment = limits.min_storage_buffer_offset_alignment.max(32);

    limits.max_uniform_buffer_binding_size = limits
        .max_uniform_buffer_binding_size
        .min(limits.max_buffer_size);
    limits.max_storage_buffer_binding_size = limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size);

    limits.max_storage_buffer_binding_size &= !(u64::from(wgt::STORAGE_BINDING_SIZE_ALIGNMENT) - 1);
    limits.max_vertex_buffer_array_stride &= !(wgt::VERTEX_ALIGNMENT as u32 - 1);

    let x = limits.max_compute_workgroup_size_x;
    let y = limits.max_compute_workgroup_size_y;
    let z = limits.max_compute_workgroup_size_z;
    let m = limits.max_compute_invocations_per_workgroup;
    limits.max_compute_workgroup_size_x = x.min(m);
    limits.max_compute_workgroup_size_y = y.min(m);
    limits.max_compute_workgroup_size_z = z.min(m);
    limits.max_compute_invocations_per_workgroup = m.min(x.saturating_mul(y).saturating_mul(z));

    limits.max_immediate_size = limits.max_immediate_size.min(256);

    limits
}

pub fn cap_limits_to_be_under_the_sum_limit<const N: usize>(
    mut limits: [&mut u32; N],
    sum_limit: u32,
) {
    limits.sort();

    let mut rem_limit = sum_limit;
    let mut divisor = limits.len() as u32;
    for limit_to_adjust in limits {
        let limit = rem_limit / divisor;
        *limit_to_adjust = (*limit_to_adjust).min(limit);
        rem_limit -= *limit_to_adjust;
        divisor -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cap_limits_to_be_under_the_sum_limit() {
        test([3, 3, 3], 3, [1, 1, 1]);
        test([3, 2, 1], 3, [1, 1, 1]);
        test([1, 2, 3], 6, [1, 2, 3]);
        test([1, 2, 3], 3, [1, 1, 1]);
        test([1, 8, 100], 6, [1, 2, 3]);
        test([2, 80, 80], 6, [2, 2, 2]);
        test([2, 80, 80], 12, [2, 5, 5]);

        #[track_caller]
        fn test<const N: usize>(mut input: [u32; N], limit: u32, output: [u32; N]) {
            cap_limits_to_be_under_the_sum_limit(input.each_mut(), limit);
            assert_eq!(input, output);
        }
    }
}
