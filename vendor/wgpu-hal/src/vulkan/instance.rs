use alloc::{borrow::ToOwned as _, boxed::Box, ffi::CString, string::String, sync::Arc, vec::Vec};
use core::{
    ffi::{c_void, CStr},
    marker::PhantomData,
    slice,
    str::FromStr,
};
use std::thread;

use arrayvec::ArrayVec;
use ash::{ext, khr, vk};
use parking_lot::RwLock;

unsafe extern "system" fn debug_utils_messenger_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data_ptr: *const vk::DebugUtilsMessengerCallbackDataEXT,
    user_data: *mut c_void,
) -> vk::Bool32 {
    use alloc::borrow::Cow;

    if thread::panicking() {
        return vk::FALSE;
    }

    let cd = unsafe { &*callback_data_ptr };
    let user_data = unsafe { &*user_data.cast::<super::DebugUtilsMessengerUserData>() };

    const VUID_VKCMDENDDEBUGUTILSLABELEXT_COMMANDBUFFER_01912: i32 = 0x56146426;
    if cd.message_id_number == VUID_VKCMDENDDEBUGUTILSLABELEXT_COMMANDBUFFER_01912 {

        if let Some(layer_properties) = user_data.validation_layer_properties.as_ref() {
            if layer_properties.layer_description.as_ref() == c"Khronos Validation Layer"
                && layer_properties.layer_spec_version >= vk::make_api_version(0, 1, 3, 240)
                && layer_properties.layer_spec_version <= vk::make_api_version(0, 1, 3, 250)
            {
                return vk::FALSE;
            }
        }
    }

    const VUID_VKSWAPCHAINCREATEINFOKHR_PNEXT_07781: i32 = 0x4c8929c1;
    if cd.message_id_number == VUID_VKSWAPCHAINCREATEINFOKHR_PNEXT_07781 {
        return vk::FALSE;
    }

    const VUID_VKRENDERPASSBEGININFO_FRAMEBUFFER_04627: i32 = 0x45125641;
    if cd.message_id_number == VUID_VKRENDERPASSBEGININFO_FRAMEBUFFER_04627
        && user_data.has_obs_layer
    {
        return vk::FALSE;
    }

    const VUID_VKCMDCOPYIMAGETOBUFFER_PREGIONS_00184: i32 = 0x45ef177c;
    if cd.message_id_number == VUID_VKCMDCOPYIMAGETOBUFFER_PREGIONS_00184 {
        return vk::FALSE;
    }

    const VUID_STANDALONESPIRV_NONE_10684: i32 = 0xb210f7c2_u32 as i32;
    if cd.message_id_number == VUID_STANDALONESPIRV_NONE_10684 {
        return vk::FALSE;
    }

    let level = match message_severity {

        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => log::Level::Trace,
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => log::Level::Debug,
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => log::Level::Warn,
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => log::Level::Error,
        _ => log::Level::Warn,
    };

    let message_id_name =
        unsafe { cd.message_id_name_as_c_str() }.map_or(Cow::Borrowed(""), CStr::to_string_lossy);
    let message = unsafe { cd.message_as_c_str() }.map_or(Cow::Borrowed(""), CStr::to_string_lossy);

    let _ = std::panic::catch_unwind(|| {
        log::log!(
            level,
            "{:?} [{} (0x{:x})]\n\t{}",
            message_type,
            message_id_name,
            cd.message_id_number,
            message,
        );
    });

    if cd.queue_label_count != 0 {
        let labels =
            unsafe { slice::from_raw_parts(cd.p_queue_labels, cd.queue_label_count as usize) };
        let names = labels
            .iter()
            .flat_map(|dul_obj| unsafe { dul_obj.label_name_as_c_str() }.map(CStr::to_string_lossy))
            .collect::<Vec<_>>();

        let _ = std::panic::catch_unwind(|| {
            log::log!(level, "\tqueues: {}", names.join(", "));
        });
    }

    if cd.cmd_buf_label_count != 0 {
        let labels =
            unsafe { slice::from_raw_parts(cd.p_cmd_buf_labels, cd.cmd_buf_label_count as usize) };
        let names = labels
            .iter()
            .flat_map(|dul_obj| unsafe { dul_obj.label_name_as_c_str() }.map(CStr::to_string_lossy))
            .collect::<Vec<_>>();

        let _ = std::panic::catch_unwind(|| {
            log::log!(level, "\tcommand buffers: {}", names.join(", "));
        });
    }

    if cd.object_count != 0 {
        let labels = unsafe { slice::from_raw_parts(cd.p_objects, cd.object_count as usize) };

        let names = labels
            .iter()
            .map(|obj_info| {
                let name = unsafe { obj_info.object_name_as_c_str() }
                    .map_or(Cow::Borrowed("?"), CStr::to_string_lossy);

                format!(
                    "(type: {:?}, hndl: 0x{:x}, name: {})",
                    obj_info.object_type, obj_info.object_handle, name
                )
            })
            .collect::<Vec<_>>();
        let _ = std::panic::catch_unwind(|| {
            log::log!(level, "\tobjects: {}", names.join(", "));
        });
    }

    #[cfg(feature = "validation_canary")]
    if cfg!(debug_assertions) && level == log::Level::Error {
        use alloc::string::ToString as _;

        crate::VALIDATION_CANARY.add(message.to_string());
    }

    vk::FALSE
}

