#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(

    clippy::arc_with_non_send_sync,

    clippy::match_like_matches_macro,

    clippy::redundant_pattern_matching,

    clippy::needless_lifetimes,

    clippy::new_without_default,

    clippy::single_match,

    clippy::vec_init_then_push,

    clippy::missing_safety_doc,

    clippy::pattern_type_mismatch,

    clippy::large_enum_variant
)]
#![warn(
    clippy::alloc_instead_of_core,
    clippy::ptr_as_ptr,
    clippy::std_instead_of_alloc,
    clippy::std_instead_of_core,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_op_in_unsafe_fn,
    unused_extern_crates,
    unused_qualifications
)]

extern crate alloc;
#[allow(unused_extern_crates)]
extern crate naga_types as nt;
extern crate wgpu_types as wgt;

#[cfg(any(dx12, gles_with_std, metal, vulkan))]
#[macro_use]
extern crate std;

#[cfg(dx12)]
pub mod dx12;

#[cfg(gles)]
pub mod gles;

#[cfg(metal)]
pub mod metal;

pub mod noop;

#[cfg(vulkan)]
pub mod vulkan;

pub mod auxil;
pub mod api {
    #[cfg(dx12)]
    pub use super::dx12::Api as Dx12;
    #[cfg(gles)]
    pub use super::gles::Api as Gles;
    #[cfg(metal)]
    pub use super::metal::Api as Metal;
    pub use super::noop::Api as Noop;
    #[cfg(vulkan)]
    pub use super::vulkan::Api as Vulkan;
}

mod dynamic;
#[cfg(feature = "validation_canary")]
mod validation_canary;

#[cfg(feature = "validation_canary")]
pub use validation_canary::{ValidationCanary, VALIDATION_CANARY};

pub(crate) use dynamic::impl_dyn_resource;
pub use dynamic::{
    DynAccelerationStructure, DynAcquiredSurfaceTexture, DynAdapter, DynBindGroup,
    DynBindGroupLayout, DynBuffer, DynCommandBuffer, DynCommandEncoder, DynComputePipeline,
    DynDevice, DynExposedAdapter, DynFence, DynInstance, DynOpenDevice, DynPipelineCache,
    DynPipelineLayout, DynQuerySet, DynQueue, DynRayTracingPipeline, DynRenderPipeline,
    DynResource, DynSampler, DynShaderModule, DynSurface, DynSurfaceTexture, DynTexture,
    DynTextureView,
};

#[allow(unused)]
use alloc::boxed::Box;
use alloc::{borrow::Cow, string::String, vec::Vec};
use core::{
    borrow::Borrow,
    error::Error,
    fmt,
    num::{NonZeroU32, NonZeroU64},
    ops::{Range, RangeInclusive},
    ptr::NonNull,
};

use bitflags::bitflags;
use raw_window_handle::DisplayHandle;
use thiserror::Error;
use wgt::WasmNotSendSync;

cfg_if::cfg_if! {
    if #[cfg(supports_ptr_atomics)] {
        use alloc::sync::Arc;
    } else if #[cfg(feature = "portable-atomic")] {
        use portable_atomic_util::Arc;
    }
}

pub const MAX_CONCURRENT_SHADER_STAGES: usize = 3;
pub const MAX_ANISOTROPY: u8 = 16;
pub const MAX_BIND_GROUPS: usize = 8;
pub const MAX_VERTEX_BUFFERS: usize = 16;
pub const MAX_COLOR_ATTACHMENTS: usize = 8;
pub const MAX_MIP_LEVELS: u32 = 16;

pub const QUERY_SIZE: wgt::BufferAddress = 8;

pub type Label<'a> = Option<&'a str>;
pub type MemoryRange = Range<wgt::BufferAddress>;
pub type FenceValue = u64;
#[cfg(supports_64bit_atomics)]
pub type AtomicFenceValue = core::sync::atomic::AtomicU64;
#[cfg(not(supports_64bit_atomics))]
pub type AtomicFenceValue = portable_atomic::AtomicU64;

#[cfg(any(gles, vulkan, metal))]
pub type DropCallback = Box<dyn FnOnce() + Send + Sync + 'static>;

#[cfg(any(gles, vulkan, metal))]
pub struct DropGuard {
    callback: Option<DropCallback>,
}

#[cfg(all(any(gles, vulkan, metal), any(native, Emscripten)))]
impl DropGuard {
    fn from_option(callback: Option<DropCallback>) -> Option<Self> {
        callback.map(Self::new)
    }

    fn new(callback: DropCallback) -> Self {
        Self {
            callback: Some(callback),
        }
    }
}

#[cfg(any(gles, vulkan, metal))]
impl Drop for DropGuard {
    fn drop(&mut self) {
        if let Some(cb) = self.callback.take() {
            (cb)();
        }
    }
}

#[cfg(any(gles, vulkan, metal))]
impl fmt::Debug for DropGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DropGuard").finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DeviceError {
    #[error("Out of memory")]
    OutOfMemory,
    #[error("Device is lost")]
    Lost,
    #[error("Unexpected error variant (driver implementation is at fault)")]
    Unexpected,
}

#[cfg(any(dx12, vulkan))]
impl From<gpu_allocator::AllocationError> for DeviceError {
    fn from(result: gpu_allocator::AllocationError) -> Self {
        match result {
            gpu_allocator::AllocationError::OutOfMemory => Self::OutOfMemory,
            gpu_allocator::AllocationError::FailedToMap(e) => {
                log::error!("gpu-allocator: Failed to map: {e}");
                Self::Lost
            }
            gpu_allocator::AllocationError::NoCompatibleMemoryTypeFound => {
                log::error!("gpu-allocator: No Compatible Memory Type Found");
                Self::Lost
            }
            gpu_allocator::AllocationError::InvalidAllocationCreateDesc => {
                log::error!("gpu-allocator: Invalid Allocation Creation Description");
                Self::Lost
            }
            gpu_allocator::AllocationError::InvalidAllocatorCreateDesc(e) => {
                log::error!("gpu-allocator: Invalid Allocator Creation Description: {e}");
                Self::Lost
            }

            gpu_allocator::AllocationError::Internal(e) => {
                log::error!("gpu-allocator: Internal Error: {e}");
                Self::Lost
            }
            gpu_allocator::AllocationError::BarrierLayoutNeedsDevice10
            | gpu_allocator::AllocationError::CastableFormatsRequiresEnhancedBarriers
            | gpu_allocator::AllocationError::CastableFormatsRequiresAtLeastDevice12 => {
                unreachable!()
            }
        }
    }
}

#[cfg_attr(not(any(dx12, vulkan)), expect(dead_code))]
pub(crate) struct AllocationSizes {
    pub(crate) min_device_memblock_size: u64,
    pub(crate) max_device_memblock_size: u64,
    pub(crate) min_host_memblock_size: u64,
    pub(crate) max_host_memblock_size: u64,
}

