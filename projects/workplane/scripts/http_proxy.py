#!/usr/bin/env python3
"""Minimal test-only localhost proxy into Docker's internal M1 network."""

from __future__ import annotations

import os
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


TARGET = os.environ.get("WORKPLANE_PROXY_TARGET", "http://172.30.17.50:8080")
HOP_HEADERS = {"connection", "keep-alive", "proxy-authenticate", "proxy-authorization", "te", "trailers", "transfer-encoding", "upgrade"}


class Proxy(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self) -> None:
        self.forward()

    def do_POST(self) -> None:
        self.forward()

    def forward(self) -> None:
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length) if length else None
        headers = {name: value for name, value in self.headers.items() if name.lower() not in HOP_HEADERS | {"host", "content-length"}}
        request = urllib.request.Request(TARGET + self.path, data=body, headers=headers, method=self.command)
        try:
            response = urllib.request.urlopen(request, timeout=10)
        except urllib.error.HTTPError as error:
            response = error
        payload = response.read()
        self.send_response(response.status)
        for name, value in response.headers.items():
            if name.lower() not in HOP_HEADERS | {"content-length"}:
                self.send_header(name, value)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format: str, *args: object) -> None:
        return


if __name__ == "__main__":
    address = os.environ.get("WORKPLANE_PROXY_ADDR", "127.0.0.1:18080")
    host, port = address.rsplit(":", 1)
    ThreadingHTTPServer((host, int(port)), Proxy).serve_forever()