impl super::DebugUtilsCreateInfo {
    fn to_vk_create_info(&self) -> vk::DebugUtilsMessengerCreateInfoEXT<'_> {
        let user_data_ptr: *const super::DebugUtilsMessengerUserData = &*self.callback_data;
        vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(self.severity)
            .message_type(self.message_type)
            .user_data(user_data_ptr as *mut _)
            .pfn_user_callback(Some(debug_utils_messenger_callback))
    }
}

impl super::InstanceShared {
    pub fn entry(&self) -> &ash::Entry {
        &self.entry
    }

    pub fn raw_instance(&self) -> &ash::Instance {
        &self.raw
    }

    pub fn instance_api_version(&self) -> u32 {
        self.instance_api_version
    }

    pub fn extensions(&self) -> &[&'static CStr] {
        &self.extensions[..]
    }
}

impl super::Instance {
    pub fn shared_instance(&self) -> &super::InstanceShared {
        &self.shared
    }

    fn enumerate_instance_extension_properties(
        entry: &ash::Entry,
        layer_name: Option<&CStr>,
    ) -> Result<Vec<vk::ExtensionProperties>, crate::InstanceError> {
        let instance_extensions = {
            profiling::scope!("vkEnumerateInstanceExtensionProperties");
            unsafe { entry.enumerate_instance_extension_properties(layer_name) }
        };
        instance_extensions.map_err(|e| {
            crate::InstanceError::with_source(
                String::from("enumerate_instance_extension_properties() failed"),
                e,
            )
        })
    }

    pub fn desired_extensions(
        entry: &ash::Entry,
        _instance_api_version: u32,
        flags: wgt::InstanceFlags,
    ) -> Result<Vec<&'static CStr>, crate::InstanceError> {
        let instance_extensions = Self::enumerate_instance_extension_properties(entry, None)?;

        let mut extensions: Vec<&'static CStr> = Vec::new();

        extensions.push(khr::surface::NAME);

        if cfg!(all(
            unix,
            not(target_os = "android"),
            not(target_os = "macos")
        )) {

            extensions.push(khr::xlib_surface::NAME);

            extensions.push(khr::xcb_surface::NAME);

            extensions.push(khr::wayland_surface::NAME);
        }
        if cfg!(target_os = "android") {

            extensions.push(khr::android_surface::NAME);
        }
        if cfg!(target_os = "windows") {

            extensions.push(khr::win32_surface::NAME);
        }
        if cfg!(target_os = "macos") {

            extensions.push(ext::metal_surface::NAME);
            extensions.push(khr::portability_enumeration::NAME);
        }
        if cfg!(drm) {

            extensions.push(ext::acquire_drm_display::NAME);
            extensions.push(ext::direct_mode_display::NAME);
            extensions.push(khr::display::NAME);
            extensions.push(khr::get_physical_device_properties2::NAME);
            extensions.push(khr::get_display_properties2::NAME);
        }

