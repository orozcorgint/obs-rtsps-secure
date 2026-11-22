//! OBS source backed by a certificate-pinned Rust TLS bridge and OBS Media Source.
#![cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]

mod audio;
mod discovery;
mod obs;
mod proxy;

use obs::Obs;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;
use std::sync::{
    atomic::{AtomicPtr, Ordering},
    Mutex,
};

const OBS_SOURCE_VIDEO: u32 = 1;
const OBS_SOURCE_AUDIO: u32 = 2;
static MODULE: AtomicPtr<Obs> = AtomicPtr::new(ptr::null_mut());

pub(crate) fn log(level: i32, text: &str) {
    if let Ok(message) = CString::new(format!("[RTSPS Secure] {text}")) {
        unsafe {
            obs::blog(level, c"%s".as_ptr(), message.as_ptr());
        }
    }
}

struct Runtime {
    config: Option<proxy::Config>,
    proxy: Option<proxy::Proxy>,
}
struct Source {
    parent: *mut Obs,
    child: *mut Obs,
    audio: audio::AudioBridge,
    discovery: Mutex<Option<discovery::Discovery>>,
    runtime: Mutex<Runtime>,
}

unsafe fn setting(settings: *mut Obs, key: &CStr) -> String {
    let value = unsafe { obs::obs_data_get_string(settings, key.as_ptr()) };
    if value.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned()
    }
}

unsafe fn child_settings(url: &str) -> *mut Obs {
    let data = unsafe { obs::obs_data_create() };
    if data.is_null() {
        return data;
    }
    let url = CString::new(url).unwrap_or_default();
    unsafe {
        obs::obs_data_set_bool(data, c"is_local_file".as_ptr(), false);
        obs::obs_data_set_bool(data, c"restart_on_activate".as_ptr(), false);
        obs::obs_data_set_bool(data, c"close_when_inactive".as_ptr(), false);
        obs::obs_data_set_string(data, c"input".as_ptr(), url.as_ptr());
        obs::obs_data_set_string(
            data,
            c"ffmpeg_options".as_ptr(),
            c"rtsp_transport=tcp".as_ptr(),
        );
    }
    data
}

unsafe fn apply_child(source: &Source, url: &str) {
    let settings = unsafe { child_settings(url) };
    if !settings.is_null() {
        unsafe {
            obs::obs_source_update(source.child, settings);
            obs::obs_data_release(settings);
        }
    }
}

unsafe extern "C" fn name(_: *mut c_void) -> *const c_char {
    c"RTSPS Secure".as_ptr()
}

unsafe extern "C" fn create(settings: *mut Obs, parent: *mut Obs) -> *mut c_void {
    let child_data = unsafe { child_settings("") };
    if child_data.is_null() {
        return ptr::null_mut();
    }
    let child = unsafe {
        obs::obs_source_create_private(
            c"ffmpeg_source".as_ptr(),
            c"RTSPS interno".as_ptr(),
            child_data,
        )
    };
    unsafe {
        obs::obs_data_release(child_data);
    }
    if child.is_null() {
        log(100, "Could not create the OBS media source");
        return ptr::null_mut();
    }
    if !unsafe { obs::obs_source_add_active_child(parent, child) } {
        unsafe {
            obs::obs_source_release(child);
        }
        return ptr::null_mut();
    }
    let audio = match audio::AudioBridge::new(parent) {
        Ok(audio) => audio,
        Err(error) => {
            log(100, &format!("Could not start audio: {error}"));
            unsafe {
                obs::obs_source_remove_active_child(parent, child);
                obs::obs_source_release(child);
            }
            return ptr::null_mut();
        }
    };
    let source = Box::new(Source {
        parent,
        child,
        audio,
        discovery: Mutex::new(Some(discovery::Discovery::new())),
        runtime: Mutex::new(Runtime {
            config: None,
            proxy: None,
        }),
    });
    let ptr = Box::into_raw(source).cast::<c_void>();
    unsafe {
        obs::obs_source_add_audio_capture_callback(child, forward_audio, ptr);
    }
    unsafe {
        update(ptr, settings);
    }
    ptr
}

unsafe extern "C" fn destroy(data: *mut c_void) {
    if data.is_null() {
        return;
    }
    let mut source = unsafe { Box::from_raw(data.cast::<Source>()) };
    // Stop the listener before releasing the media source; neither worker nor
    // callback may outlive the OBS source.
    // Do not hold our state lock while joining: the completing worker can
    // notify an OBS listener that synchronously reads the properties.
    let discovery = source
        .discovery
        .lock()
        .ok()
        .and_then(|mut slot| slot.take());
    drop(discovery);
    unsafe {
        obs::obs_source_remove_audio_capture_callback(source.child, forward_audio, data);
    }
    source.audio.stop();
    if let Ok(mut runtime) = source.runtime.lock() {
        runtime.proxy.take();
    }
    unsafe {
        obs::obs_source_remove_active_child(source.parent, source.child);
        obs::obs_source_release(source.child);
    }
}

