mod adapter;
mod command;
pub mod conv;
mod descriptor;
mod device;
mod drm;
mod instance;
mod sampler;
mod semaphore_list;
mod swapchain;

pub use adapter::PhysicalDeviceFeatures;

use alloc::{boxed::Box, ffi::CString, sync::Arc, vec::Vec};
use core::{borrow::Borrow, ffi::CStr, fmt, marker::PhantomData, mem, num::NonZeroU32};

use arrayvec::ArrayVec;
use ash::{ext, khr, vk};
use bytemuck::{Pod, Zeroable};
use hashbrown::HashSet;
use parking_lot::{Mutex, RwLock};

use naga::FastHashMap;
use wgt::InternalCounter;

use semaphore_list::SemaphoreList;

use crate::vulkan::semaphore_list::{SemaphoreListMode, SemaphoreType};

const MAX_TOTAL_ATTACHMENTS: usize = crate::MAX_COLOR_ATTACHMENTS * 2 + 1;

#[derive(Clone, Debug)]
pub struct Api;

impl crate::Api for Api {
    const VARIANT: wgt::Backend = wgt::Backend::Vulkan;

    type Instance = Instance;
    type Surface = Surface;
    type Adapter = Adapter;
    type Device = Device;

    type Queue = Queue;
    type CommandEncoder = CommandEncoder;
    type CommandBuffer = CommandBuffer;

    type Buffer = Buffer;
    type Texture = Texture;
    type SurfaceTexture = SurfaceTexture;
    type TextureView = TextureView;
    type Sampler = Sampler;
    type QuerySet = QuerySet;
    type Fence = Fence;
    type AccelerationStructure = AccelerationStructure;
    type PipelineCache = PipelineCache;

    type BindGroupLayout = BindGroupLayout;
    type BindGroup = BindGroup;
    type PipelineLayout = PipelineLayout;
    type ShaderModule = ShaderModule;
    type RenderPipeline = RenderPipeline;
    type ComputePipeline = ComputePipeline;
    type RayTracingPipeline = RayTracingPipeline;
}

crate::impl_dyn_resource!(
    Adapter,
    AccelerationStructure,
    BindGroup,
    BindGroupLayout,
    Buffer,
    CommandBuffer,
    CommandEncoder,
    ComputePipeline,
    Device,
    Fence,
    Instance,
    PipelineCache,
    PipelineLayout,
    QuerySet,
    Queue,
    RenderPipeline,
    RayTracingPipeline,
    Sampler,
    ShaderModule,
    Surface,
    SurfaceTexture,
    Texture,
    TextureView
);

struct DebugUtils {
    extension: ext::debug_utils::Instance,
    messenger: vk::DebugUtilsMessengerEXT,

    #[allow(dead_code)]
    callback_data: Box<DebugUtilsMessengerUserData>,
}

#[derive(Debug)]
pub struct DebugUtilsCreateInfo {
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: Box<DebugUtilsMessengerUserData>,
}

#[derive(Debug)]

struct ValidationLayerProperties {

    layer_description: CString,

    layer_spec_version: u32,
}

#[derive(Debug)]
pub struct DebugUtilsMessengerUserData {

    validation_layer_properties: Option<ValidationLayerProperties>,

    has_obs_layer: bool,
}

pub struct InstanceShared {
    raw: ash::Instance,
    extensions: Vec<&'static CStr>,
    flags: wgt::InstanceFlags,
    memory_budget_thresholds: wgt::MemoryBudgetThresholds,
    debug_utils: Option<DebugUtils>,
    get_physical_device_properties: Option<khr::get_physical_device_properties2::Instance>,
    entry: ash::Entry,
    has_nv_optimus: bool,
    android_sdk_version: u32,

    instance_api_version: u32,

    drop_guard: Option<crate::DropGuard>,
}

impl fmt::Debug for InstanceShared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            raw: _,
            extensions,
            flags,
            memory_budget_thresholds,
            debug_utils: _,
            get_physical_device_properties: _,
            entry: _,
            has_nv_optimus,
            android_sdk_version,
            instance_api_version,
            drop_guard: _,
        } = self;
        f.debug_struct("InstanceShared")
            .field("extensions", extensions)
            .field("flags", flags)
            .field("memory_budget_thresholds", memory_budget_thresholds)
            .field("has_nv_optimus", has_nv_optimus)
            .field("android_sdk_version", android_sdk_version)
            .field("instance_api_version", instance_api_version)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct Instance {
    shared: Arc<InstanceShared>,
}