        if flags.contains(wgt::InstanceFlags::DEBUG) {

            extensions.push(ext::debug_utils::NAME);
        }

        extensions.push(ext::swapchain_colorspace::NAME);

        extensions.push(khr::get_physical_device_properties2::NAME);

        extensions.retain(|&ext| {
            if instance_extensions
                .iter()
                .any(|inst_ext| inst_ext.extension_name_as_c_str() == Ok(ext))
            {
                true
            } else {
                log::debug!("Unable to find extension: {}", ext.to_string_lossy());
                false
            }
        });
        Ok(extensions)
    }

    #[allow(clippy::too_many_arguments)]
    pub unsafe fn from_raw(
        entry: ash::Entry,
        raw_instance: ash::Instance,
        instance_api_version: u32,
        android_sdk_version: u32,
        debug_utils_create_info: Option<super::DebugUtilsCreateInfo>,
        extensions: Vec<&'static CStr>,
        flags: wgt::InstanceFlags,
        memory_budget_thresholds: wgt::MemoryBudgetThresholds,
        has_nv_optimus: bool,
        drop_callback: Option<crate::DropCallback>,
    ) -> Result<Self, crate::InstanceError> {
        log::debug!("Instance version: 0x{instance_api_version:x}");

        let debug_utils = if let Some(debug_utils_create_info) = debug_utils_create_info {
            if extensions.contains(&ext::debug_utils::NAME) {
                log::debug!("Enabling debug utils");

                let extension = ext::debug_utils::Instance::new(&entry, &raw_instance);
                let vk_info = debug_utils_create_info.to_vk_create_info();
                let messenger =
                    unsafe { extension.create_debug_utils_messenger(&vk_info, None) }.unwrap();

                Some(super::DebugUtils {
                    extension,
                    messenger,
                    callback_data: debug_utils_create_info.callback_data,
                })
            } else {
                log::debug!("Debug utils not enabled: extension not listed");
                None
            }
        } else {
            log::debug!(
                "Debug utils not enabled: \
                        debug_utils_user_data not passed to Instance::from_raw"
            );
            None
        };

        let get_physical_device_properties =
            if extensions.contains(&khr::get_physical_device_properties2::NAME) {
                log::debug!("Enabling device properties2");
                Some(khr::get_physical_device_properties2::Instance::new(
                    &entry,
                    &raw_instance,
                ))
            } else {
                None
            };

        let drop_guard = crate::DropGuard::from_option(drop_callback);

        Ok(Self {
            shared: Arc::new(super::InstanceShared {
                raw: raw_instance,
                extensions,
                drop_guard,
                flags,
                memory_budget_thresholds,
                debug_utils,
                get_physical_device_properties,
                entry,
                has_nv_optimus,
                instance_api_version,
                android_sdk_version,
            }),
        })
    }

    fn create_surface_from_xlib(
        &self,
        dpy: *mut vk::Display,
        window: vk::Window,
    ) -> Result<super::Surface, crate::InstanceError> {
        if !self.shared.extensions.contains(&khr::xlib_surface::NAME) {
            return Err(crate::InstanceError::new(String::from(
                "Vulkan driver does not support VK_KHR_xlib_surface",
            )));
        }

        let surface = {
            let xlib_loader =
                khr::xlib_surface::Instance::new(&self.shared.entry, &self.shared.raw);
            let info = vk::XlibSurfaceCreateInfoKHR::default()
                .flags(vk::XlibSurfaceCreateFlagsKHR::empty())
                .window(window)
                .dpy(dpy);

            unsafe { xlib_loader.create_xlib_surface(&info, None) }
                .expect("XlibSurface::create_xlib_surface() failed")
        };

        Ok(self.create_surface_from_vk_surface_khr(surface, None))
    }

    fn create_surface_from_xcb(
        &self,
        connection: *mut vk::xcb_connection_t,
        window: vk::xcb_window_t,
    ) -> Result<super::Surface, crate::InstanceError> {
        if !self.shared.extensions.contains(&khr::xcb_surface::NAME) {
            return Err(crate::InstanceError::new(String::from(
                "Vulkan driver does not support VK_KHR_xcb_surface",
            )));
        }

        let surface = {
            let xcb_loader = khr::xcb_surface::Instance::new(&self.shared.entry, &self.shared.raw);
            let info = vk::XcbSurfaceCreateInfoKHR::default()
                .flags(vk::XcbSurfaceCreateFlagsKHR::empty())
                .window(window)
                .connection(connection);

            unsafe { xcb_loader.create_xcb_surface(&info, None) }
                .expect("XcbSurface::create_xcb_surface() failed")
        };

        Ok(self.create_surface_from_vk_surface_khr(surface, None))
    }

    fn create_surface_from_wayland(
        &self,
        display: *mut vk::wl_display,
        surface: *mut vk::wl_surface,
    ) -> Result<super::Surface, crate::InstanceError> {
        if !self.shared.extensions.contains(&khr::wayland_surface::NAME) {
            return Err(crate::InstanceError::new(String::from(
                "Vulkan driver does not support VK_KHR_wayland_surface",
            )));
        }

        let surface = {
            let w_loader =
                khr::wayland_surface::Instance::new(&self.shared.entry, &self.shared.raw);
            let info = vk::WaylandSurfaceCreateInfoKHR::default()
                .flags(vk::WaylandSurfaceCreateFlagsKHR::empty())
                .display(display)
                .surface(surface);

            unsafe { w_loader.create_wayland_surface(&info, None) }.expect("WaylandSurface failed")
        };

        Ok(self.create_surface_from_vk_surface_khr(surface, None))
    }

    fn create_surface_android(
        &self,
        window: *mut vk::ANativeWindow,
    ) -> Result<super::Surface, crate::InstanceError> {
        if !self.shared.extensions.contains(&khr::android_surface::NAME) {
            return Err(crate::InstanceError::new(String::from(
                "Vulkan driver does not support VK_KHR_android_surface",
            )));
        }

        let surface = {
            let a_loader =
                khr::android_surface::Instance::new(&self.shared.entry, &self.shared.raw);
            let info = vk::AndroidSurfaceCreateInfoKHR::default()
                .flags(vk::AndroidSurfaceCreateFlagsKHR::empty())
                .window(window);

            unsafe { a_loader.create_android_surface(&info, None) }.expect("AndroidSurface failed")
        };

        Ok(self.create_surface_from_vk_surface_khr(surface, None))
    }

    fn create_surface_from_hwnd(
        &self,
        hinstance: vk::HINSTANCE,
        hwnd: vk::HWND,
    ) -> Result<super::Surface, crate::InstanceError> {
        if !self.shared.extensions.contains(&khr::win32_surface::NAME) {
            return Err(crate::InstanceError::new(String::from(
                "Vulkan driver does not support VK_KHR_win32_surface",
            )));
        }

        let surface = {
            let info = vk::Win32SurfaceCreateInfoKHR::default()
                .flags(vk::Win32SurfaceCreateFlagsKHR::empty())
                .hinstance(hinstance)
                .hwnd(hwnd);
            let win32_loader =
                khr::win32_surface::Instance::new(&self.shared.entry, &self.shared.raw);
            unsafe {
                win32_loader
                    .create_win32_surface(&info, None)
                    .expect("Unable to create Win32 surface")
            }
        };

        #[cfg(windows)]
        let window_handle = Some(crate::vulkan::swapchain::WindowHandle(
            windows::Win32::Foundation::HWND(hwnd as *mut c_void),
        ));
        #[cfg(not(windows))]
        let window_handle: Option<crate::vulkan::swapchain::WindowHandle> = None;
        Ok(self.create_surface_from_vk_surface_khr(surface, window_handle))
    }

    #[cfg(target_vendor = "apple")]
    fn create_surface_from_layer(
        &self,
        layer: raw_window_metal::Layer,
    ) -> Result<super::Surface, crate::InstanceError> {
        if !self.shared.extensions.contains(&ext::metal_surface::NAME) {
            return Err(crate::InstanceError::new(String::from(
                "Vulkan driver does not support VK_EXT_metal_surface",
            )));
        }

        let surface = {
            let metal_loader =
                ext::metal_surface::Instance::new(&self.shared.entry, &self.shared.raw);
            let vk_info = vk::MetalSurfaceCreateInfoEXT::default()
                .flags(vk::MetalSurfaceCreateFlagsEXT::empty())
                .layer(layer.as_ptr().as_ptr());

            unsafe { metal_loader.create_metal_surface(&vk_info, None).unwrap() }
        };

        Ok(self.create_surface_from_vk_surface_khr(surface, None))
    }

    pub(super) fn create_surface_from_vk_surface_khr(
        &self,
        surface: vk::SurfaceKHR,
        hwnd: Option<crate::vulkan::swapchain::WindowHandle>,
    ) -> super::Surface {
        let native_surface =
            crate::vulkan::swapchain::NativeSurface::from_vk_surface_khr(self, surface, hwnd);

        super::Surface {
            swapchain: RwLock::new(None),
            inner: Box::new(native_surface),
        }
    }

    pub unsafe fn init_with_callback(
        desc: &crate::InstanceDescriptor<'_>,
        callback: Option<Box<super::CreateInstanceCallback>>,
    ) -> Result<Self, crate::InstanceError> {
        profiling::scope!("Init Vulkan Backend");

        let entry = unsafe {
            profiling::scope!("Load vk library");

            #[cfg(target_env = "ohos")]
            let loaded = ash::Entry::load_from("libvulkan.so");
            #[cfg(not(target_env = "ohos"))]
            let loaded = ash::Entry::load();
            loaded
        }
        .map_err(|err| {
            crate::InstanceError::with_source(String::from("missing Vulkan entry points"), err)
        })?;
        let version = {
            profiling::scope!("vkEnumerateInstanceVersion");
            unsafe { entry.try_enumerate_instance_version() }
        };
        let instance_api_version = match version {

            Ok(Some(version)) => version,
            Ok(None) => vk::API_VERSION_1_0,
            Err(err) => {
                return Err(crate::InstanceError::with_source(
                    String::from("try_enumerate_instance_version() failed"),
                    err,
                ));
            }
        };

        let app_name = CString::new(desc.name).unwrap();
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name.as_c_str())
            .application_version(1)
            .engine_name(c"wgpu-hal")
            .engine_version(2)
            .api_version(

                if instance_api_version < vk::API_VERSION_1_1 {
                    vk::API_VERSION_1_0
                } else {

                    vk::API_VERSION_1_3
                },
            );

        let mut extensions = Self::desired_extensions(&entry, instance_api_version, desc.flags)?;
        let mut create_info = vk::InstanceCreateInfo::default();

        if let Some(callback) = callback {
            callback(super::CreateInstanceCallbackArgs {
                extensions: &mut extensions,
                create_info: &mut create_info,
                entry: &entry,
                _phantom: PhantomData,
            });
        }

        let instance_layers = {
            profiling::scope!("vkEnumerateInstanceLayerProperties");
            unsafe { entry.enumerate_instance_layer_properties() }
        };
        let instance_layers = instance_layers.map_err(|e| {
            log::debug!("enumerate_instance_layer_properties: {e:?}");
            crate::InstanceError::with_source(
                String::from("enumerate_instance_layer_properties() failed"),
                e,
            )
        })?;

        fn find_layer<'layers>(
            instance_layers: &'layers [vk::LayerProperties],
            name: &CStr,
        ) -> Option<&'layers vk::LayerProperties> {
            instance_layers
                .iter()
                .find(|inst_layer| inst_layer.layer_name_as_c_str() == Ok(name))
        }

        let validation_layer_name = c"VK_LAYER_KHRONOS_validation";
        let validation_layer_properties = find_layer(&instance_layers, validation_layer_name);

        let validation_features_are_enabled = if validation_layer_properties.is_some() {

            let exts =
                Self::enumerate_instance_extension_properties(&entry, Some(validation_layer_name))?;

            let mut ext_names = exts
                .iter()
                .filter_map(|ext| ext.extension_name_as_c_str().ok());

            ext_names.any(|ext_name| ext_name == ext::validation_features::NAME)
        } else {
            false
        };

        let should_enable_gpu_based_validation = desc
            .flags
            .intersects(wgt::InstanceFlags::GPU_BASED_VALIDATION)
            && validation_features_are_enabled;

        let has_nv_optimus = find_layer(&instance_layers, c"VK_LAYER_NV_optimus").is_some();

        let has_obs_layer = find_layer(&instance_layers, c"VK_LAYER_OBS_HOOK").is_some();

        let mut layers: Vec<&'static CStr> = Vec::new();

        let has_debug_extension = extensions.contains(&ext::debug_utils::NAME);
        let mut debug_user_data = has_debug_extension.then(|| {

            Box::new(super::DebugUtilsMessengerUserData {
                validation_layer_properties: None,
                has_obs_layer,
            })
        });

        if desc.flags.intersects(wgt::InstanceFlags::VALIDATION)
            || should_enable_gpu_based_validation
        {
            if let Some(layer_properties) = validation_layer_properties {
                layers.push(validation_layer_name);

                if let Some(debug_user_data) = debug_user_data.as_mut() {
                    debug_user_data.validation_layer_properties =
                        Some(super::ValidationLayerProperties {
                            layer_description: layer_properties
                                .description_as_c_str()
                                .unwrap()
                                .to_owned(),
                            layer_spec_version: layer_properties.spec_version,
                        });
                }
            } else {
                log::debug!(
                    "InstanceFlags::VALIDATION requested, but unable to find layer: {}",
                    validation_layer_name.to_string_lossy()
                );
            }
        }
        let mut debug_utils = if let Some(callback_data) = debug_user_data {

            let mut severity = vk::DebugUtilsMessageSeverityFlagsEXT::ERROR;
            if log::max_level() >= log::LevelFilter::Debug {
                severity |= vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE;
            }
            if log::max_level() >= log::LevelFilter::Info {
                severity |= vk::DebugUtilsMessageSeverityFlagsEXT::INFO;
            }
            if log::max_level() >= log::LevelFilter::Warn {
                severity |= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING;
            }

            let message_type = vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE;

            let create_info = super::DebugUtilsCreateInfo {
                severity,
                message_type,
                callback_data,
            };

            Some(create_info)
        } else {
            None
        };

        #[cfg(target_os = "android")]
        let android_sdk_version = {
            let properties = android_system_properties::AndroidSystemProperties::new();

            if let Some(val) = properties.get("ro.build.version.sdk") {
                match val.parse::<u32>() {
                    Ok(sdk_ver) => sdk_ver,
                    Err(err) => {
                        log::error!(
                            concat!(
                                "Couldn't parse Android's ",
                                "ro.build.version.sdk system property ({}): {}",
                            ),
                            val,
                            err,
                        );
                        0
                    }
                }
            } else {
                log::error!("Couldn't read Android's ro.build.version.sdk system property");
                0
            }
        };
        #[cfg(not(target_os = "android"))]
        let android_sdk_version = 0;

        let mut flags = vk::InstanceCreateFlags::empty();

        if extensions.contains(&khr::portability_enumeration::NAME) {
            flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
        }
        let vk_instance = {
            let str_pointers = layers
                .iter()
                .chain(extensions.iter())
                .map(|&s: &&'static _| {

                    s.as_ptr()
                })
                .collect::<Vec<_>>();

            create_info = create_info
                .flags(flags)
                .application_info(&app_info)
                .enabled_layer_names(&str_pointers[..layers.len()])
                .enabled_extension_names(&str_pointers[layers.len()..]);

            let mut debug_utils_create_info = debug_utils
                .as_mut()
                .map(|create_info| create_info.to_vk_create_info());
            if let Some(debug_utils_create_info) = debug_utils_create_info.as_mut() {
                create_info = create_info.push_next(debug_utils_create_info);
            }

            let mut validation_features;
            let mut validation_feature_list: ArrayVec<_, 3>;
            if validation_features_are_enabled {
                validation_feature_list = ArrayVec::new();

                validation_feature_list
                    .push(vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION);

                if should_enable_gpu_based_validation {
                    validation_feature_list.push(vk::ValidationFeatureEnableEXT::GPU_ASSISTED);
                    validation_feature_list
                        .push(vk::ValidationFeatureEnableEXT::GPU_ASSISTED_RESERVE_BINDING_SLOT);
                }

                validation_features = vk::ValidationFeaturesEXT::default()
                    .enabled_validation_features(&validation_feature_list);
                create_info = create_info.push_next(&mut validation_features);
            }

            unsafe {
                profiling::scope!("vkCreateInstance");
                entry.create_instance(&create_info, None)
            }
            .map_err(|e| {
                crate::InstanceError::with_source(
                    String::from("Entry::create_instance() failed"),
                    e,
                )
            })?
        };

        unsafe {
            Self::from_raw(
                entry,
                vk_instance,
                instance_api_version,
                android_sdk_version,
                debug_utils,
                extensions,
                desc.flags,
                desc.memory_budget_thresholds,
                has_nv_optimus,
                None,
            )
        }
    }
}