unsafe extern "C" fn update(data: *mut c_void, settings: *mut Obs) {
    if data.is_null() || settings.is_null() {
        return;
    }
    let source = unsafe { &*data.cast::<Source>() };
    let url = unsafe { setting(settings, c"url") };
    let certificate = unsafe { setting(settings, c"certificate") };
    let parsed = if url.trim().is_empty() || certificate.trim().is_empty() {
        None
    } else {
        match proxy::Config::parse(&url, &certificate) {
            Ok(config) => Some(config),
            Err(e) => {
                log(200, &e);
                None
            }
        }
    };
    let Ok(mut runtime) = source.runtime.lock() else {
        return;
    };
    if runtime.config == parsed {
        return;
    }
    unsafe {
        apply_child(source, "");
    }
    runtime.proxy.take();
    runtime.config = None;
    if let Some(config) = parsed {
        match proxy::Proxy::start(config.clone()) {
            Ok(proxy) => {
                unsafe {
                    apply_child(source, &proxy.local_url);
                }
                runtime.config = Some(config);
                runtime.proxy = Some(proxy);
                log(
                    300,
                    "Local bridge started; the certificate will be verified on connect",
                );
            }
            Err(e) => log(200, &e),
        }
    }
}

unsafe extern "C" fn properties(data: *mut c_void) -> *mut Obs {
    unsafe {
        let props = obs::obs_properties_create();
        let url = obs::obs_properties_add_text(
            props,
            c"url".as_ptr(),
            c"RTSPS URL (numeric IP)".as_ptr(),
            0,
        );
        obs::obs_property_set_long_description(url, c"Example: rtsps://192.0.2.1:8554/live (replace with your camera's address). Uses RTSP over TCP and does not accept credentials.".as_ptr());
        let (message, can_approve) = if data.is_null() {
            (
                "Enter the URL, then fetch the device certificate.".to_string(),
                false,
            )
        } else {
            let source = &*data.cast::<Source>();
            match source.discovery.lock() {
                Ok(discovery) => match discovery.as_ref() {
                    Some(discovery) => (discovery.message(), discovery.can_approve()),
                    None => ("Source is closing".to_string(), false),
                },
                Err(_) => ("Certificate status unavailable".to_string(), false),
            }
        };
        let message = CString::new(message).unwrap_or_default();
        let info = obs::obs_properties_add_text(
            props,
            c"certificate_status".as_ptr(),
            message.as_ptr(),
            3,
        );
        obs::obs_property_text_set_info_type(info, 1);
        obs::obs_property_text_set_info_word_wrap(info, true);
        let fetch = obs::obs_properties_add_button2(
            props,
            c"fetch_certificate".as_ptr(),
            c"Fetch device certificate".as_ptr(),
            Some(fetch_certificate),
            data,
        );
        obs::obs_property_set_enabled(fetch, !data.is_null());
        let approve = obs::obs_properties_add_button2(
            props,
            c"approve_certificate".as_ptr(),
            c"I verified it: use certificate".as_ptr(),
            Some(approve_certificate),
            data,
        );
        obs::obs_property_set_enabled(approve, can_approve);
        let cert = obs::obs_properties_add_path(
            props,
            c"certificate".as_ptr(),
            c"Camera certificate (PEM)".as_ptr(),
            0,
            c"PEM certificates (*.pem *.crt);;All files (*.*)".as_ptr(),
            ptr::null(),
        );
        obs::obs_property_set_long_description(cert, c"Must contain the exact server certificate. Verify its fingerprint with the device through an independent channel before trusting it.".as_ptr());
        props
    }
}

unsafe fn current_url(source: &Source) -> Option<String> {
    let settings = unsafe { obs::obs_source_get_settings(source.parent) };
    if settings.is_null() {
        return None;
    }
    let url = unsafe { setting(settings, c"url") };
    unsafe {
        obs::obs_data_release(settings);
    }
    Some(url)
}

/// Update the existing property definitions without synchronously rebuilding
/// the Qt property view while its button callback is still on the stack.
/// Returning true lets OBS refresh that view later on its event loop.
unsafe fn reflect_discovery_status(props: *mut Obs, source: &Source) {
    if props.is_null() {
        return;
    }
    let (message, can_approve) = match source.discovery.lock() {
        Ok(slot) => match slot.as_ref() {
            Some(discovery) => (discovery.message(), discovery.can_approve()),
            None => ("Source is closing".to_string(), false),
        },
        Err(_) => ("Certificate status unavailable".to_string(), false),
    };
    if let Ok(message) = CString::new(message) {
        unsafe {
            let status = obs::obs_properties_get(props, c"certificate_status".as_ptr());
            if !status.is_null() {
                obs::obs_property_set_description(status, message.as_ptr());
            }
            let approval = obs::obs_properties_get(props, c"approve_certificate".as_ptr());
            if !approval.is_null() {
                obs::obs_property_set_enabled(approval, can_approve);
            }
        }
    }
}

