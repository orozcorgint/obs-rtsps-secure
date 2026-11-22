"""One-connection RTSPS-to-local-RTSP diagnostic; not the production plugin."""

import socket
import ssl
import subprocess
import sys
import threading
import argparse
from pathlib import Path


def copy(src, dst):
    try:
        while data := src.recv(65536):
            dst.sendall(data)
    except (ConnectionError, OSError, ssl.SSLError) as exc:
        print(f"proxy direction closed: {exc}", flush=True)
    finally:
        try:
            dst.shutdown(socket.SHUT_WR)
        except OSError:
            pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("host", help="Server address (provided explicitly)")
    parser.add_argument("certificate", type=Path, help="CA certificate PEM")
    parser.add_argument("--port", type=int, default=8554)
    parser.add_argument("--server-name", required=True, help="TLS hostname from the certificate")
    parser.add_argument("--obs", action="store_true")
    args = parser.parse_args()
    context = ssl.create_default_context(cafile=str(args.certificate.resolve()))
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    port = listener.getsockname()[1]
    url = f"rtsp://127.0.0.1:{port}/live"
    if args.obs:
        cmd = [sys.executable, "tools/diagnose_obs.py", url, "rtsp_transport=tcp"]
    else:
        cmd = ["ffprobe", "-v", "error", "-rtsp_transport", "tcp", "-timeout", "8000000", "-i", url,
               "-show_entries", "stream=codec_name,width,height", "-of", "csv=p=0"]
    print(f"Starting client to {url}", flush=True)
    client = subprocess.Popen(cmd)
    listener.settimeout(15)
    try:
        local, _ = listener.accept()
        with local, socket.create_connection((args.host, args.port), timeout=8) as tcp:
            with context.wrap_socket(tcp, server_hostname=args.server_name) as remote:
                remote.settimeout(None)
                print(f"TLS verified: {remote.version()}, {remote.getpeercert().get('subject')}", flush=True)
                threading.Thread(target=copy, args=(local, remote), daemon=True).start()
                copy(remote, local)
    finally:
        listener.close()
        try:
            client.wait(timeout=20)
        except subprocess.TimeoutExpired:
            client.kill()
            client.wait()
        print(f"Client exit: {client.returncode}", flush=True)


if __name__ == "__main__":
    main()