impl AllocationSizes {
    #[allow(dead_code, reason = "may be unused on some platforms")]
    pub(crate) fn from_memory_hints(memory_hints: &wgt::MemoryHints) -> Self {

        const MB: u64 = 1024 * 1024;

        match memory_hints {
            wgt::MemoryHints::Performance => Self {
                min_device_memblock_size: 128 * MB,
                max_device_memblock_size: 256 * MB,
                min_host_memblock_size: 64 * MB,
                max_host_memblock_size: 128 * MB,
            },
            wgt::MemoryHints::MemoryUsage => Self {
                min_device_memblock_size: 8 * MB,
                max_device_memblock_size: 64 * MB,
                min_host_memblock_size: 4 * MB,
                max_host_memblock_size: 32 * MB,
            },
            wgt::MemoryHints::Manual {
                suballocated_device_memory_block_size,
            } => {

                let device_size = suballocated_device_memory_block_size;
                let host_size = device_size.start / 2..device_size.end / 2;

                Self {
                    min_device_memblock_size: device_size.start.clamp(4 * MB, 256 * MB),
                    max_device_memblock_size: device_size.end.clamp(4 * MB, 256 * MB),
                    min_host_memblock_size: host_size.start.clamp(4 * MB, 256 * MB),
                    max_host_memblock_size: host_size.end.clamp(4 * MB, 256 * MB),
                }
            }
        }
    }
}

#[cfg(any(dx12, vulkan))]
impl From<AllocationSizes> for gpu_allocator::AllocationSizes {
    fn from(value: AllocationSizes) -> gpu_allocator::AllocationSizes {
        gpu_allocator::AllocationSizes::new(
            value.min_device_memblock_size,
            value.min_host_memblock_size,
        )
        .with_max_device_memblock_size(value.max_device_memblock_size)
        .with_max_host_memblock_size(value.max_host_memblock_size)
    }
}

#[allow(dead_code, reason = "may be unused on some platforms")]
#[cold]
fn hal_usage_error<T: fmt::Display>(txt: T) -> ! {
    panic!("wgpu-hal invariant was violated (usage error): {txt}")
}

