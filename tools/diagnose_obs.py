"""Probe OBS's own Media Source without changing the user's OBS profile.

Run on Windows against a stream explicitly supplied on the command line. This
uses installed OBS DLLs in a separate process; it does not open OBS itself.
"""

import ctypes as c
import hashlib
import os
import ssl
import sys
import threading
import time
from pathlib import Path


class VideoInfo(c.Structure):
    _fields_ = [
        ("graphics_module", c.c_char_p),
        ("fps_num", c.c_uint32), ("fps_den", c.c_uint32),
        ("base_width", c.c_uint32), ("base_height", c.c_uint32),
        ("output_width", c.c_uint32), ("output_height", c.c_uint32),
        ("output_format", c.c_int), ("adapter", c.c_uint32),
        ("gpu_conversion", c.c_bool),
        ("colorspace", c.c_int), ("range", c.c_int), ("scale_type", c.c_int),
    ]


class AudioInfo(c.Structure):
    _fields_ = [("samples_per_sec", c.c_uint32), ("speakers", c.c_int)]


def bind(lib, name, result, *args):
    func = getattr(lib, name)
    func.restype, func.argtypes = result, list(args)
    return func


def main():
    if len(sys.argv) < 2:
        raise SystemExit("Usage: python tools/diagnose_obs.py <stream-url> [ffmpeg-options]")
    url = sys.argv[1]
    options = sys.argv[2] if len(sys.argv) > 2 else "rtsp_transport=tcp"
    use_plugin = os.environ.get("OBS_RTSPS_TEST_PLUGIN") == "1"
    project = Path(__file__).resolve().parent.parent
    plugin = Path(os.environ.get("OBS_RTSPS_PLUGIN_FILE", str(project / "target" / "x86_64-pc-windows-gnu" / "release" / "rtsps_secure.dll"))).resolve()
    certificate_path = os.environ.get("OBS_RTSPS_CERT_FILE", "")
    certificate = Path(certificate_path).resolve() if certificate_path else None
    expect_failure = os.environ.get("OBS_RTSPS_EXPECT_FAILURE") == "1"
    test_mute = os.environ.get("OBS_RTSPS_TEST_MUTE") == "1"
    test_update = os.environ.get("OBS_RTSPS_TEST_UPDATE") == "1"
    test_discovery = os.environ.get("OBS_RTSPS_TEST_DISCOVERY") == "1"
    test_discovery_cancel = os.environ.get("OBS_RTSPS_TEST_DISCOVERY_CANCEL") == "1"
    if use_plugin and not (test_discovery or test_discovery_cancel) and certificate is None:
        raise SystemExit("Set OBS_RTSPS_CERT_FILE to the test server's certificate")
    test_stress = os.environ.get("OBS_RTSPS_TEST_STRESS") == "1"
    test_seconds = int(os.environ.get("OBS_RTSPS_TEST_SECONDS", "25"))
    root = Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "obs-studio"
    binary = root / "bin" / "64bit"
    os.add_dll_directory(str(binary))
    os.add_dll_directory(str(root / "core" / "obs-ffmpeg"))
    os.chdir(binary)
    obs = c.CDLL(str(binary / "obs.dll"))
    ptr = c.c_void_p
    startup = bind(obs, "obs_startup", c.c_bool, c.c_char_p, c.c_char_p, ptr)
    shutdown = bind(obs, "obs_shutdown", None)
    add_data = bind(obs, "obs_add_data_path", None, c.c_char_p)
    reset_audio = bind(obs, "obs_reset_audio", c.c_bool, c.POINTER(AudioInfo))
    reset_video = bind(obs, "obs_reset_video", c.c_int, c.POINTER(VideoInfo))
    open_module = bind(obs, "obs_open_module", c.c_int, c.POINTER(ptr), c.c_char_p, c.c_char_p)
    init_module = bind(obs, "obs_init_module", c.c_bool, ptr)
    data_create = bind(obs, "obs_data_create", ptr)
    data_set_string = bind(obs, "obs_data_set_string", None, ptr, c.c_char_p, c.c_char_p)
    data_set_bool = bind(obs, "obs_data_set_bool", None, ptr, c.c_char_p, c.c_bool)
    data_release = bind(obs, "obs_data_release", None, ptr)
    source_create = bind(obs, "obs_source_create_private", ptr, c.c_char_p, c.c_char_p, ptr)
    source_release = bind(obs, "obs_source_release", None, ptr)
    source_width = bind(obs, "obs_source_get_width", c.c_uint32, ptr)
    source_height = bind(obs, "obs_source_get_height", c.c_uint32, ptr)
    source_state = bind(obs, "obs_source_media_get_state", c.c_int, ptr)
    source_set_muted = bind(obs, "obs_source_set_muted", None, ptr, c.c_bool)
    source_update = bind(obs, "obs_source_update", None, ptr, ptr)
    source_settings = bind(obs, "obs_source_get_settings", ptr, ptr)
    data_get_string = bind(obs, "obs_data_get_string", c.c_char_p, ptr, c.c_char_p)
    source_properties = bind(obs, "obs_source_properties", ptr, ptr)
    properties_get = bind(obs, "obs_properties_get", ptr, ptr, c.c_char_p)
    properties_destroy = bind(obs, "obs_properties_destroy", None, ptr)
    property_description = bind(obs, "obs_property_description", c.c_char_p, ptr)
    property_click = bind(obs, "obs_property_button_clicked", c.c_bool, ptr, ptr)
    property_enabled = bind(obs, "obs_property_enabled", c.c_bool, ptr)
    source_display_name = bind(obs, "obs_source_get_display_name", c.c_char_p, c.c_char_p)
    wait_destroy = bind(obs, "obs_wait_for_destroy_queue", c.c_bool)
    set_output_source = bind(obs, "obs_set_output_source", None, c.c_uint32, ptr)
    video_cb_type = c.CFUNCTYPE(None, ptr, ptr)
    audio_cb_type = c.CFUNCTYPE(None, ptr, c.c_size_t, ptr)
    add_video_cb = bind(obs, "obs_add_raw_video_callback", None, ptr, video_cb_type, ptr)
    remove_video_cb = bind(obs, "obs_remove_raw_video_callback", None, video_cb_type, ptr)
    add_audio_cb = bind(obs, "obs_add_raw_audio_callback", None, c.c_size_t, ptr, audio_cb_type, ptr)
    remove_audio_cb = bind(obs, "obs_remove_raw_audio_callback", None, c.c_size_t, audio_cb_type, ptr)
    stats = {"frames": 0, "nonblack": 0, "audio_callbacks": 0, "audio_peak": 0.0, "interval_peak": 0.0}

    @video_cb_type
    def video_cb(_param, frame):
        stats["frames"] += 1
        if stats["frames"] % 15 != 0:
            return
        data = c.cast(frame, c.POINTER(ptr))[0]
        if data:
            # OBS was configured for 800x600 RGBA, sample the middle row.
            row = c.string_at(data + 300 * 800 * 4, 800 * 4)
            if max((row[i] for i in range(len(row)) if i % 4 != 3), default=0) > 10:
                stats["nonblack"] += 1

    @audio_cb_type
    def audio_cb(_param, _mix, frame):
        stats["audio_callbacks"] += 1
        data = c.cast(frame, c.POINTER(ptr))[0]
        if data:
            samples = c.cast(data, c.POINTER(c.c_float * 1024)).contents
            peak = max(abs(x) for x in samples)
            stats["audio_peak"] = max(stats["audio_peak"], peak)
            stats["interval_peak"] = max(stats["interval_peak"], peak)

    config_dir = Path(os.environ["LOCALAPPDATA"]) / "Temp" / "opencode" / "obs-rtsps-test-config"
    config_dir.mkdir(parents=True, exist_ok=True)
    if not startup(b"en-US", str(config_dir).encode(), None):
        raise RuntimeError("obs_startup failed")
    try:
        add_data(str(root / "data" / "libobs").encode())
        if not reset_audio(c.byref(AudioInfo(48000, 2))):
            raise RuntimeError("obs_reset_audio failed")
        video = VideoInfo(b"libobs-d3d11", 30, 1, 800, 600, 800, 600, 6, 0, False, 0, 0, 0)
        result = reset_video(c.byref(video))
        if result != 0:
            raise RuntimeError(f"obs_reset_video failed: {result}")
        module = ptr()
        result = open_module(c.byref(module), str(root / "core" / "obs-ffmpeg" / "obs-ffmpeg.dll").encode(),
                             str(root / "data" / "obs-plugins" / "obs-ffmpeg").encode())
        if result != 0 or not init_module(module):
            raise RuntimeError(f"obs-ffmpeg load failed: {result}")
        if use_plugin:
            mod2 = ptr()
            result = open_module(c.byref(mod2), plugin.as_posix().encode(), project.as_posix().encode())
            if result != 0 or not init_module(mod2):
                raise RuntimeError(f"rtsps plugin load failed: {result}")
            display_name = source_display_name(b"rtsps_secure_source").decode("utf-8")
            if test_discovery and display_name != "RTSPS Secure":
                raise RuntimeError(f"unexpected source name: {display_name}")
        settings = data_create()
        data_set_bool(settings, b"is_local_file", False)
        data_set_bool(settings, b"restart_on_activate", False)
        if use_plugin:
            data_set_string(settings, b"url", url.encode())
            if not test_discovery and not test_discovery_cancel:
                data_set_string(settings, b"certificate", str(certificate).encode())
        else:
            data_set_string(settings, b"input", url.encode())
            data_set_string(settings, b"ffmpeg_options", options.encode())
        source = source_create(b"rtsps_secure_source" if use_plugin else b"ffmpeg_source", b"isolated RTSPS diagnostic", settings)
        data_release(settings)
        if not source:
            raise RuntimeError("source creation failed")
        if test_discovery_cancel:
            props = source_properties(source)
            button = properties_get(props, b"fetch_certificate")
            if not button or not property_click(button, source):
                raise RuntimeError("certificate discovery button failed")
            properties_destroy(props)
            time.sleep(0.2)  # allow the local synthetic TLS handshake to begin
            start = time.monotonic()
            source_release(source)
            wait_destroy()
            elapsed = time.monotonic() - start
            if elapsed > 5:
                raise RuntimeError(f"closing during certificate discovery took {elapsed:.2f}s")
            print(f"TEST: source closed safely during discovery ({elapsed:.2f}s)", flush=True)
            return
        stress_stop = threading.Event()
        stress_threads = []
        if test_stress:
            def query_sizes():
                while not stress_stop.is_set():
                    source_width(source)
                    source_height(source)
                    time.sleep(0.001)
            stress_threads = [threading.Thread(target=query_sizes, daemon=True) for _ in range(6)]
            for thread in stress_threads:
                thread.start()
        try:
            set_output_source(0, source)
            add_video_cb(None, video_cb, None)
            add_audio_cb(0, None, audio_cb, None)
            if test_discovery:
                props = source_properties(source)
                approval = properties_get(props, b"approve_certificate")
                if not approval or property_enabled(approval):
                    raise RuntimeError("approval button enabled before certificate discovery")
                button = properties_get(props, b"fetch_certificate")
                if not button or not property_click(button, source):
                    raise RuntimeError("certificate discovery button failed")
                properties_destroy(props)
                expected = ":".join(f"{b:02X}" for b in hashlib.sha256(ssl.PEM_cert_to_DER_cert(certificate.read_text())).digest())
                for _ in range(12):
                    time.sleep(1)
                    props = source_properties(source)
                    status = property_description(properties_get(props, b"certificate_status")).decode("utf-8")
                    if expected in status:
                        break
                    properties_destroy(props)
                else:
                    raise RuntimeError(f"certificate discovery failed: {status}")
                settings = source_settings(source)
                selected_before = data_get_string(settings, b"certificate")
                data_release(settings)
                if selected_before or source_width(source):
                    raise RuntimeError("certificate was trusted before confirmation")
                print(f"TEST: untrusted fingerprint shown before approval: {expected}", flush=True)
                button = properties_get(props, b"approve_certificate")
                if not button or not property_enabled(button) or not property_click(button, source):
                    raise RuntimeError("certificate confirmation button failed")
                properties_destroy(props)
                settings = source_settings(source)
                saved_path = data_get_string(settings, b"certificate").decode("utf-8")
                data_release(settings)
                saved = ssl.PEM_cert_to_DER_cert(Path(saved_path).read_text())
                if hashlib.sha256(saved).hexdigest() != hashlib.sha256(ssl.PEM_cert_to_DER_cert(certificate.read_text())).hexdigest():
                    raise RuntimeError("approved certificate does not match the observed certificate")
                print(f"TEST: confirmed certificate saved to isolated config: {saved_path}", flush=True)
            before_reconnect = None
            for index in range(test_seconds):
                time.sleep(1)
                print(f"state={source_state(source)}, size={source_width(source)}x{source_height(source)}, stats={stats}", flush=True)
                if test_update and index in (4, 9):
                    new_settings = data_create()
                    new_url = "rtsps://127.0.0.1:1/live" if index == 4 else url
                    data_set_string(new_settings, b"url", new_url.encode())
                    source_update(source, new_settings)
                    data_release(new_settings)
                    if index == 9:
                        before_reconnect = stats["nonblack"]
                    print(f"TEST: updated URL to {new_url}", flush=True)
                if test_mute:
                    if index == 4:
                        source_set_muted(source, True)
                        print("TEST: muted parent source", flush=True)
                    if index == 8 and stats["interval_peak"] > 0.00001:
                        raise RuntimeError("audio remains audible after muting the source")
                    if index == 9:
                        source_set_muted(source, False)
                        print("TEST: unmuted parent source", flush=True)
                    stats["interval_peak"] = 0.0
                if not use_plugin and source_width(source) and source_height(source):
                    break
                ready_after_update = not test_update or (before_reconnect is not None and stats["nonblack"] > before_reconnect and source_width(source) > 0)
                if use_plugin and not expect_failure and not test_stress and index >= (14 if test_mute else 12) and ready_after_update and stats["nonblack"] > 0 and stats["audio_peak"] > 0.00001:
                    break
                if use_plugin and expect_failure and index >= 12:
                    break
            if use_plugin and expect_failure and (source_width(source) or stats["nonblack"]):
                raise RuntimeError("a mismatched certificate was accepted")
            if use_plugin and not expect_failure and (not stats["nonblack"] or stats["audio_peak"] <= 0.00001):
                raise RuntimeError("plugin did not produce observable video and audio")
            if test_update and (before_reconnect is None or stats["nonblack"] <= before_reconnect or not source_width(source)):
                raise RuntimeError("plugin did not reconnect after changing its settings")
        finally:
            stress_stop.set()
            for thread in stress_threads:
                thread.join(timeout=2)
            if any(thread.is_alive() for thread in stress_threads):
                raise RuntimeError("source size queries are stuck")
            remove_video_cb(video_cb, None)
            remove_audio_cb(0, audio_cb, None)
            set_output_source(0, None)
            source_release(source)
    finally:
        shutdown()


if __name__ == "__main__":
    main()