impl Drop for super::InstanceShared {
    fn drop(&mut self) {
        unsafe {

            let _du = self.debug_utils.take().inspect(|du| {
                du.extension
                    .destroy_debug_utils_messenger(du.messenger, None);
            });
            if self.drop_guard.is_none() {
                self.raw.destroy_instance(None);
            }
        }
    }
}

impl crate::Instance for super::Instance {
    type A = super::Api;

    unsafe fn init(desc: &crate::InstanceDescriptor<'_>) -> Result<Self, crate::InstanceError> {
        unsafe { Self::init_with_callback(desc, None) }
    }

    unsafe fn create_surface(
        &self,
        display_handle: raw_window_handle::RawDisplayHandle,
        window_handle: raw_window_handle::RawWindowHandle,
    ) -> Result<super::Surface, crate::InstanceError> {
        use raw_window_handle::{RawDisplayHandle as Rdh, RawWindowHandle as Rwh};

        match (window_handle, display_handle) {
            (Rwh::Wayland(handle), Rdh::Wayland(display)) => {
                self.create_surface_from_wayland(display.display.as_ptr(), handle.surface.as_ptr())
            }
            (Rwh::Xlib(handle), Rdh::Xlib(display)) => {
                let display = display.display.expect("Display pointer is not set.");
                self.create_surface_from_xlib(display.as_ptr(), handle.window)
            }
            (Rwh::Xcb(handle), Rdh::Xcb(display)) => {
                let connection = display.connection.expect("Pointer to X-Server is not set.");
                self.create_surface_from_xcb(connection.as_ptr(), handle.window.get())
            }
            #[cfg(drm)]
            (Rwh::Drm(handle), Rdh::Drm(display)) => {
                self.create_surface_from_drm_plane(display.fd, handle.plane)
            }
            (Rwh::AndroidNdk(handle), _) => {
                self.create_surface_android(handle.a_native_window.as_ptr())
            }
            (Rwh::Win32(handle), _) => {
                let hinstance = handle.hinstance.ok_or_else(|| {
                    crate::InstanceError::new(String::from(
                        "Vulkan requires raw-window-handle's Win32::hinstance to be set",
                    ))
                })?;
                self.create_surface_from_hwnd(hinstance.get(), handle.hwnd.get())
            }
            #[cfg(target_vendor = "apple")]
            (Rwh::AppKit(handle), _)
                if self.shared.extensions.contains(&ext::metal_surface::NAME) =>
            {
                let layer = unsafe { raw_window_metal::Layer::from_ns_view(handle.ns_view) };
                self.create_surface_from_layer(layer)
            }
            #[cfg(target_vendor = "apple")]
            (Rwh::UiKit(handle), _)
                if self.shared.extensions.contains(&ext::metal_surface::NAME) =>
            {
                let layer = unsafe { raw_window_metal::Layer::from_ui_view(handle.ui_view) };
                self.create_surface_from_layer(layer)
            }
            (_, _) => Err(crate::InstanceError::new(format!(
                "window handle {window_handle:?} is not a Vulkan-compatible handle"
            ))),
        }
    }