#[allow(dead_code, reason = "may be unused on some platforms")]
#[cold]
fn hal_internal_error<T: fmt::Display>(txt: T) -> ! {
    panic!("wgpu-hal ran into a preventable internal error: {txt}")
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum ShaderError {
    #[error("Compilation failed: {0:?}")]
    Compilation(String),
    #[error(transparent)]
    Device(#[from] DeviceError),
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum PipelineError {
    #[error("Linkage failed for stage {0:?}: {1}")]
    Linkage(wgt::ShaderStages, String),
    #[error("Entry point for stage {0:?} is invalid")]
    EntryPoint(naga::ShaderStage),
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error("Pipeline constant error for stage {0:?}: {1}")]
    PipelineConstants(wgt::ShaderStages, String),
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum PipelineCacheError {
    #[error(transparent)]
    Device(#[from] DeviceError),
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum SurfaceError {
    #[error("Surface is lost")]
    Lost,
    #[error("Surface is outdated, needs to be re-created")]
    Outdated,
    #[error("Timed out waiting for a surface texture")]
    Timeout,
    #[error("The window is occluded (e.g. minimized or behind another window). Try again once the window is no longer occluded.")]
    Occluded,
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error("Other reason: {0}")]
    Other(&'static str),
}

#[derive(Clone, Debug, Error)]
#[error("{message}")]
pub struct InstanceError {

    message: String,

    #[source]
    source: Option<Arc<dyn Error + Send + Sync + 'static>>,
}

impl InstanceError {
    #[allow(dead_code, reason = "may be unused on some platforms")]
    pub(crate) fn new(message: String) -> Self {
        Self {
            message,
            source: None,
        }
    }
    #[allow(dead_code, reason = "may be unused on some platforms")]
    pub(crate) fn with_source(message: String, source: impl Error + Send + Sync + 'static) -> Self {
        cfg_if::cfg_if! {
            if #[cfg(supports_ptr_atomics)] {
                let source = Arc::new(source);
            } else {

                let source: Box<dyn Error + Send + Sync + 'static> = Box::new(source);
                let source = Arc::from(source);
            }
        }
        Self {
            message,
            source: Some(source),
        }
    }
}

pub trait Api: Clone + fmt::Debug + Sized + WasmNotSendSync + 'static {
    const VARIANT: wgt::Backend;

    type Instance: DynInstance + Instance<A = Self>;
    type Surface: DynSurface + Surface<A = Self>;
    type Adapter: DynAdapter + Adapter<A = Self>;
    type Device: DynDevice + Device<A = Self>;

    type Queue: DynQueue + Queue<A = Self>;
    type CommandEncoder: DynCommandEncoder + CommandEncoder<A = Self>;

    type CommandBuffer: DynCommandBuffer;

    type Buffer: DynBuffer;
    type Texture: DynTexture;
    type SurfaceTexture: DynSurfaceTexture + Borrow<Self::Texture>;
    type TextureView: DynTextureView;
    type Sampler: DynSampler;
    type QuerySet: DynQuerySet;

    type Fence: DynFence;

    type BindGroupLayout: DynBindGroupLayout;
    type BindGroup: DynBindGroup;
    type PipelineLayout: DynPipelineLayout;
    type ShaderModule: DynShaderModule;
    type RenderPipeline: DynRenderPipeline;
    type ComputePipeline: DynComputePipeline;
    type RayTracingPipeline: DynRayTracingPipeline;
    type PipelineCache: DynPipelineCache;

    type AccelerationStructure: DynAccelerationStructure + 'static;
}

pub trait Instance: Sized + WasmNotSendSync {
    type A: Api;

    unsafe fn init(desc: &InstanceDescriptor<'_>) -> Result<Self, InstanceError>;
    unsafe fn create_surface(
        &self,
        display_handle: raw_window_handle::RawDisplayHandle,
        window_handle: raw_window_handle::RawWindowHandle,
    ) -> Result<<Self::A as Api>::Surface, InstanceError>;

    unsafe fn enumerate_adapters(
        &self,
        surface_hint: Option<&<Self::A as Api>::Surface>,
    ) -> Vec<ExposedAdapter<Self::A>>;
}

pub trait Surface: WasmNotSendSync {
    type A: Api;

    unsafe fn configure(
        &self,
        device: &<Self::A as Api>::Device,
        config: &SurfaceConfiguration,
    ) -> Result<(), SurfaceError>;

    unsafe fn unconfigure(&self, device: &<Self::A as Api>::Device);

    unsafe fn acquire_texture(
        &self,
        timeout: Option<core::time::Duration>,
        fence: &<Self::A as Api>::Fence,
    ) -> Result<AcquiredSurfaceTexture<Self::A>, SurfaceError>;

    unsafe fn discard_texture(&self, texture: <Self::A as Api>::SurfaceTexture);
}

pub trait Adapter: WasmNotSendSync {
    type A: Api;

    unsafe fn open(
        &self,
        features: wgt::Features,
        limits: &wgt::Limits,
        memory_hints: &wgt::MemoryHints,
    ) -> Result<OpenDevice<Self::A>, DeviceError>;

    unsafe fn texture_format_capabilities(
        &self,
        format: wgt::TextureFormat,
    ) -> TextureFormatCapabilities;

    unsafe fn surface_capabilities(
        &self,
        surface: &<Self::A as Api>::Surface,
    ) -> Option<SurfaceCapabilities>;

    unsafe fn surface_display_hdr_info(
        &self,
        surface: &<Self::A as Api>::Surface,
    ) -> Option<wgt::DisplayHdrInfo> {
        let _ = surface;
        None
    }

    unsafe fn get_presentation_timestamp(&self) -> wgt::PresentationTimestamp;

    fn get_ordered_buffer_usages(&self) -> wgt::BufferUses;

    fn get_ordered_texture_usages(&self) -> wgt::TextureUses;
}

pub trait Device: WasmNotSendSync {
    type A: Api;

    unsafe fn create_buffer(
        &self,
        desc: &BufferDescriptor,
    ) -> Result<<Self::A as Api>::Buffer, DeviceError>;

    unsafe fn destroy_buffer(&self, buffer: <Self::A as Api>::Buffer);

    unsafe fn add_raw_buffer(&self, buffer: &<Self::A as Api>::Buffer);

    unsafe fn map_buffer(
        &self,
        buffer: &<Self::A as Api>::Buffer,
        range: MemoryRange,
    ) -> Result<BufferMapping, DeviceError>;

    unsafe fn unmap_buffer(&self, buffer: &<Self::A as Api>::Buffer);

    unsafe fn flush_mapped_ranges<I>(&self, buffer: &<Self::A as Api>::Buffer, ranges: I)
    where
        I: Iterator<Item = MemoryRange>;

    unsafe fn invalidate_mapped_ranges<I>(&self, buffer: &<Self::A as Api>::Buffer, ranges: I)
    where
        I: Iterator<Item = MemoryRange>;

    unsafe fn create_texture(
        &self,
        desc: &TextureDescriptor,
    ) -> Result<<Self::A as Api>::Texture, DeviceError>;
    unsafe fn destroy_texture(&self, texture: <Self::A as Api>::Texture);

    unsafe fn add_raw_texture(&self, texture: &<Self::A as Api>::Texture);

    unsafe fn create_texture_view(
        &self,
        texture: &<Self::A as Api>::Texture,
        desc: &TextureViewDescriptor,
    ) -> Result<<Self::A as Api>::TextureView, DeviceError>;
    unsafe fn destroy_texture_view(&self, view: <Self::A as Api>::TextureView);
    unsafe fn create_sampler(
        &self,
        desc: &SamplerDescriptor,
    ) -> Result<<Self::A as Api>::Sampler, DeviceError>;
    unsafe fn destroy_sampler(&self, sampler: <Self::A as Api>::Sampler);

    unsafe fn create_command_encoder(
        &self,
        desc: &CommandEncoderDescriptor<<Self::A as Api>::Queue>,
    ) -> Result<<Self::A as Api>::CommandEncoder, DeviceError>;

    unsafe fn create_bind_group_layout(
        &self,
        desc: &BindGroupLayoutDescriptor,
    ) -> Result<<Self::A as Api>::BindGroupLayout, DeviceError>;
    unsafe fn destroy_bind_group_layout(&self, bg_layout: <Self::A as Api>::BindGroupLayout);
    unsafe fn create_pipeline_layout(
        &self,
        desc: &PipelineLayoutDescriptor<<Self::A as Api>::BindGroupLayout>,
    ) -> Result<<Self::A as Api>::PipelineLayout, DeviceError>;
    unsafe fn destroy_pipeline_layout(&self, pipeline_layout: <Self::A as Api>::PipelineLayout);

    #[allow(clippy::type_complexity)]
    unsafe fn create_bind_group(
        &self,
        desc: &BindGroupDescriptor<
            <Self::A as Api>::BindGroupLayout,
            <Self::A as Api>::Buffer,
            <Self::A as Api>::Sampler,
            <Self::A as Api>::TextureView,
            <Self::A as Api>::AccelerationStructure,
        >,
    ) -> Result<<Self::A as Api>::BindGroup, DeviceError>;
    unsafe fn destroy_bind_group(&self, group: <Self::A as Api>::BindGroup);

    unsafe fn create_shader_module(
        &self,
        desc: &ShaderModuleDescriptor,
        shader: ShaderInput,
    ) -> Result<<Self::A as Api>::ShaderModule, ShaderError>;
    unsafe fn destroy_shader_module(&self, module: <Self::A as Api>::ShaderModule);

    #[allow(clippy::type_complexity)]
    unsafe fn create_render_pipeline(
        &self,
        desc: &RenderPipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::RenderPipeline, PipelineError>;
    unsafe fn destroy_render_pipeline(&self, pipeline: <Self::A as Api>::RenderPipeline);

    #[allow(clippy::type_complexity)]
    unsafe fn create_compute_pipeline(
        &self,
        desc: &ComputePipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::ComputePipeline, PipelineError>;
    unsafe fn destroy_compute_pipeline(&self, pipeline: <Self::A as Api>::ComputePipeline);

    #[allow(clippy::type_complexity)]
    unsafe fn create_ray_tracing_pipeline(
        &self,
        desc: &RayTracingPipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::RayTracingPipeline, PipelineError>;
    unsafe fn destroy_ray_tracing_pipeline(&self, pipeline: <Self::A as Api>::RayTracingPipeline);

    unsafe fn get_raytracing_pipeline_group_data(
        &self,
        pipeline: &<Self::A as Api>::RayTracingPipeline,
        groups: Range<u32>,
    ) -> Result<Vec<u8>, DeviceError>;

    unsafe fn create_pipeline_cache(
        &self,
        desc: &PipelineCacheDescriptor<'_>,
    ) -> Result<<Self::A as Api>::PipelineCache, PipelineCacheError>;
    fn pipeline_cache_validation_key(&self) -> Option<[u8; 16]> {
        None
    }
    unsafe fn destroy_pipeline_cache(&self, cache: <Self::A as Api>::PipelineCache);

    unsafe fn create_query_set(
        &self,
        desc: &wgt::QuerySetDescriptor<Label>,
    ) -> Result<<Self::A as Api>::QuerySet, DeviceError>;
    unsafe fn destroy_query_set(&self, set: <Self::A as Api>::QuerySet);
    unsafe fn create_fence(&self) -> Result<<Self::A as Api>::Fence, DeviceError>;
    unsafe fn destroy_fence(&self, fence: <Self::A as Api>::Fence);
    unsafe fn get_fence_value(
        &self,
        fence: &<Self::A as Api>::Fence,
    ) -> Result<FenceValue, DeviceError>;

    unsafe fn wait(
        &self,
        fence: &<Self::A as Api>::Fence,
        value: FenceValue,
        timeout: Option<core::time::Duration>,
    ) -> Result<bool, DeviceError>;

    unsafe fn start_graphics_debugger_capture(&self) -> bool;

    unsafe fn stop_graphics_debugger_capture(&self);

    #[allow(unused_variables)]
    unsafe fn pipeline_cache_get_data(
        &self,
        cache: &<Self::A as Api>::PipelineCache,
    ) -> Option<Vec<u8>> {
        None
    }

    unsafe fn create_acceleration_structure(
        &self,
        desc: &AccelerationStructureDescriptor,
    ) -> Result<<Self::A as Api>::AccelerationStructure, DeviceError>;
    unsafe fn get_acceleration_structure_build_sizes(
        &self,
        desc: &GetAccelerationStructureBuildSizesDescriptor<<Self::A as Api>::Buffer>,
    ) -> AccelerationStructureBuildSizes;
    unsafe fn get_acceleration_structure_device_address(
        &self,
        acceleration_structure: &<Self::A as Api>::AccelerationStructure,
    ) -> wgt::BufferAddress;
    unsafe fn destroy_acceleration_structure(
        &self,
        acceleration_structure: <Self::A as Api>::AccelerationStructure,
    );
    fn tlas_instance_to_bytes(&self, instance: TlasInstance) -> Vec<u8>;

    fn get_internal_counters(&self) -> wgt::HalCounters;

    fn generate_allocator_report(&self) -> Option<wgt::AllocatorReport> {
        None
    }

    fn check_if_oom(&self) -> Result<(), DeviceError>;
}

pub trait Queue: WasmNotSendSync {
    type A: Api;

    unsafe fn submit(
        &self,
        command_buffers: &[&<Self::A as Api>::CommandBuffer],
        surface_textures: &[&<Self::A as Api>::SurfaceTexture],
        signal_fence: (&<Self::A as Api>::Fence, FenceValue),
    ) -> Result<(), DeviceError>;

    unsafe fn present(
        &self,
        surface: &<Self::A as Api>::Surface,
        texture: <Self::A as Api>::SurfaceTexture,
    ) -> Result<(), SurfaceError>;

    unsafe fn wait_for_idle(&self) -> Result<(), DeviceError>;
    unsafe fn get_timestamp_period(&self) -> f32;
}

pub trait CommandEncoder: WasmNotSendSync + fmt::Debug {
    type A: Api;

    unsafe fn begin_encoding(&mut self, label: Label) -> Result<(), DeviceError>;

    unsafe fn discard_encoding(&mut self);

    unsafe fn end_encoding(&mut self) -> Result<<Self::A as Api>::CommandBuffer, DeviceError>;

    unsafe fn reset_all<I>(&mut self, command_buffers: I)
    where
        I: Iterator<Item = <Self::A as Api>::CommandBuffer>;

    unsafe fn transition_buffers<'a, T>(&mut self, barriers: T)
    where
        T: Iterator<Item = BufferBarrier<'a, <Self::A as Api>::Buffer>>;

    unsafe fn transition_textures<'a, T>(&mut self, barriers: T)
    where
        T: Iterator<Item = TextureBarrier<'a, <Self::A as Api>::Texture>>;

    unsafe fn clear_buffer(&mut self, buffer: &<Self::A as Api>::Buffer, range: MemoryRange);

    unsafe fn copy_buffer_to_buffer<T>(
        &mut self,
        src: &<Self::A as Api>::Buffer,
        dst: &<Self::A as Api>::Buffer,
        regions: T,
    ) where
        T: Iterator<Item = BufferCopy>;

    #[cfg(webgl)]
    unsafe fn copy_external_image_to_texture<T>(
        &mut self,
        src: &wgt::CopyExternalImageSourceInfo,
        dst: &<Self::A as Api>::Texture,
        dst_premultiplication: bool,
        regions: T,
    ) where
        T: Iterator<Item = TextureCopy>;

    unsafe fn copy_texture_to_texture<T>(
        &mut self,
        src: &<Self::A as Api>::Texture,
        src_usage: wgt::TextureUses,
        dst: &<Self::A as Api>::Texture,
        regions: T,
    ) where
        T: Iterator<Item = TextureCopy>;

    unsafe fn copy_buffer_to_texture<T>(
        &mut self,
        src: &<Self::A as Api>::Buffer,
        dst: &<Self::A as Api>::Texture,
        regions: T,
    ) where
        T: Iterator<Item = BufferTextureCopy>;

    unsafe fn copy_texture_to_buffer<T>(
        &mut self,
        src: &<Self::A as Api>::Texture,
        src_usage: wgt::TextureUses,
        dst: &<Self::A as Api>::Buffer,
        regions: T,
    ) where
        T: Iterator<Item = BufferTextureCopy>;

    unsafe fn copy_acceleration_structure_to_acceleration_structure(
        &mut self,
        src: &<Self::A as Api>::AccelerationStructure,
        dst: &<Self::A as Api>::AccelerationStructure,
        copy: wgt::AccelerationStructureCopy,
    );

    unsafe fn set_bind_group(
        &mut self,
        layout: &<Self::A as Api>::PipelineLayout,
        index: u32,
        group: &<Self::A as Api>::BindGroup,
        dynamic_offsets: &[wgt::DynamicOffset],
    );

    unsafe fn set_immediates(
        &mut self,
        layout: &<Self::A as Api>::PipelineLayout,
        offset_bytes: u32,
        data: &[u32],
    );

    unsafe fn insert_debug_marker(&mut self, label: &str);
    unsafe fn begin_debug_marker(&mut self, group_label: &str);
    unsafe fn end_debug_marker(&mut self);

    unsafe fn begin_query(&mut self, set: &<Self::A as Api>::QuerySet, index: u32);

    unsafe fn end_query(&mut self, set: &<Self::A as Api>::QuerySet, index: u32);
    unsafe fn write_timestamp(&mut self, set: &<Self::A as Api>::QuerySet, index: u32);
    unsafe fn reset_queries(&mut self, set: &<Self::A as Api>::QuerySet, range: Range<u32>);
    unsafe fn copy_query_results(
        &mut self,
        set: &<Self::A as Api>::QuerySet,
        range: Range<u32>,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        stride: wgt::BufferSize,
    );

    unsafe fn begin_render_pass(
        &mut self,
        desc: &RenderPassDescriptor<<Self::A as Api>::QuerySet, <Self::A as Api>::TextureView>,
    ) -> Result<(), DeviceError>;

    unsafe fn end_render_pass(&mut self);

    unsafe fn set_render_pipeline(&mut self, pipeline: &<Self::A as Api>::RenderPipeline);

    unsafe fn set_index_buffer<'a>(
        &mut self,
        binding: BufferBinding<'a, <Self::A as Api>::Buffer>,
        format: wgt::IndexFormat,
    );
    unsafe fn set_vertex_buffer<'a>(
        &mut self,
        index: u32,
        binding: BufferBinding<'a, <Self::A as Api>::Buffer>,
    );
    unsafe fn set_viewport(&mut self, rect: &Rect<f32>, depth_range: Range<f32>);
    unsafe fn set_scissor_rect(&mut self, rect: &Rect<u32>);
    unsafe fn set_stencil_reference(&mut self, value: u32);
    unsafe fn set_blend_constants(&mut self, color: &[f32; 4]);

    unsafe fn draw(
        &mut self,
        first_vertex: u32,
        vertex_count: u32,
        first_instance: u32,
        instance_count: u32,
    );
    unsafe fn draw_indexed(
        &mut self,
        first_index: u32,
        index_count: u32,
        base_vertex: i32,
        first_instance: u32,
        instance_count: u32,
    );
    unsafe fn draw_indirect(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        draw_count: u32,
    );
    unsafe fn draw_indexed_indirect(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        draw_count: u32,
    );
    unsafe fn draw_indirect_count(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        count_buffer: &<Self::A as Api>::Buffer,
        count_offset: wgt::BufferAddress,
        max_count: u32,
    );
    unsafe fn draw_indexed_indirect_count(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        count_buffer: &<Self::A as Api>::Buffer,
        count_offset: wgt::BufferAddress,
        max_count: u32,
    );
    unsafe fn draw_mesh_tasks(
        &mut self,
        group_count_x: u32,
        group_count_y: u32,
        group_count_z: u32,
    );
    unsafe fn draw_mesh_tasks_indirect(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        draw_count: u32,
    );
    unsafe fn draw_mesh_tasks_indirect_count(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        count_buffer: &<Self::A as Api>::Buffer,
        count_offset: wgt::BufferAddress,
        max_count: u32,
    );

    unsafe fn begin_compute_pass(
        &mut self,
        desc: &ComputePassDescriptor<<Self::A as Api>::QuerySet>,
    );

    unsafe fn end_compute_pass(&mut self);

    unsafe fn set_compute_pipeline(&mut self, pipeline: &<Self::A as Api>::ComputePipeline);

    unsafe fn dispatch_workgroups(&mut self, count: [u32; 3]);
    unsafe fn dispatch_workgroups_indirect(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
    );

    unsafe fn begin_ray_tracing_pass(&mut self, desc: &RayTracingPassDescriptor);

    unsafe fn end_ray_tracing_pass(&mut self);

    unsafe fn set_ray_tracing_pipeline(&mut self, pipeline: &<Self::A as Api>::RayTracingPipeline);

    unsafe fn trace_rays<'a>(
        &mut self,
        count: [u32; 3],
        ray_generation_group_data: PipelineGroupData<'a, <Self::A as Api>::Buffer>,
        miss_group_data: PipelineGroupData<'a, <Self::A as Api>::Buffer>,
        intersection_group_data: PipelineGroupData<'a, <Self::A as Api>::Buffer>,
    );

    unsafe fn build_acceleration_structures<'a, T>(
        &mut self,
        descriptor_count: u32,
        descriptors: T,
    ) where
        Self::A: 'a,
        T: IntoIterator<
            Item = BuildAccelerationStructureDescriptor<
                'a,
                <Self::A as Api>::Buffer,
                <Self::A as Api>::AccelerationStructure,
            >,
        >;
    unsafe fn place_acceleration_structure_barrier(
        &mut self,
        barrier: AccelerationStructureBarrier,
    );

    unsafe fn read_acceleration_structure_compact_size(
        &mut self,
        acceleration_structure: &<Self::A as Api>::AccelerationStructure,
        buf: &<Self::A as Api>::Buffer,
    );
    unsafe fn set_acceleration_structure_dependencies(
        command_buffers: &[&<Self::A as Api>::CommandBuffer],
        dependencies: &[&<Self::A as Api>::AccelerationStructure],
    );
}

bitflags!(

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct PipelineLayoutFlags: u32 {

        const FIRST_VERTEX_INSTANCE = 1 << 0;

        const NUM_WORK_GROUPS = 1 << 1;

        const INDIRECT_BUILTIN_UPDATE = 1 << 2;
    }
);

bitflags!(

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct BindGroupLayoutFlags: u32 {

        const PARTIALLY_BOUND = 1 << 0;
    }
);

bitflags!(

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct TextureFormatCapabilities: u32 {

        const SAMPLED = 1 << 0;

        const SAMPLED_LINEAR = 1 << 1;

        const SAMPLED_MINMAX = 1 << 2;

        const STORAGE_READ_ONLY = 1 << 3;

        const STORAGE_WRITE_ONLY = 1 << 4;

        const STORAGE_READ_WRITE = 1 << 5;

        const STORAGE_ATOMIC = 1 << 6;

        const COLOR_ATTACHMENT = 1 << 7;

        const COLOR_ATTACHMENT_BLEND = 1 << 8;

        const DEPTH_STENCIL_ATTACHMENT = 1 << 9;

        const MULTISAMPLE_X2   = 1 << 10;

        const MULTISAMPLE_X4   = 1 << 11;

        const MULTISAMPLE_X8   = 1 << 12;

        const MULTISAMPLE_X16  = 1 << 13;

        const MULTISAMPLE_RESOLVE = 1 << 14;

        const COPY_SRC = 1 << 15;

        const COPY_DST = 1 << 16;
    }
);

bitflags!(

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct FormatAspects: u8 {
        const COLOR = 1 << 0;
        const DEPTH = 1 << 1;
        const STENCIL = 1 << 2;
        const PLANE_0 = 1 << 3;
        const PLANE_1 = 1 << 4;
        const PLANE_2 = 1 << 5;

        const DEPTH_STENCIL = Self::DEPTH.bits() | Self::STENCIL.bits();
    }
);

impl FormatAspects {
    pub fn new(format: wgt::TextureFormat, aspect: wgt::TextureAspect) -> Self {
        let aspect_mask = match aspect {
            wgt::TextureAspect::All => Self::all(),
            wgt::TextureAspect::DepthOnly => Self::DEPTH,
            wgt::TextureAspect::StencilOnly => Self::STENCIL,
            wgt::TextureAspect::Plane0 => Self::PLANE_0,
            wgt::TextureAspect::Plane1 => Self::PLANE_1,
            wgt::TextureAspect::Plane2 => Self::PLANE_2,
        };
        Self::from(format) & aspect_mask
    }

    pub fn is_one(&self) -> bool {
        self.bits().is_power_of_two()
    }

    pub fn map(&self) -> wgt::TextureAspect {
        match *self {
            Self::COLOR => wgt::TextureAspect::All,
            Self::DEPTH => wgt::TextureAspect::DepthOnly,
            Self::STENCIL => wgt::TextureAspect::StencilOnly,
            Self::PLANE_0 => wgt::TextureAspect::Plane0,
            Self::PLANE_1 => wgt::TextureAspect::Plane1,
            Self::PLANE_2 => wgt::TextureAspect::Plane2,
            _ => unreachable!(),
        }
    }
}

impl From<wgt::TextureFormat> for FormatAspects {
    fn from(format: wgt::TextureFormat) -> Self {
        match format {
            wgt::TextureFormat::Stencil8 => Self::STENCIL,
            wgt::TextureFormat::Depth16Unorm
            | wgt::TextureFormat::Depth32Float
            | wgt::TextureFormat::Depth24Plus => Self::DEPTH,
            wgt::TextureFormat::Depth32FloatStencil8 | wgt::TextureFormat::Depth24PlusStencil8 => {
                Self::DEPTH_STENCIL
            }
            wgt::TextureFormat::NV12 | wgt::TextureFormat::P010 => Self::PLANE_0 | Self::PLANE_1,
            _ => Self::COLOR,
        }
    }
}

bitflags!(
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct MemoryFlags: u32 {
        const TRANSIENT = 1 << 0;
        const PREFER_COHERENT = 1 << 1;
    }
);

bitflags!(

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct AttachmentOps: u8 {

        const LOAD = 1 << 0;

        const LOAD_CLEAR = 1 << 1;

        const LOAD_DONT_CARE = 1 << 2;

        const STORE = 1 << 3;

        const STORE_DISCARD = 1 << 4;
    }
);

#[derive(Debug)]
pub struct InstanceDescriptor<'a> {
    pub name: &'a str,
    pub flags: wgt::InstanceFlags,
    pub memory_budget_thresholds: wgt::MemoryBudgetThresholds,
    pub backend_options: wgt::BackendOptions,
    pub telemetry: Option<Telemetry>,

    pub display: Option<DisplayHandle<'a>>,
}

#[derive(Clone, Debug)]
pub struct Alignments {

    pub buffer_copy_offset: wgt::BufferSize,

    pub buffer_copy_pitch: wgt::BufferSize,

    pub uniform_bounds_check_alignment: wgt::BufferSize,

    pub raw_tlas_instance_size: u32,

    pub ray_tracing_scratch_buffer_alignment: u32,

    pub ray_tracing_pipeline_group_data_size: u32,

    pub ray_tracing_pipeline_group_data_alignment: u32,

    pub ray_tracing_pipeline_data_offset_alignment: u32,
}

#[derive(Clone, Debug)]
pub struct Capabilities {
    pub limits: wgt::Limits,
    pub alignments: Alignments,
    pub downlevel: wgt::DownlevelCapabilities,

    pub cooperative_matrix_properties: Vec<wgt::CooperativeMatrixProperties>,
}

#[derive(Debug)]
pub struct ExposedAdapter<A: Api> {
    pub adapter: A::Adapter,
    pub info: wgt::AdapterInfo,
    pub features: wgt::Features,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone)]
pub struct SurfaceCapabilities {

    pub formats: Vec<wgt::SurfaceFormatCapabilities>,

    pub maximum_frame_latency: RangeInclusive<u32>,

    pub current_extent: Option<wgt::Extent3d>,

    pub usage: wgt::TextureUses,

    pub present_modes: Vec<wgt::PresentMode>,

    pub composite_alpha_modes: Vec<wgt::CompositeAlphaMode>,
}

impl SurfaceCapabilities {

    pub fn texture_formats(&self) -> impl Iterator<Item = wgt::TextureFormat> + '_ {
        self.formats.iter().map(|fc| fc.format)
    }
}

