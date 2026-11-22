//! The small subset of the OBS 33 C ABI used by this plugin.
use std::ffi::{c_char, c_int, c_void};

pub type Obs = c_void;
pub type EnumCallback = unsafe extern "C" fn(*mut Obs, *mut Obs, *mut c_void);
pub type AudioCallback = unsafe extern "C" fn(*mut c_void, *mut Obs, *const AudioData, bool);
pub type ButtonCallback = unsafe extern "C" fn(*mut Obs, *mut Obs, *mut c_void) -> bool;

#[repr(C)]
pub struct AudioData {
    pub data: [*const u8; 8],
    pub frames: u32,
    pub timestamp: u64,
}
#[repr(C)]
pub struct SourceAudio {
    pub data: [*const u8; 8],
    pub frames: u32,
    pub speakers: c_int,
    pub format: c_int,
    pub samples_per_sec: u32,
    pub timestamp: u64,
}
#[repr(C)]
pub struct AudioInfo {
    pub samples_per_sec: u32,
    pub speakers: c_int,
}

// Prefix of obs_source_info through audio_render; OBS accepts a size argument
// and zero-initializes fields introduced after this prefix.
#[repr(C)]
pub struct SourceInfo {
    pub id: *const c_char,
    pub source_type: c_int,
    pub output_flags: u32,
    pub get_name: Option<unsafe extern "C" fn(*mut c_void) -> *const c_char>,
    pub create: Option<unsafe extern "C" fn(*mut Obs, *mut Obs) -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub get_width: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    pub get_height: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    pub get_defaults: Option<unsafe extern "C" fn(*mut Obs)>,
    pub get_properties: Option<unsafe extern "C" fn(*mut c_void) -> *mut Obs>,
    pub update: Option<unsafe extern "C" fn(*mut c_void, *mut Obs)>,
    pub activate: *const c_void,
    pub deactivate: *const c_void,
    pub show: *const c_void,
    pub hide: *const c_void,
    pub video_tick: *const c_void,
    pub video_render: Option<unsafe extern "C" fn(*mut c_void, *mut Obs)>,
    pub filter_video: *const c_void,
    pub filter_audio: *const c_void,
    pub enum_active_sources: Option<unsafe extern "C" fn(*mut c_void, EnumCallback, *mut c_void)>,
    pub save: *const c_void,
    pub load: *const c_void,
    pub mouse_click: *const c_void,
    pub mouse_move: *const c_void,
    pub mouse_wheel: *const c_void,
    pub focus: *const c_void,
    pub key_click: *const c_void,
    pub filter_remove: *const c_void,
    pub type_data: *mut c_void,
    pub free_type_data: *const c_void,
    pub audio_render: *const c_void,
}

#[cfg_attr(target_os = "windows", link(name = "obs", kind = "raw-dylib"))]
#[cfg_attr(target_os = "linux", link(name = "obs"))]
unsafe extern "C" {
    pub fn blog(level: c_int, format: *const c_char, ...);
    pub fn obs_register_source_s(info: *const SourceInfo, size: usize);
    pub fn obs_data_create() -> *mut Obs;
    pub fn obs_data_release(data: *mut Obs);
    pub fn obs_data_set_string(data: *mut Obs, name: *const c_char, value: *const c_char);
    pub fn obs_data_set_bool(data: *mut Obs, name: *const c_char, value: bool);
    pub fn obs_data_get_string(data: *mut Obs, name: *const c_char) -> *const c_char;
    pub fn obs_source_get_settings(source: *const Obs) -> *mut Obs;
    pub fn obs_source_update_properties(source: *mut Obs);
    pub fn obs_module_get_config_path(module: *mut Obs, file: *const c_char) -> *mut c_char;
    pub fn bfree(ptr: *mut c_void);
    pub fn obs_properties_create() -> *mut Obs;
    pub fn obs_properties_get(props: *mut Obs, name: *const c_char) -> *mut Obs;
    pub fn obs_properties_add_text(
        props: *mut Obs,
        name: *const c_char,
        description: *const c_char,
        kind: c_int,
    ) -> *mut Obs;
    pub fn obs_properties_add_path(
        props: *mut Obs,
        name: *const c_char,
        description: *const c_char,
        kind: c_int,
        filter: *const c_char,
        default_path: *const c_char,
    ) -> *mut Obs;
    pub fn obs_property_set_long_description(prop: *mut Obs, description: *const c_char);
    pub fn obs_property_set_description(prop: *mut Obs, description: *const c_char);
    pub fn obs_property_text_set_info_type(prop: *mut Obs, kind: c_int);
    pub fn obs_property_text_set_info_word_wrap(prop: *mut Obs, word_wrap: bool);
    pub fn obs_property_set_enabled(prop: *mut Obs, enabled: bool);
    pub fn obs_properties_add_button2(
        props: *mut Obs,
        name: *const c_char,
        description: *const c_char,
        callback: Option<ButtonCallback>,
        private_data: *mut c_void,
    ) -> *mut Obs;
    pub fn obs_source_create_private(
        id: *const c_char,
        name: *const c_char,
        settings: *mut Obs,
    ) -> *mut Obs;
    pub fn obs_source_release(source: *mut Obs);
    pub fn obs_source_update(source: *mut Obs, settings: *mut Obs);
    pub fn obs_source_add_active_child(parent: *mut Obs, child: *mut Obs) -> bool;
    pub fn obs_source_remove_active_child(parent: *mut Obs, child: *mut Obs);
    pub fn obs_source_get_width(source: *mut Obs) -> u32;
    pub fn obs_source_get_height(source: *mut Obs) -> u32;
    pub fn obs_source_video_render(source: *mut Obs);
    pub fn obs_get_audio_info(info: *mut AudioInfo) -> bool;
    pub fn obs_source_output_audio(source: *mut Obs, audio: *const SourceAudio);
    pub fn obs_source_add_audio_capture_callback(
        source: *mut Obs,
        callback: AudioCallback,
        param: *mut c_void,
    );
    pub fn obs_source_remove_audio_capture_callback(
        source: *mut Obs,
        callback: AudioCallback,
        param: *mut c_void,
    );
}