    unsafe fn enumerate_adapters(
        &self,
        _surface_hint: Option<&super::Surface>,
    ) -> Vec<crate::ExposedAdapter<super::Api>> {
        use crate::auxil::db;

        let raw_devices = match unsafe { self.shared.raw.enumerate_physical_devices() } {
            Ok(devices) => devices,
            Err(err) => {
                log::error!("enumerate_adapters: {err}");
                Vec::new()
            }
        };

        let mut exposed_adapters = raw_devices
            .into_iter()
            .flat_map(|device| self.expose_adapter(device))
            .collect::<Vec<_>>();

        let has_nvidia_dgpu = exposed_adapters.iter().any(|exposed| {
            exposed.info.device_type == wgt::DeviceType::DiscreteGpu
                && exposed.info.vendor == db::nvidia::VENDOR
        });
        if cfg!(target_os = "linux") && has_nvidia_dgpu && self.shared.has_nv_optimus {
            for exposed in exposed_adapters.iter_mut() {
                if exposed.info.device_type == wgt::DeviceType::IntegratedGpu
                    && exposed.info.vendor == db::intel::VENDOR
                {

                    if let Some(version) = exposed.info.driver_info.split_once("Mesa ").map(|s| {
                        let mut components = s.1.split('.');
                        let major = components.next().and_then(|s| u8::from_str(s).ok());
                        let minor = components.next().and_then(|s| u8::from_str(s).ok());
                        if let (Some(major), Some(minor)) = (major, minor) {
                            (major, minor)
                        } else {
                            (0, 0)
                        }
                    }) {
                        if version < (21, 2) {

                            log::debug!(
                                concat!(
                                    "Disabling presentation on '{}' (id {:?}) ",
                                    "due to NV Optimus and Intel Mesa < v21.2"
                                ),
                                exposed.info.name,
                                exposed.adapter.raw
                            );
                            exposed.adapter.private_caps.can_present = false;
                        }
                    }
                }
            }
        }

        exposed_adapters
    }
}