#[derive(Debug)]
pub struct AcquiredSurfaceTexture<A: Api> {
    pub texture: A::SurfaceTexture,

    pub suboptimal: bool,
}

#[derive(Debug)]
pub struct OpenDevice<A: Api> {
    pub device: A::Device,
    pub queue: A::Queue,
}

#[derive(Clone, Debug)]
pub struct BufferMapping {
    pub ptr: NonNull<u8>,
    pub is_coherent: bool,
}

#[derive(Clone, Debug)]
pub struct BufferDescriptor<'a> {
    pub label: Label<'a>,
    pub size: wgt::BufferAddress,
    pub usage: wgt::BufferUses,
    pub memory_flags: MemoryFlags,
}

#[derive(Clone, Debug)]
pub struct TextureDescriptor<'a> {
    pub label: Label<'a>,
    pub size: wgt::Extent3d,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub dimension: wgt::TextureDimension,
    pub format: wgt::TextureFormat,
    pub usage: wgt::TextureUses,
    pub memory_flags: MemoryFlags,

    pub view_formats: Vec<wgt::TextureFormat>,
}

impl TextureDescriptor<'_> {
    pub fn copy_extent(&self) -> CopyExtent {
        CopyExtent::map_extent_to_copy_size(&self.size, self.dimension)
    }

    pub fn is_cube_compatible(&self) -> bool {
        self.dimension == wgt::TextureDimension::D2
            && self.size.depth_or_array_layers.is_multiple_of(6)
            && self.sample_count == 1
            && self.size.width == self.size.height
    }

    pub fn array_layer_count(&self) -> u32 {
        match self.dimension {
            wgt::TextureDimension::D1 | wgt::TextureDimension::D3 => 1,
            wgt::TextureDimension::D2 => self.size.depth_or_array_layers,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextureViewDescriptor<'a> {
    pub label: Label<'a>,
    pub format: wgt::TextureFormat,
    pub dimension: wgt::TextureViewDimension,
    pub usage: wgt::TextureUses,
    pub range: wgt::ImageSubresourceRange,
}

#[derive(Clone, Debug)]
pub struct SamplerDescriptor<'a> {
    pub label: Label<'a>,
    pub address_modes: [wgt::AddressMode; 3],
    pub mag_filter: wgt::FilterMode,
    pub min_filter: wgt::FilterMode,
    pub mipmap_filter: wgt::MipmapFilterMode,
    pub lod_clamp: Range<f32>,
    pub compare: Option<wgt::CompareFunction>,

    pub anisotropy_clamp: u16,
    pub border_color: Option<wgt::SamplerBorderColor>,
}