#[expect(missing_debug_implementations, reason = "TODO?")]
pub struct Surface {
    swapchain: RwLock<Option<Box<dyn swapchain::Swapchain>>>,
    inner: Box<dyn swapchain::Surface>,
}

impl Surface {

    pub unsafe fn raw_native_handle(&self) -> Option<vk::SurfaceKHR> {
        Some(
            self.inner
                .as_any()
                .downcast_ref::<swapchain::NativeSurface>()?
                .as_raw(),
        )
    }

    pub fn raw_native_swapchain(&self) -> Option<vk::SwapchainKHR> {
        let read = self.swapchain.read();
        Some(
            read.as_ref()?
                .as_any()
                .downcast_ref::<swapchain::NativeSwapchain>()?
                .as_raw(),
        )
    }

    #[track_caller]
    pub fn set_next_present_time(&self, present_timing: vk::PresentTimeGOOGLE) {
        let mut swapchain = self.swapchain.write();
        swapchain
            .as_mut()
            .expect("Surface should have been configured")
            .as_any_mut()
            .downcast_mut::<swapchain::NativeSwapchain>()
            .expect("Surface should have a native Vulkan swapchain")
            .set_next_present_time(present_timing);
    }
}

#[derive(Debug)]
pub struct SurfaceTexture {
    index: u32,
    texture: Texture,
    metadata: Box<dyn swapchain::SurfaceTextureMetadata>,
}

impl crate::DynSurfaceTexture for SurfaceTexture {}

impl Borrow<Texture> for SurfaceTexture {
    fn borrow(&self) -> &Texture {
        &self.texture
    }
}

impl Borrow<dyn crate::DynTexture> for SurfaceTexture {
    fn borrow(&self) -> &dyn crate::DynTexture {
        &self.texture
    }
}

#[derive(Debug)]
pub struct Adapter {
    raw: vk::PhysicalDevice,
    instance: Arc<InstanceShared>,

    known_memory_flags: vk::MemoryPropertyFlags,
    phd_capabilities: adapter::PhysicalDeviceProperties,
    phd_features: PhysicalDeviceFeatures,
    downlevel_flags: wgt::DownlevelFlags,
    private_caps: PrivateCapabilities,
    workarounds: Workarounds,
}

enum ExtensionFn<T> {

    Extension(T),

    Promoted,
}

struct DeviceExtensionFunctions {
    debug_utils: Option<ext::debug_utils::Device>,
    draw_indirect_count: Option<khr::draw_indirect_count::Device>,
    timeline_semaphore: Option<ExtensionFn<khr::timeline_semaphore::Device>>,
    ray_tracing: Option<RayTracingDeviceExtensionFunctions>,
    ray_tracing_pipelines: Option<khr::ray_tracing_pipeline::Device>,
    mesh_shading: Option<ext::mesh_shader::Device>,
    #[cfg_attr(not(unix), allow(dead_code))]
    external_memory_fd: Option<khr::external_memory_fd::Device>,
}

struct RayTracingDeviceExtensionFunctions {
    acceleration_structure: khr::acceleration_structure::Device,
    buffer_device_address: khr::buffer_device_address::Device,
}

#[derive(Clone, Debug)]
struct PrivateCapabilities {
    image_view_usage: bool,
    timeline_semaphores: bool,
    texture_d24: bool,
    texture_d24_s8: bool,
    texture_s8: bool,

    can_present: bool,
    non_coherent_map_mask: wgt::BufferAddress,
    multi_draw_indirect: bool,
    max_draw_indirect_count: u32,

    robust_buffer_access: bool,

    robust_image_access: bool,

    robust_buffer_access2: bool,

    robust_image_access2: bool,
    zero_initialize_workgroup_memory: bool,
    image_format_list: bool,
    maximum_samplers: u32,

    shader_integer_dot_product: bool,

    shader_int8: bool,

    multiview_instance_index_limit: u32,

    scratch_buffer_alignment: u32,

    ray_tracing_pipeline_group_data_size: u32,
}

bitflags::bitflags!(

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct Workarounds: u32 {

        const SEPARATE_ENTRY_POINTS = 0x1;

        const EMPTY_RESOLVE_ATTACHMENT_LISTS = 0x2;

        const FORCE_FILL_BUFFER_WITH_SIZE_GREATER_4096_ALIGNED_OFFSET_16 = 0x4;
    }
);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AttachmentKey {
    format: vk::Format,
    layout: vk::ImageLayout,
    ops: crate::AttachmentOps,
}

impl AttachmentKey {

