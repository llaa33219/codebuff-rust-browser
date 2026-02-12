//! Vulkan FFI loader.
//!
//! Dynamically loads `libvulkan.so.1` and resolves Vulkan function pointers.
//! Provides type-safe wrappers for the Vulkan initialization sequence.

use crate::syscall;
use core::ffi::c_void;
use std::fmt;

// ─────────────────────────────────────────────────────────────────────────────
// Vulkan type aliases
// ─────────────────────────────────────────────────────────────────────────────

pub type VkResult = i32;
pub type VkBool32 = u32;
pub type VkFlags = u32;
pub type VkDeviceSize = u64;

// Non-dispatchable handles (u64 on all platforms for us)
pub type VkInstance = u64;
pub type VkPhysicalDevice = u64;
pub type VkDevice = u64;
pub type VkQueue = u64;
pub type VkSurfaceKHR = u64;
pub type VkSwapchainKHR = u64;
pub type VkImage = u64;
pub type VkImageView = u64;
pub type VkRenderPass = u64;
pub type VkFramebuffer = u64;
pub type VkPipeline = u64;
pub type VkPipelineLayout = u64;
pub type VkShaderModule = u64;
pub type VkCommandPool = u64;
pub type VkCommandBuffer = u64;
pub type VkSemaphore = u64;
pub type VkFence = u64;
pub type VkBuffer = u64;
pub type VkDeviceMemory = u64;
pub type VkDescriptorSetLayout = u64;
pub type VkDescriptorPool = u64;
pub type VkDescriptorSet = u64;
pub type VkSampler = u64;

// VkResult constants
pub const VK_SUCCESS: VkResult = 0;
pub const VK_NOT_READY: VkResult = 1;
pub const VK_TIMEOUT: VkResult = 2;
pub const VK_ERROR_OUT_OF_HOST_MEMORY: VkResult = -1;
pub const VK_ERROR_OUT_OF_DEVICE_MEMORY: VkResult = -2;
pub const VK_ERROR_INITIALIZATION_FAILED: VkResult = -3;
pub const VK_ERROR_DEVICE_LOST: VkResult = -4;
pub const VK_ERROR_LAYER_NOT_PRESENT: VkResult = -6;
pub const VK_ERROR_EXTENSION_NOT_PRESENT: VkResult = -7;
pub const VK_ERROR_SURFACE_LOST_KHR: VkResult = -1000000000;
pub const VK_SUBOPTIMAL_KHR: VkResult = 1000001003;
pub const VK_ERROR_OUT_OF_DATE_KHR: VkResult = -1000001004;

// VkStructureType constants (selected)
pub const VK_STRUCTURE_TYPE_APPLICATION_INFO: u32 = 0;
pub const VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO: u32 = 1;
pub const VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO: u32 = 2;
pub const VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO: u32 = 3;
pub const VK_STRUCTURE_TYPE_SUBMIT_INFO: u32 = 4;
pub const VK_STRUCTURE_TYPE_PRESENT_INFO_KHR: u32 = 1000001001;
pub const VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR: u32 = 1000001000;
pub const VK_STRUCTURE_TYPE_XCB_SURFACE_CREATE_INFO_KHR: u32 = 1000005000;
pub const VK_STRUCTURE_TYPE_XLIB_SURFACE_CREATE_INFO_KHR: u32 = 1000004000;

