"""End-to-end OBS tests with an entirely local, synthetic video/audio stream.

Requires the official MediaMTX binary in the OpenCode temp directory,
ffmpeg/ffprobe and Python's cryptography package. Opens only loopback ports;
never accesses a real camera or microphone. All launched processes are stopped
on exit. The test certificate and key are newly generated for this test only.
"""
import datetime
import ctypes
from ctypes import wintypes
import ipaddress
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import uuid
import shutil

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID


ROOT = Path(__file__).resolve().parent.parent
TEMP = Path(os.environ["LOCALAPPDATA"]) / "Temp" / "opencode"
MEDIAMTX = TEMP / "mediamtx-v1.21.4-test" / "mediamtx.exe"
PLUGIN = ROOT / "target" / "x86_64-pc-windows-gnu" / "release" / "rtsps_secure.dll"


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()  # only the process started by this test
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def test_portable_gui(secure_url, cert_path):
    root = TEMP / "obs-rtsps-portable-test"
    executable = root / "bin" / "64bit" / "obs64.exe"
    collection = root / "config" / "obs-studio" / "basic" / "scenes" / "Untitled.json"
    if not executable.is_file() or not collection.is_file():
        raise RuntimeError("Isolated portable OBS test copy is missing")
    shutil.copyfile(PLUGIN, root / "plugins" / "rtsps_secure" / "rtsps_secure.dll")
    # The copied installation contains a legacy DroidCam plugin whose missing
    # dependencies cause an unrelated modal error. Disable ONLY the test copy.
    legacy = root / "obs-plugins" / "64bit" / "droidcam-obs.dll"
    disabled = legacy.with_suffix(".dll.disabled-for-test")
    if legacy.exists():
        if disabled.exists():
            raise RuntimeError("Refusing to overwrite the test plugin backup")
        legacy.rename(disabled)
    settings = root / "config" / "obs-studio" / "user.ini"
    if settings.exists():
        text = settings.read_text(encoding="utf-8-sig")
        settings.write_text(text.replace("ConfirmOnExit=true", "ConfirmOnExit=false"), encoding="utf-8")
    for marker in (root / "config" / "obs-studio" / ".sentinel").glob("run_*"):
        if marker.is_file():
            marker.unlink()  # only markers created by our portable test copy
    scene = json.loads(collection.read_text(encoding="utf-8-sig"))
    camera = next(s for s in scene["sources"] if s["id"] == "rtsps_secure_source")
    camera["settings"]["url"] = secure_url
    camera["settings"]["certificate"] = str(cert_path)
    collection.write_text(json.dumps(scene, ensure_ascii=False, indent=2), encoding="utf-8")

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    user32.SendMessageTimeoutW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM,
                                           wintypes.UINT, wintypes.UINT, ctypes.POINTER(ctypes.c_size_t)]
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    app = subprocess.Popen([str(executable), "--portable", "--disable-updater"], cwd=executable.parent)
    main_window = None
    checks = 0

    def windows():
        result = []
        @callback_type
        def visit(hwnd, _):
            pid = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if pid.value == app.pid and user32.IsWindowVisible(hwnd):
                title = ctypes.create_unicode_buffer(500)
                user32.GetWindowTextW(hwnd, title, 500)
                result.append((hwnd, title.value))
            return True
        user32.EnumWindows(visit, 0)
        return result

    try:
        end = time.monotonic() + 35
        while time.monotonic() < end:
            if app.poll() is not None:
                raise RuntimeError(f"Portable OBS exited early: {app.returncode}")
            visible = windows()
            errors = [title for _, title in visible if title == "Plugin Errors"]
            if errors:
                raise RuntimeError("An unrelated plugin-error dialog blocked portable OBS")
            main_window = next((handle for handle, title in visible if "OBS 33.0.1" in title), main_window)
            if main_window:
                # Simulate cursor movement without moving the user's actual pointer.
                for x, y in [(120, 120), (300, 180), (500, 280), (700, 400)]:
                    user32.PostMessageW(main_window, 0x200, 0, (y << 16) | x)
                result = ctypes.c_size_t()
                if not user32.SendMessageTimeoutW(main_window, 0, 0, 0, 2, 2000, ctypes.byref(result)):
                    raise RuntimeError("Portable OBS window stopped responding")
                checks += 1
            time.sleep(0.15)
        if checks < 20:
            raise RuntimeError("Portable OBS window was not observed long enough")
        user32.PostMessageW(main_window, 0x10, 0, 0)  # WM_CLOSE, test process only
        try:
            app.wait(timeout=12)
        except subprocess.TimeoutExpired:
            raise RuntimeError(f"Portable OBS did not close; visible windows: {windows()}")
        if app.returncode != 0:
            raise RuntimeError(f"Portable OBS exited abnormally: {app.returncode}")
        logs = sorted((root / "config" / "obs-studio" / "logs").glob("*.txt"))
        text = logs[-1].read_text(encoding="utf-8", errors="replace")
        if "pinned certificate" not in text or "==== Shutting down" not in text:
            raise RuntimeError("Portable OBS did not connect or shut down cleanly")
        print(f"PASS: portable OBS stayed responsive ({checks} checks) and shut down cleanly", flush=True)
    finally:
        stop(app)