    fn compatible(format: vk::Format, layout: vk::ImageLayout) -> Self {
        Self {
            format,
            layout,
            ops: crate::AttachmentOps::all(),
        }
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct ColorAttachmentKey {
    base: AttachmentKey,
    resolve: Option<AttachmentKey>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct DepthStencilAttachmentKey {
    base: AttachmentKey,
    stencil_ops: crate::AttachmentOps,
}

#[derive(Clone, Eq, Default, Hash, PartialEq)]
struct RenderPassKey {
    colors: ArrayVec<Option<ColorAttachmentKey>, { crate::MAX_COLOR_ATTACHMENTS }>,
    depth_stencil: Option<DepthStencilAttachmentKey>,
    sample_count: u32,
    multiview_mask: Option<NonZeroU32>,
}

struct DeviceShared {
    raw: ash::Device,
    family_index: u32,
    queue_index: u32,
    raw_queue: vk::Queue,
    instance: Arc<InstanceShared>,
    physical_device: vk::PhysicalDevice,
    enabled_extensions: Vec<&'static CStr>,
    extension_fns: DeviceExtensionFunctions,
    vendor_id: u32,
    pipeline_cache_validation_key: [u8; 16],
    timestamp_period: f32,
    private_caps: PrivateCapabilities,
    workarounds: Workarounds,
    features: wgt::Features,
    render_passes: Mutex<FastHashMap<RenderPassKey, vk::RenderPass>>,
    sampler_cache: Mutex<sampler::SamplerCache>,
    memory_allocations_counter: InternalCounter,

    texture_identity_factory: ResourceIdentityFactory<vk::Image>,

    texture_view_identity_factory: ResourceIdentityFactory<vk::ImageView>,

    empty_descriptor_set_layout: vk::DescriptorSetLayout,

    drop_guard: Option<crate::DropGuard>,
}

impl Drop for DeviceShared {
    fn drop(&mut self) {
        for &raw in self.render_passes.lock().values() {
            unsafe { self.raw.destroy_render_pass(raw, None) };
        }
        unsafe {
            self.raw
                .destroy_descriptor_set_layout(self.empty_descriptor_set_layout, None)
        };
        if self.drop_guard.is_none() {
            unsafe { self.raw.destroy_device(None) };
        }
    }
}

#[expect(
    missing_debug_implementations,
    reason = "needs work to not be disastrously verbose"
)]
pub struct Device {
    mem_allocator: Mutex<gpu_allocator::vulkan::Allocator>,
    desc_allocator: Mutex<descriptor::DescriptorAllocator>,
    valid_ash_memory_types: u32,
    naga_options: naga::back::spv::Options<'static>,
    #[cfg(feature = "renderdoc")]
    render_doc: crate::auxil::renderdoc::RenderDoc,
    counters: Arc<wgt::HalCounters>,

    shared: Arc<DeviceShared>,
}

impl Drop for Device {
    fn drop(&mut self) {}
}

#[derive(Clone)]
struct RelaySemaphores {

    wait: Option<vk::Semaphore>,

    signal: vk::Semaphore,
}

impl RelaySemaphores {
    fn new(device: &DeviceShared) -> Result<Self, crate::DeviceError> {
        Ok(Self {
            wait: None,
            signal: device.new_binary_semaphore("RelaySemaphores: 1")?,
        })
    }

    fn advance(&mut self, device: &DeviceShared) -> Result<Self, crate::DeviceError> {
        let old = self.clone();

        match self.wait {
            None => {

                self.wait = Some(old.signal);
                self.signal = device.new_binary_semaphore("RelaySemaphores: 2")?;
            }
            Some(ref mut wait) => {

                mem::swap(wait, &mut self.signal);
            }
        };

        Ok(old)
    }

    unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            if let Some(wait) = self.wait {
                device.destroy_semaphore(wait, None);
            }
            device.destroy_semaphore(self.signal, None);
        }
    }
}

pub struct Queue {
    raw: vk::Queue,
    device: Arc<DeviceShared>,
    family_index: u32,
    relay_semaphores: Mutex<RelaySemaphores>,
    signal_semaphores: Mutex<SemaphoreList>,
    wait_semaphores: Mutex<SemaphoreList>,
}

impl fmt::Debug for Queue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            raw: _,
            device: _,
            family_index,
            relay_semaphores: _,
            signal_semaphores: _,
            wait_semaphores: _,
        } = self;
        f.debug_struct("Queue")
            .field("family_index", family_index)
            .finish_non_exhaustive()
    }
}