unsafe extern "C" fn fetch_certificate(props: *mut Obs, _: *mut Obs, data: *mut c_void) -> bool {
    if data.is_null() {
        return false;
    }
    let source = unsafe { &*data.cast::<Source>() };
    let url = unsafe { current_url(source) }.unwrap_or_default();
    if let Ok(mut slot) = source.discovery.lock() {
        if let Some(discovery) = slot.as_mut() {
            if let Err(error) = discovery.start(&url, source.parent) {
                discovery.error(error);
            }
        }
    }
    unsafe { reflect_discovery_status(props, source) };
    true
}

unsafe fn certificate_directory() -> Result<std::path::PathBuf, String> {
    let module = MODULE.load(Ordering::Acquire);
    if module.is_null() {
        return Err("The OBS module is unavailable".into());
    }
    let raw = unsafe { obs::obs_module_get_config_path(module, c"certificates".as_ptr()) };
    if raw.is_null() {
        return Err("The OBS configuration folder was not found".into());
    }
    let path = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    unsafe {
        obs::bfree(raw.cast());
    }
    if path.is_empty() {
        return Err("The OBS configuration folder was not found".into());
    }
    Ok(std::path::PathBuf::from(path))
}

unsafe extern "C" fn approve_certificate(props: *mut Obs, _: *mut Obs, data: *mut c_void) -> bool {
    if data.is_null() {
        return false;
    }
    let source = unsafe { &*data.cast::<Source>() };
    let url = unsafe { current_url(source) }.unwrap_or_default();
    if let Ok(slot) = source.discovery.lock() {
        if let Some(discovery) = slot.as_ref() {
            let result = unsafe { certificate_directory() }
                .and_then(|directory| discovery.approve(&url, &directory));
            match result {
                Ok(path) => {
                    let settings = unsafe { obs::obs_source_get_settings(source.parent) };
                    if !settings.is_null() {
                        if let Ok(path) = CString::new(path.to_string_lossy().as_bytes()) {
                            unsafe {
                                obs::obs_data_set_string(
                                    settings,
                                    c"certificate".as_ptr(),
                                    path.as_ptr(),
                                );
                                obs::obs_source_update(source.parent, settings);
                            }
                        }
                        unsafe {
                            obs::obs_data_release(settings);
                        }
                    }
                }
                Err(error) => discovery.error(error),
            }
        }
    }
    unsafe { reflect_discovery_status(props, source) };
    true
}

unsafe extern "C" fn width(data: *mut c_void) -> u32 {
    if data.is_null() {
        return 0;
    }
    unsafe { obs::obs_source_get_width((*data.cast::<Source>()).child) }
}
unsafe extern "C" fn height(data: *mut c_void) -> u32 {
    if data.is_null() {
        return 0;
    }
    unsafe { obs::obs_source_get_height((*data.cast::<Source>()).child) }
}
unsafe extern "C" fn render(data: *mut c_void, _: *mut Obs) {
    if !data.is_null() {
        unsafe {
            obs::obs_source_video_render((*data.cast::<Source>()).child);
        }
    }
}
unsafe extern "C" fn enumerate(data: *mut c_void, callback: obs::EnumCallback, param: *mut c_void) {
    if data.is_null() {
        return;
    }
    let source = unsafe { &*data.cast::<Source>() };
    unsafe {
        callback(source.parent, source.child, param);
    }
}
unsafe extern "C" fn forward_audio(
    data: *mut c_void,
    _: *mut Obs,
    audio: *const obs::AudioData,
    muted: bool,
) {
    if data.is_null() || audio.is_null() || muted {
        return;
    }
    let mut info = obs::AudioInfo {
        samples_per_sec: 0,
        speakers: 0,
    };
    if !unsafe { obs::obs_get_audio_info(&mut info) } {
        return;
    }
    let source = unsafe { &*data.cast::<Source>() };
    unsafe { source.audio.enqueue(&*audio, &info) };
}

#[no_mangle]
pub extern "C" fn obs_module_set_pointer(module: *mut c_void) {
    MODULE.store(module.cast::<Obs>(), Ordering::Release);
}
#[no_mangle]
pub extern "C" fn obs_module_ver() -> u32 {
    (33 << 24) | 1
}
#[no_mangle]
pub extern "C" fn obs_module_load() -> bool {
    let mut info: obs::SourceInfo = unsafe { std::mem::zeroed() };
    info.id = c"rtsps_secure_source".as_ptr();
    info.source_type = 0;
    info.output_flags = OBS_SOURCE_VIDEO | OBS_SOURCE_AUDIO;
    info.get_name = Some(name);
    info.create = Some(create);
    info.destroy = Some(destroy);
    info.get_width = Some(width);
    info.get_height = Some(height);
    info.get_properties = Some(properties);
    info.update = Some(update);
    info.video_render = Some(render);
    info.enum_active_sources = Some(enumerate);
    unsafe {
        obs::obs_register_source_s(&info, std::mem::size_of_val(&info));
    }
    log(300, "Plugin loaded");
    true
}

#[no_mangle]
pub extern "C" fn obs_module_name() -> *const c_char {
    c"RTSPS Secure".as_ptr()
}