// ─────────────────────────────────────────────────────────────────────────────
// VkError
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum VkError {
    /// Failed to load libvulkan.so
    LoadFailed(String),
    /// A required symbol was not found
    SymbolNotFound(&'static str),
    /// A Vulkan API call returned an error
    ApiError { function: &'static str, result: VkResult },
    /// No suitable physical device found
    NoSuitableDevice,
    /// No suitable queue family found
    NoSuitableQueueFamily,
}

impl fmt::Display for VkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadFailed(msg) => write!(f, "Vulkan load failed: {msg}"),
            Self::SymbolNotFound(sym) => write!(f, "Vulkan symbol not found: {sym}"),
            Self::ApiError { function, result } => {
                write!(f, "Vulkan error in {function}: VkResult={result}")
            }
            Self::NoSuitableDevice => write!(f, "no suitable Vulkan physical device"),
            Self::NoSuitableQueueFamily => write!(f, "no suitable Vulkan queue family"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Function pointer types
// ─────────────────────────────────────────────────────────────────────────────

/// `PFN_vkGetInstanceProcAddr`
pub type PfnGetInstanceProcAddr =
    unsafe extern "C" fn(instance: VkInstance, name: *const u8) -> *mut c_void;

/// `PFN_vkCreateInstance`
pub type PfnCreateInstance =
    unsafe extern "C" fn(
        create_info: *const c_void,
        allocator: *const c_void,
        instance: *mut VkInstance,
    ) -> VkResult;

/// `PFN_vkDestroyInstance`
pub type PfnDestroyInstance =
    unsafe extern "C" fn(instance: VkInstance, allocator: *const c_void);

/// `PFN_vkEnumeratePhysicalDevices`
pub type PfnEnumeratePhysicalDevices =
    unsafe extern "C" fn(
        instance: VkInstance,
        count: *mut u32,
        devices: *mut VkPhysicalDevice,
    ) -> VkResult;

/// `PFN_vkGetPhysicalDeviceQueueFamilyProperties`
pub type PfnGetPhysicalDeviceQueueFamilyProperties =
    unsafe extern "C" fn(
        device: VkPhysicalDevice,
        count: *mut u32,
        properties: *mut c_void,
    );

/// `PFN_vkCreateDevice`
pub type PfnCreateDevice =
    unsafe extern "C" fn(
        physical_device: VkPhysicalDevice,
        create_info: *const c_void,
        allocator: *const c_void,
        device: *mut VkDevice,
    ) -> VkResult;

/// `PFN_vkDestroyDevice`
pub type PfnDestroyDevice =
    unsafe extern "C" fn(device: VkDevice, allocator: *const c_void);

/// `PFN_vkGetDeviceQueue`
pub type PfnGetDeviceQueue =
    unsafe extern "C" fn(
        device: VkDevice,
        queue_family: u32,
        queue_index: u32,
        queue: *mut VkQueue,
    );

// ─────────────────────────────────────────────────────────────────────────────
// VulkanLib — dynamic library handle
// ─────────────────────────────────────────────────────────────────────────────

/// Handle to the loaded `libvulkan.so.1` with core function pointers.
pub struct VulkanLib {
    handle: *mut c_void,
    pub get_instance_proc_addr: PfnGetInstanceProcAddr,
}

impl VulkanLib {
    /// Paths to try when loading the Vulkan library.
    const LIB_PATHS: &[&[u8]] = &[
        b"libvulkan.so.1\0",
        b"libvulkan.so\0",
    ];

    /// Load the Vulkan library and resolve `vkGetInstanceProcAddr`.
    pub fn load() -> Result<Self, VkError> {
        let mut handle: *mut c_void = core::ptr::null_mut();

        for path in Self::LIB_PATHS {
            handle = unsafe { syscall::dlopen(path.as_ptr(), syscall::RTLD_NOW) };
            if !handle.is_null() {
                break;
            }
        }

        if handle.is_null() {
            let err_msg = unsafe {
                let p = syscall::dlerror();
                if p.is_null() {
                    "unknown error".to_string()
                } else {
                    let mut len = 0;
                    while *p.add(len) != 0 { len += 1; }
                    String::from_utf8_lossy(core::slice::from_raw_parts(p, len)).to_string()
                }
            };
            return Err(VkError::LoadFailed(err_msg));
        }

        let get_instance_proc_addr: PfnGetInstanceProcAddr = unsafe {
            let sym = syscall::dlsym(handle, b"vkGetInstanceProcAddr\0".as_ptr());
            if sym.is_null() {
                syscall::dlclose(handle);
                return Err(VkError::SymbolNotFound("vkGetInstanceProcAddr"));
            }
            core::mem::transmute(sym)
        };

        Ok(Self {
            handle,
            get_instance_proc_addr,
        })
    }

    /// Resolve an instance-level function pointer by name.
    ///
    /// # Safety
    /// The returned pointer must be cast to the correct function type.
    pub unsafe fn get_proc(&self, instance: VkInstance, name: &[u8]) -> *mut c_void {
        // name must be NUL-terminated
        unsafe { (self.get_instance_proc_addr)(instance, name.as_ptr()) as *mut c_void }
    }

    /// Load a function pointer, returning `Err` if not found.
    pub fn load_fn<F>(&self, instance: VkInstance, name: &'static [u8]) -> Result<F, VkError>
    where
        F: Copy,
    {
        let ptr = unsafe { self.get_proc(instance, name) };
        if ptr.is_null() {
            // Strip NUL for error message
            let name_str = core::str::from_utf8(&name[..name.len().saturating_sub(1)])
                .unwrap_or("unknown");
            // We can't easily include the name in SymbolNotFound(&'static str)
            // so we use a static str approach
            let _ = name_str;
            return Err(VkError::SymbolNotFound("vulkan function"));
        }
        Ok(unsafe { core::mem::transmute_copy(&ptr) })
    }
}

impl Drop for VulkanLib {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                syscall::dlclose(self.handle);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// VulkanContext — high-level Vulkan state
// ─────────────────────────────────────────────────────────────────────────────

/// High-level Vulkan context holding all initialized Vulkan objects.
///
/// The initialization sequence is:
/// 1. `dlopen("libvulkan.so.1")`
/// 2. `vkCreateInstance` (with VK_KHR_surface + VK_KHR_xcb_surface extensions)
/// 3. `vkEnumeratePhysicalDevices` → select device with graphics queue
/// 4. `vkCreateDevice` (with VK_KHR_swapchain extension)
/// 5. `vkCreateXcbSurfaceKHR` or `vkCreateXlibSurfaceKHR`
/// 6. `vkCreateSwapchainKHR`
/// 7. Create render pass, pipeline, framebuffers, command buffers, sync objects
pub struct VulkanContext {
    pub lib: VulkanLib,
    pub instance: VkInstance,
    pub physical_device: VkPhysicalDevice,
    pub device: VkDevice,
    pub graphics_queue: VkQueue,
    pub present_queue: VkQueue,
    pub graphics_queue_family: u32,
    pub present_queue_family: u32,
    pub surface: VkSurfaceKHR,
    pub swapchain: VkSwapchainKHR,
    pub swapchain_images: Vec<VkImage>,
    pub swapchain_image_views: Vec<VkImageView>,
    pub render_pass: VkRenderPass,
    pub framebuffers: Vec<VkFramebuffer>,
    pub command_pool: VkCommandPool,
    pub command_buffers: Vec<VkCommandBuffer>,
    pub image_available_semaphore: VkSemaphore,
    pub render_finished_semaphore: VkSemaphore,
    pub in_flight_fence: VkFence,
    pub width: u32,
    pub height: u32,
}

impl VulkanContext {
    /// Create a new Vulkan context. This performs the full initialization sequence.
    ///
    /// Currently returns a placeholder — the actual Vulkan setup requires
    /// substantial struct definitions (VkApplicationInfo, VkInstanceCreateInfo, etc.)
    /// which will be fully implemented in the gfx_vulkan crate.
    pub fn new(_display_num: u32, _window_id: u32, _width: u32, _height: u32) -> Result<Self, VkError> {
        let _lib = VulkanLib::load()?;

        // TODO: Full Vulkan initialization sequence
        // This will be implemented in gfx_vulkan crate which uses platform_linux
        // as a building block. Here we provide the raw loader and types.

        todo!("Full Vulkan initialization will be implemented in gfx_vulkan crate. \
               Use VulkanLib::load() directly for now.")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vk_result_constants() {
        assert_eq!(VK_SUCCESS, 0);
        assert!(VK_ERROR_OUT_OF_HOST_MEMORY < 0);
        assert!(VK_NOT_READY > 0);
    }

    #[test]
    fn vk_error_display() {
        let e = VkError::LoadFailed("libvulkan.so not found".into());
        assert!(format!("{e}").contains("libvulkan.so not found"));

        let e = VkError::SymbolNotFound("vkCreateInstance");
        assert!(format!("{e}").contains("vkCreateInstance"));

        let e = VkError::ApiError { function: "vkCreateDevice", result: -3 };
        assert!(format!("{e}").contains("vkCreateDevice"));
        assert!(format!("{e}").contains("-3"));
    }

    #[test]
    fn vk_structure_type_constants() {
        assert_eq!(VK_STRUCTURE_TYPE_APPLICATION_INFO, 0);
        assert_eq!(VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO, 1);
        assert_eq!(VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO, 3);
    }

    // NOTE: VulkanLib::load() is not tested in CI because it requires
    // libvulkan.so to be installed. Integration tests should be run
    // on a machine with a Vulkan-capable GPU.
}
