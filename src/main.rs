#![allow(
    dead_code,
    unused_variables,
    clippy::too_many_arguments,
    clippy::unnecessary_wraps
)]

use std::collections::HashSet;
use std::ffi::CStr;
use std::os::raw::c_void;

use anyhow::{anyhow, Result};
use log::*;
use thiserror::Error;

use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};
use vulkanalia::loader::{LibloadingLoader, LIBRARY};
use vulkanalia::window as vk_window;
use vulkanalia::prelude::v1_0::*;

use vulkanalia::vk::{ExtDebugUtilsExtension, KhrSurfaceExtension};

const VALIDATION_ENABLED: bool = cfg!(debug_assertions);
const VALIDATION_LAYER: vk::ExtensionName = vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");


/// Our Vulkan App
#[derive (Clone, Debug)]
struct App {
    entry: Entry,
    instance: Instance,
    data: AppData,
    device: Device,
}

impl App {
    /// Creates our Vulkan App
    unsafe fn create (window: &Window) -> Result<Self> {
        let mut data = AppData::default();
        let loader = LibloadingLoader::new (LIBRARY)?;
        let entry = Entry::new (loader).map_err(|b| anyhow!("{}", b))?;
        // Only for X11
        let instance = create_instance(window, &entry, &mut data)?;
        data.surface = vk_window::create_surface(&instance, window)?;
        pick_physical_device(&instance, &mut data)?;
        let device = create_logical_device(&instance, &mut data)?;
        Ok(Self {entry, instance, data, device})
    }

    /// Render a frame for out vulkan app
    unsafe fn render (&mut self, window: &Window) -> Result<()> {
        Ok(())
    }

    /// Destroyes out Vulkan app
    unsafe fn destroy (&mut self) {
        if VALIDATION_ENABLED {
            self.instance.destroy_debug_utils_messenger_ext(self.data.messenger, None);
        }
        self.instance.destroy_surface_khr(self.data.surface, None);
        self.device.destroy_device(None);
        self.instance.destroy_instance(None);
    }
}

#[derive (Clone, Debug, Default)]
struct AppData{
    messenger: vk::DebugUtilsMessengerEXT,
    physical_device: vk::PhysicalDevice,
    graphics_queue: vk::Queue,
    surface: vk::SurfaceKHR,
    //present_queue: vk::Queue
}

#[derive(Debug, Error)]
#[error("Missing {0}.")]
pub struct SuitabilityError (pub &'static str);

unsafe fn pick_physical_device (instance: &Instance, data: &mut AppData) -> Result<()>{
    for physical_device in instance.enumerate_physical_devices()? {
        let proparties = instance.get_physical_device_properties(physical_device);
        if let Err(error) = check_physical_device(instance, data, physical_device) {
            warn!("Skipping physical device (`{}`): {}", proparties.device_name, error);
        } else {
            info!("Selected Device is `{}`", proparties.device_name);
            data.physical_device = physical_device;
            return Ok(())
        }
    }
    Err (anyhow!("Failed to find suitable grpahics device"))
}

unsafe fn check_physical_device (
    instance: &Instance,
    data: &AppData,
    physical_device: vk::PhysicalDevice
) -> Result<()> {
    let proparties = instance.get_physical_device_properties(physical_device);
    if String::from_utf8_lossy (proparties.device_name.as_bytes()).contains("NVIDIA") {
        QueueFamilyIndices::get(instance, data, physical_device)?;
        Ok(())
    } else {
        Err (anyhow!("!Nvidia"))
    }
}

unsafe fn create_logical_device (instance: &Instance, data: &mut AppData) -> Result<Device> {
    let indices = QueueFamilyIndices::get (instance, data, data.physical_device)?;
    let queue_priorities = &[1.0];

    let queue_info = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index (indices.graphics)
        .queue_priorities (queue_priorities);

    let leayers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        vec![]
    };

    let features = vk::PhysicalDeviceFeatures::builder();
    let queue_info = &[queue_info];
    let info = vk::DeviceCreateInfo::builder()
        .queue_create_infos (queue_info)
        .enabled_layer_names (&leayers)
        .enabled_features (&features);
    let device = instance.create_device(data.physical_device, &info, None)?;
    data.graphics_queue = device.get_device_queue(indices.graphics, 0);
    Ok(device)
}