impl Queue {
    pub fn as_raw(&self) -> vk::Queue {
        self.raw
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        unsafe { self.relay_semaphores.lock().destroy(&self.device.raw) };
    }
}
#[derive(Debug)]
enum BufferMemoryBacking {
    Managed(gpu_allocator::vulkan::Allocation),
    VulkanMemory {
        memory: vk::DeviceMemory,
        offset: u64,
        size: u64,
    },
}
impl BufferMemoryBacking {
    fn memory(&self) -> vk::DeviceMemory {
        match self {
            Self::Managed(m) => unsafe { m.memory() },
            Self::VulkanMemory { memory, .. } => *memory,
        }
    }
    fn offset(&self) -> u64 {
        match self {
            Self::Managed(m) => m.offset(),
            Self::VulkanMemory { offset, .. } => *offset,
        }
    }
    fn size(&self) -> u64 {
        match self {
            Self::Managed(m) => m.size(),
            Self::VulkanMemory { size, .. } => *size,
        }
    }
}

#[derive(Debug)]
enum BufferOwnership {

    Managed(Mutex<BufferMemoryBacking>),

    RawHandle,

    External(crate::DropGuard),
}

#[derive(Debug)]
pub struct Buffer {
    raw: vk::Buffer,

    ownership: BufferOwnership,
}
impl Buffer {

    pub unsafe fn from_raw(vk_buffer: vk::Buffer) -> Self {
        Self {
            raw: vk_buffer,
            ownership: BufferOwnership::RawHandle,
        }
    }

    pub unsafe fn from_raw_externally_owned(
        vk_buffer: vk::Buffer,
        drop_callback: crate::DropCallback,
    ) -> Self {
        Self {
            raw: vk_buffer,
            ownership: BufferOwnership::External(crate::DropGuard::new(drop_callback)),
        }
    }

    pub unsafe fn from_raw_managed(
        vk_buffer: vk::Buffer,
        memory: vk::DeviceMemory,
        offset: u64,
        size: u64,
    ) -> Self {
        Self {
            raw: vk_buffer,
            ownership: BufferOwnership::Managed(Mutex::new(BufferMemoryBacking::VulkanMemory {
                memory,
                offset,
                size,
            })),
        }
    }

    pub unsafe fn raw_handle(&self) -> vk::Buffer {
        self.raw
    }
}

impl crate::DynBuffer for Buffer {}

#[derive(Debug)]
pub struct AccelerationStructure {
    raw: vk::AccelerationStructureKHR,
    buffer: vk::Buffer,
    allocation: gpu_allocator::vulkan::Allocation,
    compacted_size_query: Option<vk::QueryPool>,
}

impl crate::DynAccelerationStructure for AccelerationStructure {}

#[derive(Debug)]
pub enum TextureMemory {

    Allocation(gpu_allocator::vulkan::Allocation),

    Dedicated(vk::DeviceMemory),

    External,
}

#[derive(Debug)]
pub struct Texture {
    raw: vk::Image,
    memory: TextureMemory,
    format: wgt::TextureFormat,
    copy_size: crate::CopyExtent,
    identity: ResourceIdentity<vk::Image>,

    drop_guard: Option<crate::DropGuard>,
}

impl crate::DynTexture for Texture {}

impl Texture {

    pub unsafe fn raw_handle(&self) -> vk::Image {
        self.raw
    }

    pub unsafe fn memory(&self) -> &TextureMemory {
        &self.memory
    }
}

#[derive(Debug)]
pub struct TextureView {
    raw_texture: vk::Image,
    raw: vk::ImageView,
    _layers: NonZeroU32,
    format: wgt::TextureFormat,
    raw_format: vk::Format,
    base_mip_level: u32,
    dimension: wgt::TextureViewDimension,
    texture_identity: ResourceIdentity<vk::Image>,
    view_identity: ResourceIdentity<vk::ImageView>,
}

impl crate::DynTextureView for TextureView {}

impl TextureView {

    pub unsafe fn raw_handle(&self) -> vk::ImageView {
        self.raw
    }

    fn identified_raw_view(&self) -> IdentifiedTextureView {
        IdentifiedTextureView {
            raw: self.raw,
            identity: self.view_identity,
        }
    }
}

#[derive(Debug)]
pub struct Sampler {
    raw: vk::Sampler,
    create_info: vk::SamplerCreateInfo<'static>,
}

impl crate::DynSampler for Sampler {}

#[derive(Copy, Clone, Debug)]
struct BindingInfo {
    binding: u32,
    binding_array_size: Option<NonZeroU32>,
}

#[derive(Debug)]
pub struct BindGroupLayout {
    raw: vk::DescriptorSetLayout,
    desc_count: descriptor::DescriptorCounts,

    entries: Box<[wgt::BindGroupLayoutEntry]>,

    binding_map: Vec<(u32, BindingInfo)>,
    contains_binding_arrays: bool,
}

