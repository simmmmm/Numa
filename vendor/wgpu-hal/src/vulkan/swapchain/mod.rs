use alloc::boxed::Box;
use core::{any::Any, fmt::Debug, time::Duration};

use crate::vulkan::{semaphore_list::SemaphoreType, DeviceShared};

pub(super) use native::*;

mod native;

#[cfg(windows)]
#[derive(Clone, Copy)]
pub(crate) struct WindowHandle(pub(crate) windows::Win32::Foundation::HWND);

#[cfg(not(windows))]
#[derive(Clone, Copy)]
pub(crate) enum WindowHandle {}

pub(super) trait Surface: Send + Sync + 'static {

    fn surface_capabilities(&self, adapter: &super::Adapter) -> Option<crate::SurfaceCapabilities>;

    unsafe fn create_swapchain(
        &self,
        device: &super::Device,
        config: &crate::SurfaceConfiguration,
        provided_old_swapchain: Option<Box<dyn Swapchain>>,
    ) -> Result<Box<dyn Swapchain>, crate::SurfaceError>;

    fn display_hdr_info(&self) -> Option<wgt::DisplayHdrInfo> {
        None
    }

    fn as_any(&self) -> &dyn Any;
}

pub(super) trait Swapchain: Send + Sync + 'static {

    unsafe fn release_resources(&mut self, device: &super::Device);

    unsafe fn acquire(
        &mut self,
        timeout: Option<Duration>,
        fence: &super::Fence,
    ) -> Result<crate::AcquiredSurfaceTexture<crate::api::Vulkan>, crate::SurfaceError>;

    unsafe fn discard_texture(
        &mut self,
        texture: super::SurfaceTexture,
    ) -> Result<(), crate::SurfaceError>;

    unsafe fn present(
        &mut self,
        queue: &super::Queue,
        texture: crate::vulkan::SurfaceTexture,
    ) -> Result<(), crate::SurfaceError>;

    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub(super) trait SurfaceTextureMetadata: Debug + Send + Sync + 'static {

    fn get_semaphore_guard(&self) -> Box<dyn SwapchainSubmissionSemaphoreGuard + '_>;

    fn as_any(&self) -> &dyn Any;
}

pub(super) trait SwapchainSubmissionSemaphoreGuard {

    fn set_used_fence_value(&mut self, value: u64);

    fn get_acquire_wait_semaphore(&mut self) -> Option<SemaphoreType>;

    fn get_submit_signal_semaphore(
        &mut self,
        device: &DeviceShared,
    ) -> Result<SemaphoreType, crate::DeviceError>;
}