#[derive(Copy, Clone, Debug)]
struct QueueFamilyIndices {
    graphics: u32,
    present: u32,
}

impl QueueFamilyIndices {
    unsafe fn get (
        instance: &Instance,
        data: &AppData,
        physical_device: vk::PhysicalDevice
    ) -> Result<Self> {
        let proparties = instance.get_physical_device_queue_family_properties(physical_device);
        let graphics = proparties.iter()
                                 .position (|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                                 .map(|i| i as u32);

        // Look for Presentation support
        let mut present = None;
        for (index, properties) in proparties.iter().enumerate() {
            if instance.get_physical_device_surface_support_khr(physical_device, index as u32, data.surface)? {
                present = Some (index as u32);
                break;
            }
        }

        // Look for Graphics support
        if let (Some(graphics), Some(present)) = (graphics, present) {
            Ok (Self { graphics, present })
        }
        else {
            Err(anyhow!(SuitabilityError("Missing required queue families")))
        }

    }
}

unsafe fn create_instance (window: &Window, entry: &Entry, data: &mut AppData) -> Result<Instance>{
    let applicatoin_info = vk::ApplicationInfo::builder()
    .application_name (b"Vulkan Application in Rust\0")
    .application_version (vk::make_version(1, 0, 0))
    .engine_name (b"No Engine\0")
    .engine_version (vk::make_version(1, 0, 0))
    .api_version (vk::make_version(1, 0, 0));

    let mut extensions = vk_window::get_required_instance_extensions(window)
        .iter ()
        .map (|e| e.as_ptr())
        .collect::<Vec<_>>();
    info!("Required Extenstions: {:?}", extensions.iter().map (|s| CStr::from_ptr(*s)).collect::<Vec<_>>() );
    if VALIDATION_ENABLED {
        extensions.push (vk::EXT_DEBUG_UTILS_EXTENSION.name.as_ptr());
    }

    let available_layers = entry
        .enumerate_instance_layer_properties()?
        .iter()
        .map(|l| l.layer_name)
        .collect::<HashSet<_>>();

    if VALIDATION_ENABLED && !available_layers.contains(&VALIDATION_LAYER) {
        return Err(anyhow!("Validation layer requetsed but not supported."))
    }

    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        Vec::new()
    };

    let mut info = vk::InstanceCreateInfo::builder()
        .application_info (&applicatoin_info)
        .enabled_layer_names(&layers)
        .enabled_extension_names (&extensions);

    let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
        .message_severity (vk::DebugUtilsMessageSeverityFlagsEXT::all())
        .message_type (vk::DebugUtilsMessageTypeFlagsEXT::all())
        .user_callback (Some(debug_callback));
        
    if VALIDATION_ENABLED {
        info = info.push_next(&mut debug_info);
    }

    let instance = entry.create_instance(&info, None)?;

    if VALIDATION_ENABLED {
        data.messenger = instance.create_debug_utils_messenger_ext(&debug_info, None)?;
    }

    Ok (instance)
}

extern "system" fn debug_callback (
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    type_: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void
) -> vk::Bool32 {
    let data = unsafe { *data };
    let message = unsafe { CStr::from_ptr(data.message) }.to_string_lossy();

    if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR { 
        error!("({:?}) {}", type_, message); 
    }
    else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING { 
        warn!("({:?}) {}", type_, message);
    }
    else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::INFO { 
        debug!("({:?}) {}", type_, message);
    }
    else {
        trace!("({:?}) {}", type_, message);
    }

    vk::FALSE
}



fn main() -> Result<()> {
    pretty_env_logger::init();

    // Window

    let event_loop = EventLoop::new ();
    let window = WindowBuilder::new ()
        .with_title("Hey Vulkan")
        .with_inner_size(LogicalSize::new(1024, 768))
        .build(&event_loop)?;

    // App

    let mut app = unsafe { App::create (&window)? };
    let mut destroying = false;
    event_loop.run (move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            // Render a frame if our Vulkan app is not being destrpyed
            Event::MainEventsCleared if !destroying => unsafe {
                app.render (&window)
            }.unwrap(),

            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                destroying = true;
                *control_flow = ControlFlow::Exit;
                unsafe { app.destroy(); }
            }
            _ => {}
        }
    })
}