#[derive(Clone, Debug)]
pub struct BindGroupLayoutDescriptor<'a> {
    pub label: Label<'a>,
    pub flags: BindGroupLayoutFlags,
    pub entries: &'a [wgt::BindGroupLayoutEntry],
}

#[derive(Clone, Debug)]
pub struct PipelineLayoutDescriptor<'a, B: DynBindGroupLayout + ?Sized> {
    pub label: Label<'a>,
    pub flags: PipelineLayoutFlags,
    pub bind_group_layouts: &'a [Option<&'a B>],
    pub immediate_size: u32,
}

#[derive(Debug)]
pub struct BufferBinding<'a, B: DynBuffer + ?Sized> {

    pub(crate) buffer: &'a B,

    pub offset: wgt::BufferAddress,

    pub size: Option<wgt::BufferSize>,
}

impl<B: DynBuffer + ?Sized> Clone for BufferBinding<'_, B> {
    fn clone(&self) -> Self {
        BufferBinding {
            buffer: self.buffer,
            offset: self.offset,
            size: self.size,
        }
    }
}

pub trait ShouldBeNonZeroExt {
    fn get(&self) -> u64;
}

impl ShouldBeNonZeroExt for NonZeroU64 {
    fn get(&self) -> u64 {
        NonZeroU64::get(*self)
    }
}