def main():
    if not MEDIAMTX.is_file() or not PLUGIN.is_file():
        raise RuntimeError("MediaMTX test binary or plugin DLL is missing")
    directory = TEMP / f"rtsps-synthetic-{uuid.uuid4().hex}"
    directory.mkdir()
    key_path, cert_path = directory / "test.key", directory / "test.pem"
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost")])
    now = datetime.datetime.now(datetime.timezone.utc)
    cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
            .serial_number(x509.random_serial_number()).not_valid_before(now - datetime.timedelta(minutes=10))
            .not_valid_after(now + datetime.timedelta(days=1))
            .add_extension(x509.SubjectAlternativeName([x509.IPAddress(ipaddress.ip_address("127.0.0.1"))]), False)
            .sign(key, hashes.SHA256()))
    key_path.write_bytes(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.TraditionalOpenSSL,
                                          serialization.NoEncryption()))
    cert_path.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
    other_cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
                  .serial_number(x509.random_serial_number()).not_valid_before(now - datetime.timedelta(minutes=10))
                  .not_valid_after(now + datetime.timedelta(days=1))
                  .add_extension(x509.SubjectAlternativeName([x509.IPAddress(ipaddress.ip_address("127.0.0.1"))]), False)
                  .sign(key, hashes.SHA256()))
    other_cert_path = directory / "different-cert.pem"
    other_cert_path.write_bytes(other_cert.public_bytes(serialization.Encoding.PEM))
    # Reserve two distinct local ports while choosing them.
    sockets = [socket.socket(), socket.socket()]
    for item in sockets:
        item.bind(("127.0.0.1", 0))
    plain, secure = [item.getsockname()[1] for item in sockets]
    for item in sockets:
        item.close()
    config = directory / "mediamtx.yml"
    config.write_text(f"""logLevel: warn
rtsp: yes
rtspTransports: [tcp]
rtspEncryption: optional
rtspAddress: 127.0.0.1:{plain}
rtspsAddress: 127.0.0.1:{secure}
rtspServerKey: {json.dumps(key_path.as_posix())}
rtspServerCert: {json.dumps(cert_path.as_posix())}
rtmp: no
hls: no
webrtc: no
srt: no
moq: no
api: no
metrics: no
pprof: no
playback: no
paths:
  all_others:
""", encoding="utf-8")
    server = publisher = None
    with (directory / "server.log").open("wb") as server_log, (directory / "publisher.log").open("wb") as publisher_log:
        try:
            server = subprocess.Popen([str(MEDIAMTX), str(config)], cwd=directory, stdout=server_log, stderr=subprocess.STDOUT)
            for _ in range(30):
                if server.poll() is not None:
                    raise RuntimeError("Synthetic server exited early; see server.log")
                try:
                    with socket.create_connection(("127.0.0.1", plain), timeout=0.2):
                        break
                except OSError:
                    time.sleep(0.2)
            else:
                raise RuntimeError("Synthetic server did not start")
            source_url = f"rtsp://127.0.0.1:{plain}/live"
            secure_url = f"rtsps://127.0.0.1:{secure}/live"
            publisher = subprocess.Popen([
                "ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-re",
                "-f", "lavfi", "-i", "testsrc=size=800x600:rate=15",
                "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "ultrafast", "-tune", "zerolatency",
                "-g", "15", "-c:a", "aac", "-f", "rtsp", "-rtsp_transport", "tcp", source_url,
            ], cwd=directory, stdout=publisher_log, stderr=subprocess.STDOUT)
            for _ in range(8):
                if publisher.poll() is not None:
                    raise RuntimeError("Synthetic publisher exited; see publisher.log")
                probe = subprocess.run(["ffprobe", "-v", "error", "-rtsp_transport", "tcp", "-timeout", "3000000",
                                        "-show_entries", "stream=codec_name", "-of", "csv=p=0", source_url],
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=8)
                if probe.returncode == 0 and b"h264" in probe.stdout and b"aac" in probe.stdout:
                    break
                time.sleep(0.5)
            else:
                raise RuntimeError("Synthetic stream did not start")
            print(f"Synthetic RTSPS stream available on 127.0.0.1:{secure}; no real camera used", flush=True)
            common = os.environ.copy()
            common.update(OBS_RTSPS_TEST_PLUGIN="1", OBS_RTSPS_PLUGIN_FILE=str(PLUGIN), OBS_RTSPS_CERT_FILE=str(cert_path))
            cases = [
                ("discovery + concurrent OBS/audio stress", {"OBS_RTSPS_TEST_DISCOVERY": "1", "OBS_RTSPS_TEST_STRESS": "1", "OBS_RTSPS_TEST_MUTE": "1", "OBS_RTSPS_TEST_SECONDS": "50"}, 75),
                ("settings change and reconnection", {"OBS_RTSPS_TEST_UPDATE": "1"}, 50),
            ]
            if "--gui-only" in sys.argv or "--gui-cert-only" in sys.argv:
                cases = []
            if "--discovery-only" in sys.argv:
                cases = [("certificate discovery and approval", {"OBS_RTSPS_TEST_DISCOVERY": "1"}, 45)]
            if "--negative-only" in sys.argv:
                cases = [("mismatched certificate rejection", {"OBS_RTSPS_EXPECT_FAILURE": "1", "OBS_RTSPS_CERT_FILE": str(other_cert_path)}, 35)]
            for name, extra, timeout in cases:
                print(f"TEST: {name}", flush=True)
                env = common.copy(); env.update(extra)
                result = subprocess.run([sys.executable, str(ROOT / "tools" / "diagnose_obs.py"), secure_url],
                                        cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout)
                text = result.stdout.decode("utf-8", errors="replace")
                for line in text.splitlines():
                    if "TEST:" in line or "[RTSPS Secure]" in line or "Traceback" in line or "RuntimeError" in line:
                        print(line, flush=True)
                states = [line for line in text.splitlines() if line.startswith("state=")]
                if states:
                    print(states[-1], flush=True)
                if result.returncode:
                    print(text[-4000:], flush=True)
                    raise RuntimeError(f"Test failed: {name} ({result.returncode})")
                if name == "mismatched certificate rejection" and "does not match" not in text:
                    raise RuntimeError("Expected certificate mismatch was not reported")
            if "--gui-cert-only" in sys.argv:
                print("TEST: real OBS GUI certificate fetch and approval", flush=True)
                from gui_certificate_regression import run as run_gui_certificate
                run_gui_certificate(PLUGIN, secure_url, approve=True, expected_cert=cert_path)
            elif "--discovery-only" not in sys.argv and "--negative-only" not in sys.argv:
                print("TEST: portable OBS window and clean shutdown", flush=True)
                test_portable_gui(secure_url, cert_path)
            print(f"PASS: synthetic integration tests; logs: {directory}", flush=True)
        finally:
            stop(publisher)
            stop(server)


if __name__ == "__main__":
    main()