impl crate::DynBindGroupLayout for BindGroupLayout {}

#[derive(Debug)]
pub struct PipelineLayout {
    raw: vk::PipelineLayout,
    binding_map: naga::back::spv::BindingMap,
}

impl crate::DynPipelineLayout for PipelineLayout {}

#[derive(Debug)]
pub struct BindGroup {
    set: descriptor::DescriptorSet,
}

impl crate::DynBindGroup for BindGroup {}

#[derive(Default)]
struct Temp {
    marker: Vec<u8>,
    buffer_barriers: Vec<vk::BufferMemoryBarrier<'static>>,
    image_barriers: Vec<vk::ImageMemoryBarrier<'static>>,
}

impl Temp {
    fn clear(&mut self) {
        self.marker.clear();
        self.buffer_barriers.clear();
        self.image_barriers.clear();
    }

    fn make_c_str(&mut self, name: &str) -> &CStr {
        self.marker.clear();
        self.marker.extend_from_slice(name.as_bytes());
        self.marker.push(0);
        unsafe { CStr::from_bytes_with_nul_unchecked(&self.marker) }
    }
}

struct ResourceIdentityFactory<T> {
    #[cfg(not(target_has_atomic = "64"))]
    next_id: Mutex<u64>,
    #[cfg(target_has_atomic = "64")]
    next_id: core::sync::atomic::AtomicU64,
    _phantom: PhantomData<T>,
}

impl<T> ResourceIdentityFactory<T> {
    fn new() -> Self {
        Self {
            #[cfg(not(target_has_atomic = "64"))]
            next_id: Mutex::new(0),
            #[cfg(target_has_atomic = "64")]
            next_id: core::sync::atomic::AtomicU64::new(0),
            _phantom: PhantomData,
        }
    }