impl ShouldBeNonZeroExt for u64 {
    fn get(&self) -> u64 {
        *self
    }
}

impl ShouldBeNonZeroExt for Option<NonZeroU64> {
    fn get(&self) -> u64 {
        match *self {
            Some(non_zero) => non_zero.get(),
            None => 0,
        }
    }
}

impl<'a, B: DynBuffer + ?Sized> BufferBinding<'a, B> {

    pub fn new_unchecked<S: Into<Option<NonZeroU64>>>(
        buffer: &'a B,
        offset: wgt::BufferAddress,
        size: S,
    ) -> Self {
        Self {
            buffer,
            offset,
            size: size.into(),
        }
    }
}

#[derive(Debug)]
pub struct TextureBinding<'a, T: DynTextureView + ?Sized> {
    pub view: &'a T,
    pub usage: wgt::TextureUses,
}

impl<'a, T: DynTextureView + ?Sized> Clone for TextureBinding<'a, T> {
    fn clone(&self) -> Self {
        TextureBinding {
            view: self.view,
            usage: self.usage,
        }
    }
}

#[derive(Debug)]
pub struct ExternalTextureBinding<'a, B: DynBuffer + ?Sized, T: DynTextureView + ?Sized> {
    pub planes: [TextureBinding<'a, T>; 3],
    pub params: BufferBinding<'a, B>,
}

impl<'a, B: DynBuffer + ?Sized, T: DynTextureView + ?Sized> Clone
    for ExternalTextureBinding<'a, B, T>
{
    fn clone(&self) -> Self {
        ExternalTextureBinding {
            planes: self.planes.clone(),
            params: self.params.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BindGroupEntry {
    pub binding: u32,
    pub resource_index: u32,
    pub count: u32,
}

#[derive(Clone, Debug)]
pub struct BindGroupDescriptor<
    'a,
    Bgl: DynBindGroupLayout + ?Sized,
    B: DynBuffer + ?Sized,
    S: DynSampler + ?Sized,
    T: DynTextureView + ?Sized,
    A: DynAccelerationStructure + ?Sized,
> {
    pub label: Label<'a>,
    pub layout: &'a Bgl,
    pub buffers: &'a [BufferBinding<'a, B>],
    pub samplers: &'a [&'a S],
    pub textures: &'a [TextureBinding<'a, T>],
    pub entries: &'a [BindGroupEntry],
    pub acceleration_structures: &'a [&'a A],
    pub external_textures: &'a [ExternalTextureBinding<'a, B, T>],
}

#[derive(Clone, Debug)]
pub struct CommandEncoderDescriptor<'a, Q: DynQueue + ?Sized> {
    pub label: Label<'a>,
    pub queue: &'a Q,
}

#[derive(Default)]
pub struct NagaShader {

    pub module: Cow<'static, naga::Module>,

    pub info: naga::valid::ModuleInfo,

    pub debug_source: Option<DebugSource>,
}

impl fmt::Debug for NagaShader {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "Naga shader")
    }
}

pub enum ShaderInput<'a> {
    Naga(NagaShader),
    MetalLib {
        file: &'a [u8],
        num_workgroups: hashbrown::HashMap<String, (u32, u32, u32)>,
    },
    Msl {
        shader: &'a str,
        num_workgroups: hashbrown::HashMap<String, (u32, u32, u32)>,
    },
    SpirV(&'a [u32]),
    Dxil {
        shader: &'a [u8],
    },
    Hlsl {
        shader: &'a str,
    },
    Glsl {
        shader: &'a str,
    },
}

