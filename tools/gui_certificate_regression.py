"""Exercise OBS's actual Qt property button in an isolated portable copy."""
import ctypes
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import shutil
import ssl
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
PORTABLE = Path(os.environ["LOCALAPPDATA"]) / "Temp" / "opencode" / "obs-rtsps-portable-test"


def run(plugin: Path, url: str, approve: bool = False, expected_cert: Path | None = None, expect_error: bool = False):
    binary = PORTABLE / "bin" / "64bit" / "obs64.exe"
    collection = PORTABLE / "config" / "obs-studio" / "basic" / "scenes" / "Untitled.json"
    if not binary.is_file() or not collection.is_file():
        raise RuntimeError("isolated portable OBS is unavailable")
    shutil.copyfile(plugin, PORTABLE / "plugins" / "rtsps_secure" / "rtsps_secure.dll")
    scene = json.loads(collection.read_text(encoding="utf-8-sig"))
    source = next(s for s in scene["sources"] if s["id"] == "rtsps_secure_source")
    source["settings"]["url"] = url
    source["settings"]["certificate"] = ""
    collection.write_text(json.dumps(scene, ensure_ascii=False, indent=2), encoding="utf-8")
    for marker in (PORTABLE / "config" / "obs-studio" / ".sentinel").glob("run_*"):
        if marker.is_file():
            marker.unlink()  # markers created by this isolated test copy only

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    callback_type = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
    user32.GetWindowThreadProcessId.argtypes = [w.HWND, ctypes.POINTER(w.DWORD)]
    user32.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
    user32.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM,
                                           w.UINT, w.UINT, ctypes.POINTER(ctypes.c_size_t)]
    app = subprocess.Popen([str(binary), "--portable", "--disable-updater"], cwd=binary.parent)

    def windows():
        found = []
        @callback_type
        def visit(handle, _):
            pid = w.DWORD(); user32.GetWindowThreadProcessId(handle, ctypes.byref(pid))
            if pid.value == app.pid and user32.IsWindowVisible(handle):
                title = ctypes.create_unicode_buffer(500)
                user32.GetWindowTextW(handle, title, 500)
                found.append((handle, title.value))
            return True
        user32.EnumWindows(visit, 0)
        return found

    try:
        main = None
        for _ in range(80):
            if app.poll() is not None:
                raise RuntimeError(f"portable OBS exited before the test: {app.returncode}")
            visible = windows()
            if any(title == "Plugin Errors" for _, title in visible):
                raise RuntimeError("unrelated plugin-error dialog in test copy")
            main = next((handle for handle, title in visible if "OBS 33.0.1" in title), None)
            if main:
                break
            time.sleep(0.25)
        if not main:
            raise RuntimeError(f"portable OBS window not found: {windows()}")
        source_name = next(s["name"] for s in scene["sources"] if s["id"] == "rtsps_secure_source")
        command = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                   str(ROOT / "tools" / "gui_certificate_click.ps1"),
                   "-ObsHandle", str(main), "-ObsProcessId", str(app.pid),
                   "-SourceName", source_name,
                   "-Repeat", os.environ.get("OBS_RTSPS_GUI_REPEATS", "1")]
        if approve:
            command.append("-Approve")
        if expect_error:
            command.append("-ExpectError")
        click = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=90)
        text = click.stdout.decode("utf-8", errors="replace")
        print(text, flush=True)
        if click.returncode:
            raise RuntimeError(f"UI automation failed: {click.returncode}; OBS exit={app.poll()}; windows={windows()}")
        if app.poll() is not None:
            raise RuntimeError(f"OBS crashed after clicking the certificate button: {app.returncode}")
        value = ctypes.c_size_t()
        if not user32.SendMessageTimeoutW(main, 0, 0, 0, 2, 3000, ctypes.byref(value)):
            raise RuntimeError("OBS stopped responding after clicking the certificate button")
        print(f"GUI_TEST: windows before closing: {windows()}", flush=True)
        # Close the property dialog, then the main OBS window.
        if not approve:
            for handle, title in windows():
                if title.startswith("Properties for"):
                    user32.PostMessageW(handle, 0x10, 0, 0)
        time.sleep(0.5)
        user32.PostMessageW(main, 0x10, 0, 0)
        try:
            app.wait(timeout=12)
        except subprocess.TimeoutExpired:
            print(f"GUI_TEST: windows after close request: {windows()}", flush=True)
            stack_tool = Path(os.environ["LOCALAPPDATA"]) / "Temp" / "opencode" / "obs-live-stacks.exe"
            if stack_tool.is_file():
                stacks = subprocess.run([str(stack_tool), str(app.pid)], stdout=subprocess.PIPE,
                                        stderr=subprocess.STDOUT, timeout=25).stdout.decode("utf-8", errors="replace")
                for part in stacks.split("\nTHREAD "):
                    if any(x in part for x in ("rtsps_secure", "obs_source_output_audio", "obs_source_get_width", "obs_source_destroy")):
                        print("\nTHREAD " + part[:2500], flush=True)
            raise RuntimeError("OBS did not close after the GUI click test")
        if app.returncode:
            raise RuntimeError(f"OBS exited abnormally after the click test: {app.returncode}")
        if approve and expected_cert:
            saved = json.loads(collection.read_text(encoding="utf-8-sig"))
            saved_source = next(s for s in saved["sources"] if s["id"] == "rtsps_secure_source")
            selected = Path(saved_source["settings"]["certificate"])
            if not selected.is_file():
                raise RuntimeError("approved certificate was not saved to the source")
            if ssl.PEM_cert_to_DER_cert(selected.read_text()) != ssl.PEM_cert_to_DER_cert(expected_cert.read_text()):
                raise RuntimeError("the saved certificate differs from the displayed server certificate")
            print("GUI_TEST: confirmed certificate persisted to source settings", flush=True)
        print("PASS: real OBS Qt certificate button and clean shutdown", flush=True)
    finally:
        if app.poll() is None:
            app.kill()  # this process belongs exclusively to the test
            app.wait()


if __name__ == "__main__":
    chosen_plugin = Path(os.environ.get("OBS_RTSPS_PLUGIN_FILE", str(ROOT / "target" / "x86_64-pc-windows-gnu" / "release" / "rtsps_secure.dll"))).resolve()
    run(chosen_plugin, sys.argv[1] if len(sys.argv) > 1 else "rtsps://127.0.0.1:1/live",
        "--approve" in sys.argv, expect_error="--expect-error" in sys.argv)