    fn next(&self) -> ResourceIdentity<T> {
        #[cfg(not(target_has_atomic = "64"))]
        {
            let mut next_id = self.next_id.lock();
            let id = *next_id;
            *next_id += 1;
            ResourceIdentity {
                id,
                _phantom: PhantomData,
            }
        }

        #[cfg(target_has_atomic = "64")]
        ResourceIdentity {
            id: self
                .next_id
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed),
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
struct ResourceIdentity<T> {
    id: u64,
    _phantom: PhantomData<T>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct FramebufferKey {
    raw_pass: vk::RenderPass,

    attachment_identities: ArrayVec<ResourceIdentity<vk::ImageView>, { MAX_TOTAL_ATTACHMENTS }>,

    attachment_views: ArrayVec<vk::ImageView, { MAX_TOTAL_ATTACHMENTS }>,
    extent: wgt::Extent3d,
}

impl FramebufferKey {
    fn push_view(&mut self, view: IdentifiedTextureView) {
        self.attachment_identities.push(view.identity);
        self.attachment_views.push(view.raw);
    }
}

#[derive(Copy, Clone)]
struct IdentifiedTextureView {
    raw: vk::ImageView,
    identity: ResourceIdentity<vk::ImageView>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct TempTextureViewKey {
    texture: vk::Image,

    texture_identity: ResourceIdentity<vk::Image>,
    format: vk::Format,
    mip_level: u32,
    depth_slice: u32,
}

pub struct CommandEncoder {
    raw: vk::CommandPool,
    device: Arc<DeviceShared>,

    active: vk::CommandBuffer,

    bind_point: vk::PipelineBindPoint,

    temp: Temp,

    free: Vec<vk::CommandBuffer>,

    discarded: Vec<vk::CommandBuffer>,

    rpass_debug_marker_active: bool,

    end_of_pass_timer_query: Option<(vk::QueryPool, u32)>,

    framebuffers: FastHashMap<FramebufferKey, vk::Framebuffer>,
    temp_texture_views: FastHashMap<TempTextureViewKey, IdentifiedTextureView>,

    counters: Arc<wgt::HalCounters>,

    current_pipeline_is_multiview: bool,
}

impl Drop for CommandEncoder {
    fn drop(&mut self) {

        unsafe {

            self.device.raw.destroy_command_pool(self.raw, None);
        }

        for (_, fb) in self.framebuffers.drain() {
            unsafe { self.device.raw.destroy_framebuffer(fb, None) };
        }

        for (_, view) in self.temp_texture_views.drain() {
            unsafe { self.device.raw.destroy_image_view(view.raw, None) };
        }

        self.counters.command_encoders.sub(1);
    }
}

impl CommandEncoder {

    pub unsafe fn raw_handle(&self) -> vk::CommandBuffer {
        self.active
    }
}

impl fmt::Debug for CommandEncoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandEncoder")
            .field("raw", &self.raw)
            .finish()
    }
}

#[derive(Debug)]
pub struct CommandBuffer {
    raw: vk::CommandBuffer,
}

impl crate::DynCommandBuffer for CommandBuffer {}

#[derive(Debug)]
pub enum ShaderModule {
    Raw(vk::ShaderModule),
    Intermediate {
        naga_shader: crate::NagaShader,
        runtime_checks: wgt::ShaderRuntimeChecks,
    },
}

impl crate::DynShaderModule for ShaderModule {}

#[derive(Debug)]
pub struct RenderPipeline {
    raw: vk::Pipeline,
    is_multiview: bool,
}

impl crate::DynRenderPipeline for RenderPipeline {}

#[derive(Debug)]
pub struct ComputePipeline {
    raw: vk::Pipeline,
}

impl crate::DynComputePipeline for ComputePipeline {}

#[derive(Debug)]
pub struct RayTracingPipeline {
    raw: vk::Pipeline,
}

impl crate::DynRayTracingPipeline for RayTracingPipeline {}

#[derive(Debug)]
pub struct PipelineCache {
    raw: vk::PipelineCache,
}

impl crate::DynPipelineCache for PipelineCache {}

#[derive(Debug)]
pub struct QuerySet {
    raw: vk::QueryPool,
}

impl crate::DynQuerySet for QuerySet {}

#[derive(Debug)]
pub enum Fence {

    TimelineSemaphore(vk::Semaphore),

    FencePool(RwLock<FencePool>),
}

pub(super) type SynchronizedFence = Arc<vk::Fence>;

#[derive(Debug)]
pub struct FencePool {
    last_completed: crate::FenceValue,

    active: Vec<(crate::FenceValue, SynchronizedFence)>,

    free: Vec<vk::Fence>,
}

impl crate::DynFence for Fence {}

impl Fence {

    fn check_active(
        device: &ash::Device,
        mut last_completed: crate::FenceValue,
        active: &[(crate::FenceValue, SynchronizedFence)],
    ) -> Result<crate::FenceValue, crate::DeviceError> {
        for &(value, ref raw) in active.iter() {
            unsafe {
                if value > last_completed
                    && device

                        .get_fence_status(**raw)
                        .map_err(map_host_device_oom_and_lost_err)?
                {
                    last_completed = value;
                }
            }
        }
        Ok(last_completed)
    }

    fn get_latest(
        &self,
        device: &ash::Device,
        extension: Option<&ExtensionFn<khr::timeline_semaphore::Device>>,
    ) -> Result<crate::FenceValue, crate::DeviceError> {
        match *self {
            Self::TimelineSemaphore(raw) => unsafe {
                Ok(match *extension.unwrap() {
                    ExtensionFn::Extension(ref ext) => ext
                        .get_semaphore_counter_value(raw)
                        .map_err(map_host_device_oom_and_lost_err)?,
                    ExtensionFn::Promoted => device
                        .get_semaphore_counter_value(raw)
                        .map_err(map_host_device_oom_and_lost_err)?,
                })
            },
            Self::FencePool(ref pool) => {
                let FencePool {
                    last_completed,
                    ref active,
                    free: _,
                } = *pool.read();
                Self::check_active(device, last_completed, active)
            }
        }
    }

    fn maintain(&self, device: &ash::Device) -> Result<(), crate::DeviceError> {
        match *self {
            Self::TimelineSemaphore(_) => {}
            Self::FencePool(ref pool) => {
                let FencePool {
                    ref mut last_completed,
                    ref mut active,
                    ref mut free,
                } = *pool.write();

                let base_free = free.len();
                let latest = Self::check_active(device, *last_completed, active)?;

                active.retain_mut(|&mut (value, ref mut fence)| {
                    if value > latest {
                        true
                    } else if let Some(fence) = Arc::get_mut(fence) {

                        free.push(*fence);
                        false
                    } else {

                        true
                    }
                });

                if free.len() != base_free {
                    unsafe { device.reset_fences(&free[base_free..]) }
                        .map_err(map_device_oom_err)?
                }
                *last_completed = latest;
            }
        }
        Ok(())
    }
}

impl crate::Queue for Queue {
    type A = Api;

    unsafe fn submit(
        &self,
        command_buffers: &[&CommandBuffer],
        surface_textures: &[&SurfaceTexture],
        (signal_fence, signal_value): (&Fence, crate::FenceValue),
    ) -> Result<(), crate::DeviceError> {
        let mut fence_raw = vk::Fence::null();

        let mut wait_semaphores = SemaphoreList::new(SemaphoreListMode::Wait);
        let mut signal_semaphores = SemaphoreList::new(SemaphoreListMode::Signal);

        debug_assert!(
            {
                let mut check = HashSet::with_capacity(surface_textures.len());

                for st in surface_textures {
                    let ptr: *const () = <*const _>::cast(&*st.metadata);
                    check.insert(ptr as usize);
                }
                check.len() == surface_textures.len()
            },
            "More than one surface texture is being used from the same swapchain. This will cause a deadlock in release."
        );

        let locked_swapchain_semaphores = surface_textures
            .iter()
            .map(|st| st.metadata.get_semaphore_guard())
            .collect::<Vec<_>>();

        for mut semaphores in locked_swapchain_semaphores {
            semaphores.set_used_fence_value(signal_value);

            if let Some(sem) = semaphores.get_acquire_wait_semaphore() {
                wait_semaphores.push_wait(sem, vk::PipelineStageFlags::TOP_OF_PIPE);
            }

            let signal_semaphore = semaphores.get_submit_signal_semaphore(&self.device)?;
            signal_semaphores.push_signal(signal_semaphore);
        }

        let mut guard = self.signal_semaphores.lock();
        if !guard.is_empty() {
            signal_semaphores.append(&mut guard);
        }

        let mut wait_guard = self.wait_semaphores.lock();
        if !wait_guard.is_empty() {
            wait_semaphores.append(&mut wait_guard);
        }

        let semaphore_state = self.relay_semaphores.lock().advance(&self.device)?;

        if let Some(sem) = semaphore_state.wait {
            wait_semaphores.push_wait(
                SemaphoreType::Binary(sem),
                vk::PipelineStageFlags::TOP_OF_PIPE,
            );
        }

        signal_semaphores.push_signal(SemaphoreType::Binary(semaphore_state.signal));

        signal_fence.maintain(&self.device.raw)?;

        let shared_fence;
        match *signal_fence {
            Fence::TimelineSemaphore(raw) => {
                signal_semaphores.push_signal(SemaphoreType::Timeline(raw, signal_value));
            }
            Fence::FencePool(ref pool) => {
                let FencePool {
                    ref mut active,
                    ref mut free,
                    ..
                } = *pool.write();
                shared_fence = match free.pop() {
                    Some(raw) => Arc::new(raw),
                    None => unsafe {
                        let fence = self
                            .device
                            .raw
                            .create_fence(&vk::FenceCreateInfo::default(), None)
                            .map_err(map_host_device_oom_err)?;
                        Arc::new(fence)
                    },
                };
                fence_raw = *shared_fence;
                active.push((signal_value, shared_fence.clone()));
            }
        }

        let vk_cmd_buffers = command_buffers
            .iter()
            .map(|cmd| cmd.raw)
            .collect::<Vec<_>>();

        let mut vk_info = vk::SubmitInfo::default().command_buffers(&vk_cmd_buffers);
        let mut vk_timeline_info = mem::MaybeUninit::uninit();
        vk_info = SemaphoreList::add_to_submit(
            &mut wait_semaphores,
            &mut signal_semaphores,
            vk_info,
            &mut vk_timeline_info,
        );

        profiling::scope!("vkQueueSubmit");
        unsafe {
            self.device
                .raw
                .queue_submit(self.raw, &[vk_info], fence_raw)
                .map_err(map_host_device_oom_and_lost_err)?
        };
        Ok(())
    }

    unsafe fn present(
        &self,
        surface: &Surface,
        texture: SurfaceTexture,
    ) -> Result<(), crate::SurfaceError> {
        let mut swapchain = surface.swapchain.write();

        unsafe { swapchain.as_mut().unwrap().present(self, texture) }
    }

    unsafe fn get_timestamp_period(&self) -> f32 {
        self.device.timestamp_period
    }

    unsafe fn wait_for_idle(&self) -> Result<(), crate::DeviceError> {
        unsafe { self.device.raw.queue_wait_idle(self.raw) }
            .map_err(map_host_device_oom_and_lost_err)
    }
}

impl Queue {
    pub fn raw_device(&self) -> &ash::Device {
        &self.device.raw
    }

    pub fn add_signal_semaphore(&self, semaphore: vk::Semaphore, semaphore_value: Option<u64>) {
        let mut guard = self.signal_semaphores.lock();
        if let Some(value) = semaphore_value {
            guard.push_signal(SemaphoreType::Timeline(semaphore, value));
        } else {
            guard.push_signal(SemaphoreType::Binary(semaphore));
        }
    }

    pub fn remove_signal_semaphore(&self, semaphore: vk::Semaphore) -> bool {
        self.signal_semaphores.lock().remove(semaphore)
    }

    pub fn add_wait_semaphore(
        &self,
        semaphore: vk::Semaphore,
        semaphore_value: Option<u64>,
        stage: vk::PipelineStageFlags,
    ) {
        let mut guard = self.wait_semaphores.lock();
        if let Some(value) = semaphore_value {
            guard.push_wait(SemaphoreType::Timeline(semaphore, value), stage);
        } else {
            guard.push_wait(SemaphoreType::Binary(semaphore), stage);
        }
    }

    pub fn remove_wait_semaphore(&self, semaphore: vk::Semaphore) -> bool {
        self.wait_semaphores.lock().remove(semaphore)
    }
}

fn map_host_device_oom_err(err: vk::Result) -> crate::DeviceError {
    match err {
        vk::Result::ERROR_OUT_OF_HOST_MEMORY | vk::Result::ERROR_OUT_OF_DEVICE_MEMORY => {
            get_oom_err(err)
        }
        e => get_unexpected_err(e),
    }
}

fn map_host_device_oom_and_lost_err(err: vk::Result) -> crate::DeviceError {
    match err {
        vk::Result::ERROR_DEVICE_LOST => get_lost_err(),
        other => map_host_device_oom_err(other),
    }
}

fn map_host_device_oom_and_fragmentation_err(err: vk::Result) -> crate::DeviceError {
    match err {
        vk::Result::ERROR_FRAGMENTATION => get_oom_err(err),
        other => map_host_device_oom_err(other),
    }
}

fn map_host_device_oom_and_ioca_err(err: vk::Result) -> crate::DeviceError {

    map_host_device_oom_err(err)
}

fn map_host_oom_err(err: vk::Result) -> crate::DeviceError {
    match err {
        vk::Result::ERROR_OUT_OF_HOST_MEMORY => get_oom_err(err),
        e => get_unexpected_err(e),
    }
}

fn map_device_oom_err(err: vk::Result) -> crate::DeviceError {
    match err {
        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY => get_oom_err(err),
        e => get_unexpected_err(e),
    }
}

fn map_host_oom_and_ioca_err(err: vk::Result) -> crate::DeviceError {

    map_host_oom_err(err)
}

fn map_pipeline_err(err: vk::Result) -> crate::DeviceError {

    map_host_device_oom_err(err)
}

fn get_unexpected_err(_err: vk::Result) -> crate::DeviceError {
    #[cfg(feature = "internal_error_panic")]
    panic!("Unexpected Vulkan error: {_err:?}");

    #[allow(unreachable_code)]
    crate::DeviceError::Unexpected
}

fn get_oom_err(_err: vk::Result) -> crate::DeviceError {
    crate::DeviceError::OutOfMemory
}

fn get_lost_err() -> crate::DeviceError {
    #[cfg(feature = "device_lost_panic")]
    panic!("Device lost");

    #[allow(unreachable_code)]
    crate::DeviceError::Lost
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct RawTlasInstance {
    transform: [f32; 12],
    custom_data_and_mask: u32,
    shader_binding_table_record_offset_and_flags: u32,
    acceleration_structure_reference: u64,
}

#[derive(Debug)]
pub struct CreateDeviceCallbackArgs<'arg, 'pnext, 'this>
where
    'this: 'pnext,
{

    pub extensions: &'arg mut Vec<&'static CStr>,

    pub device_features: &'arg mut PhysicalDeviceFeatures,

    pub queue_create_infos: &'arg mut Vec<vk::DeviceQueueCreateInfo<'pnext>>,

    pub create_info: &'arg mut vk::DeviceCreateInfo<'pnext>,

    _phantom: PhantomData<&'this ()>,
}

pub type CreateDeviceCallback<'this> =
    dyn for<'arg, 'pnext> FnOnce(CreateDeviceCallbackArgs<'arg, 'pnext, 'this>) + 'this;

#[expect(missing_debug_implementations, reason = "TODO?")]
pub struct CreateInstanceCallbackArgs<'arg, 'pnext, 'this>
where
    'this: 'pnext,
{

    pub extensions: &'arg mut Vec<&'static CStr>,

    pub create_info: &'arg mut vk::InstanceCreateInfo<'pnext>,

    pub entry: &'arg ash::Entry,

    _phantom: PhantomData<&'this ()>,
}

pub type CreateInstanceCallback<'this> =
    dyn for<'arg, 'pnext> FnOnce(CreateInstanceCallbackArgs<'arg, 'pnext, 'this>) + 'this;