impl fmt::Debug for ShaderInput<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {

            Self::Naga { .. } => f.debug_tuple("Naga").finish_non_exhaustive(),
            Self::MetalLib { .. } => f.debug_tuple("MetalLib").finish_non_exhaustive(),
            Self::Msl { .. } => f.debug_tuple("Msl").finish_non_exhaustive(),
            Self::SpirV { .. } => f.debug_tuple("SpirV").finish_non_exhaustive(),
            Self::Dxil { .. } => f.debug_tuple("Dxil").finish_non_exhaustive(),
            Self::Hlsl { .. } => f.debug_tuple("Hlsl").finish_non_exhaustive(),
            Self::Glsl { .. } => f.debug_tuple("Glsl").finish_non_exhaustive(),
        }
    }
}

#[derive(Debug)]
pub struct ShaderModuleDescriptor<'a> {
    pub label: Label<'a>,

    pub runtime_checks: wgt::ShaderRuntimeChecks,
}

#[derive(Debug, Clone)]
pub struct DebugSource {
    pub file_name: Cow<'static, str>,
    pub source_code: Cow<'static, str>,
}

#[derive(Debug)]
pub struct ProgrammableStage<'a, M: DynShaderModule + ?Sized> {

    pub module: &'a M,

    pub entry_point: &'a str,

    pub constants: &'a naga::back::PipelineConstants,

    pub zero_initialize_workgroup_memory: bool,
}

impl<M: DynShaderModule + ?Sized> Clone for ProgrammableStage<'_, M> {
    fn clone(&self) -> Self {
        Self {
            module: self.module,
            entry_point: self.entry_point,
            constants: self.constants,
            zero_initialize_workgroup_memory: self.zero_initialize_workgroup_memory,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ComputePipelineDescriptor<
    'a,
    Pl: DynPipelineLayout + ?Sized,
    M: DynShaderModule + ?Sized,
    Pc: DynPipelineCache + ?Sized,
> {
    pub label: Label<'a>,

    pub layout: &'a Pl,

    pub stage: ProgrammableStage<'a, M>,

    pub cache: Option<&'a Pc>,
}

#[derive(Debug)]
pub struct PipelineCacheDescriptor<'a> {
    pub label: Label<'a>,
    pub data: Option<&'a [u8]>,
}

#[derive(Clone, Debug)]
pub struct VertexBufferLayout<'a> {

    pub array_stride: wgt::BufferAddress,

    pub step_mode: wgt::VertexStepMode,

    pub attributes: &'a [wgt::VertexAttribute],
}

#[derive(Clone, Debug)]
pub enum VertexProcessor<'a, M: DynShaderModule + ?Sized> {
    Standard {

        vertex_buffers: &'a [Option<VertexBufferLayout<'a>>],

        vertex_stage: ProgrammableStage<'a, M>,
    },
    Mesh {
        task_stage: Option<ProgrammableStage<'a, M>>,
        mesh_stage: ProgrammableStage<'a, M>,
    },
}

#[derive(Clone, Debug)]
pub struct RenderPipelineDescriptor<
    'a,
    Pl: DynPipelineLayout + ?Sized,
    M: DynShaderModule + ?Sized,
    Pc: DynPipelineCache + ?Sized,
> {
    pub label: Label<'a>,

    pub layout: &'a Pl,

    pub vertex_processor: VertexProcessor<'a, M>,

    pub primitive: wgt::PrimitiveState,

    pub depth_stencil: Option<wgt::DepthStencilState>,

    pub multisample: wgt::MultisampleState,

    pub fragment_stage: Option<ProgrammableStage<'a, M>>,

    pub color_targets: &'a [Option<wgt::ColorTargetState>],

    pub multiview_mask: Option<NonZeroU32>,

    pub cache: Option<&'a Pc>,
}

#[derive(Clone, Debug)]
pub struct RayObjectIntersectionState<'a, M: DynShaderModule + ?Sized> {
    pub closest_hit: ProgrammableStage<'a, M>,
    pub any_hit: Option<ProgrammableStage<'a, M>>,
}

#[derive(Clone, Debug)]
pub struct RayTracingPipelineDescriptor<
    'a,
    Pl: DynPipelineLayout + ?Sized,
    M: DynShaderModule + ?Sized,
    Pc: DynPipelineCache + ?Sized,
> {
    pub label: Label<'a>,

    pub layout: &'a Pl,

    pub ray_generation: ProgrammableStage<'a, M>,

    pub miss: ProgrammableStage<'a, M>,

    pub intersection: &'a [RayObjectIntersectionState<'a, M>],

    pub max_recursion_depth: u32,

    pub cache: Option<&'a Pc>,
}

#[derive(Debug, Clone)]
pub struct SurfaceConfiguration {

    pub maximum_frame_latency: u32,

    pub present_mode: wgt::PresentMode,

    pub composite_alpha_mode: wgt::CompositeAlphaMode,

    pub format: wgt::TextureFormat,

    pub color_space: wgt::SurfaceColorSpace,

    pub extent: wgt::Extent3d,

    pub usage: wgt::TextureUses,

    pub view_formats: Vec<wgt::TextureFormat>,
}

#[derive(Debug, Clone)]
pub struct Rect<T> {
    pub x: T,
    pub y: T,
    pub w: T,
    pub h: T,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateTransition<T> {
    pub from: T,
    pub to: T,
}

#[derive(Debug, Clone)]
pub struct BufferBarrier<'a, B: DynBuffer + ?Sized> {
    pub buffer: &'a B,
    pub usage: StateTransition<wgt::BufferUses>,
}

#[derive(Debug, Clone)]
pub struct TextureBarrier<'a, T: DynTexture + ?Sized> {
    pub texture: &'a T,
    pub range: wgt::ImageSubresourceRange,
    pub usage: StateTransition<wgt::TextureUses>,
}

#[derive(Clone, Copy, Debug)]
pub struct BufferCopy {
    pub src_offset: wgt::BufferAddress,
    pub dst_offset: wgt::BufferAddress,
    pub size: wgt::BufferSize,
}

#[derive(Clone, Debug)]
pub struct TextureCopyBase {
    pub mip_level: u32,
    pub array_layer: u32,

    pub origin: wgt::Origin3d,
    pub aspect: FormatAspects,
}