impl crate::Surface for super::Surface {
    type A = super::Api;

    unsafe fn configure(
        &self,
        device: &super::Device,
        config: &crate::SurfaceConfiguration,
    ) -> Result<(), crate::SurfaceError> {

        let mut swap_chain = self.swapchain.write();

        let mut old = swap_chain.take();
        if let Some(ref mut old) = old {
            unsafe { old.release_resources(device) };
        }

        let swapchain = unsafe { self.inner.create_swapchain(device, config, old)? };
        *swap_chain = Some(swapchain);

        Ok(())
    }

    unsafe fn unconfigure(&self, device: &super::Device) {
        if let Some(mut sc) = self.swapchain.write().take() {

            unsafe { sc.release_resources(device) };
        }
    }

    unsafe fn acquire_texture(
        &self,
        timeout: Option<core::time::Duration>,
        fence: &super::Fence,
    ) -> Result<crate::AcquiredSurfaceTexture<super::Api>, crate::SurfaceError> {
        let mut swapchain = self.swapchain.write();
        let swapchain = swapchain.as_mut().unwrap();

        unsafe { swapchain.acquire(timeout, fence) }
    }

    unsafe fn discard_texture(&self, texture: super::SurfaceTexture) {
        unsafe {
            self.swapchain
                .write()
                .as_mut()
                .unwrap()
                .discard_texture(texture)
                .unwrap()
        };
    }
}