#[derive(Clone, Copy, Debug)]
pub struct CopyExtent {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

impl From<wgt::Extent3d> for CopyExtent {
    fn from(value: wgt::Extent3d) -> Self {
        let wgt::Extent3d {
            width,
            height,
            depth_or_array_layers,
        } = value;
        Self {
            width,
            height,
            depth: depth_or_array_layers,
        }
    }
}

impl From<CopyExtent> for wgt::Extent3d {
    fn from(value: CopyExtent) -> Self {
        let CopyExtent {
            width,
            height,
            depth,
        } = value;
        Self {
            width,
            height,
            depth_or_array_layers: depth,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextureCopy {
    pub src_base: TextureCopyBase,
    pub dst_base: TextureCopyBase,
    pub size: CopyExtent,
}

#[derive(Clone, Debug)]
pub struct BufferTextureCopy {
    pub buffer_layout: wgt::TexelCopyBufferLayout,
    pub texture_base: TextureCopyBase,
    pub size: CopyExtent,
}

#[derive(Clone, Debug)]
pub struct Attachment<'a, T: DynTextureView + ?Sized> {
    pub view: &'a T,

    pub usage: wgt::TextureUses,
}

#[derive(Clone, Debug)]
pub struct ColorAttachment<'a, T: DynTextureView + ?Sized> {
    pub target: Attachment<'a, T>,
    pub depth_slice: Option<u32>,
    pub resolve_target: Option<Attachment<'a, T>>,
    pub ops: AttachmentOps,
    pub clear_value: wgt::Color,
}

#[derive(Clone, Debug)]
pub struct DepthStencilAttachment<'a, T: DynTextureView + ?Sized> {
    pub target: Attachment<'a, T>,
    pub depth_ops: AttachmentOps,
    pub stencil_ops: AttachmentOps,
    pub clear_value: (f32, u32),
}

#[derive(Clone, Debug)]
pub struct PassTimestampWrites<'a, Q: DynQuerySet + ?Sized> {
    pub query_set: &'a Q,
    pub beginning_of_pass_write_index: Option<u32>,
    pub end_of_pass_write_index: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct RenderPassDescriptor<'a, Q: DynQuerySet + ?Sized, T: DynTextureView + ?Sized> {
    pub label: Label<'a>,
    pub extent: wgt::Extent3d,
    pub sample_count: u32,
    pub color_attachments: &'a [Option<ColorAttachment<'a, T>>],
    pub depth_stencil_attachment: Option<DepthStencilAttachment<'a, T>>,
    pub multiview_mask: Option<NonZeroU32>,
    pub timestamp_writes: Option<PassTimestampWrites<'a, Q>>,
    pub occlusion_query_set: Option<&'a Q>,
}

#[derive(Clone, Debug)]
pub struct ComputePassDescriptor<'a, Q: DynQuerySet + ?Sized> {
    pub label: Label<'a>,
    pub timestamp_writes: Option<PassTimestampWrites<'a, Q>>,
}

#[derive(Clone, Debug)]
pub struct RayTracingPassDescriptor<'a> {
    pub label: Label<'a>,
}

#[test]
fn test_default_limits() {
    let limits = wgt::Limits::default();
    assert!(limits.max_bind_groups <= MAX_BIND_GROUPS as u32);
}

#[derive(Clone, Debug)]
pub struct AccelerationStructureDescriptor<'a> {
    pub label: Label<'a>,
    pub size: wgt::BufferAddress,
    pub format: AccelerationStructureFormat,
    pub allow_compaction: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AccelerationStructureFormat {
    TopLevel,
    BottomLevel,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AccelerationStructureBuildMode {
    Build,
    Update,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct AccelerationStructureBuildSizes {
    pub acceleration_structure_size: wgt::BufferAddress,
    pub update_scratch_size: wgt::BufferAddress,
    pub build_scratch_size: wgt::BufferAddress,
}

#[derive(Clone, Debug)]
pub struct BuildAccelerationStructureDescriptor<
    'a,
    B: DynBuffer + ?Sized,
    A: DynAccelerationStructure + ?Sized,
> {
    pub entries: &'a AccelerationStructureEntries<'a, B>,
    pub mode: AccelerationStructureBuildMode,
    pub flags: AccelerationStructureBuildFlags,
    pub source_acceleration_structure: Option<&'a A>,
    pub destination_acceleration_structure: &'a A,
    pub scratch_buffer: &'a B,
    pub scratch_buffer_offset: wgt::BufferAddress,
}

#[derive(Clone, Debug)]
pub struct GetAccelerationStructureBuildSizesDescriptor<'a, B: DynBuffer + ?Sized> {
    pub entries: &'a AccelerationStructureEntries<'a, B>,
    pub flags: AccelerationStructureBuildFlags,
}

#[derive(Debug)]
pub enum AccelerationStructureEntries<'a, B: DynBuffer + ?Sized> {
    Instances(AccelerationStructureInstances<'a, B>),
    Triangles(Vec<AccelerationStructureTriangles<'a, B>>),
    AABBs(Vec<AccelerationStructureAABBs<'a, B>>),
}

#[derive(Clone, Debug)]
pub struct AccelerationStructureTriangles<'a, B: DynBuffer + ?Sized> {
    pub vertex_buffer: Option<&'a B>,
    pub vertex_format: wgt::VertexFormat,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub vertex_stride: wgt::BufferAddress,
    pub indices: Option<AccelerationStructureTriangleIndices<'a, B>>,
    pub transform: Option<AccelerationStructureTriangleTransform<'a, B>>,
    pub flags: AccelerationStructureGeometryFlags,
}

#[derive(Clone, Debug)]
pub struct AccelerationStructureAABBs<'a, B: DynBuffer + ?Sized> {
    pub buffer: Option<&'a B>,
    pub offset: u32,
    pub count: u32,
    pub stride: wgt::BufferAddress,
    pub flags: AccelerationStructureGeometryFlags,
}

#[derive(Clone, Debug)]
pub struct AccelerationStructureCopy {
    pub copy_flags: wgt::AccelerationStructureCopy,
    pub type_flags: wgt::AccelerationStructureType,
}

#[derive(Clone, Debug)]
pub struct AccelerationStructureInstances<'a, B: DynBuffer + ?Sized> {
    pub buffer: Option<&'a B>,
    pub offset: u32,
    pub count: u32,
}

#[derive(Clone, Debug)]
pub struct AccelerationStructureTriangleIndices<'a, B: DynBuffer + ?Sized> {
    pub format: wgt::IndexFormat,
    pub buffer: Option<&'a B>,
    pub offset: u32,
    pub count: u32,
}

#[derive(Clone, Debug)]
pub struct AccelerationStructureTriangleTransform<'a, B: DynBuffer + ?Sized> {
    pub buffer: &'a B,
    pub offset: u32,
}

pub use wgt::AccelerationStructureFlags as AccelerationStructureBuildFlags;
pub use wgt::AccelerationStructureGeometryFlags;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct AccelerationStructureUses: u8 {

        const BUILD_INPUT = 1 << 0;

        const BUILD_OUTPUT = 1 << 1;

        const SHADER_INPUT = 1 << 2;

        const QUERY_INPUT = 1 << 3;

        const COPY_SRC = 1 << 4;

        const COPY_DST = 1 << 5;
    }
}

#[derive(Debug, Clone)]
pub struct AccelerationStructureBarrier {
    pub usage: StateTransition<AccelerationStructureUses>,
}

#[derive(Debug, Copy, Clone)]
pub struct TlasInstance {
    pub transform: [f32; 12],
    pub custom_data: u32,
    pub mask: u8,
    pub blas_address: u64,

    pub pipeline_intersection_data_offset: u32,
}

#[cfg(dx12)]
#[derive(Debug)]
pub enum D3D12ExposeAdapterResult {
    CreateDeviceError(dx12::CreateDeviceError),
    UnknownFeatureLevel(i32),
    ResourceBindingTier2Requirement,
    ShaderModel6Requirement,
    Success(dx12::FeatureLevel, dx12::ShaderModel),
}

#[derive(Debug, Clone, Copy)]
pub struct Telemetry {
    #[cfg(dx12)]
    pub d3d12_expose_adapter: fn(
        desc: &windows::Win32::Graphics::Dxgi::DXGI_ADAPTER_DESC2,
        driver_version: Result<[u16; 4], windows_core::HRESULT>,
        result: D3D12ExposeAdapterResult,
    ),
}

#[derive(Debug)]
pub struct PipelineGroupData<'a, B: DynBuffer + ?Sized> {
    pub buffer: &'a B,
    pub offset: wgt::BufferAddress,
    pub stride: u64,
    pub count: u64,
}
