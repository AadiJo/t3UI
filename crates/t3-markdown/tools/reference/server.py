# Serves /tmp/markdown-reference like `python3 -m http.server`, plus PUT for
# top-level *.json files so the page can save its own layout metrics.
# Usage: python3 /tmp/markdown-reference/server.py <port>
import http.server
import os
import re
import sys

ROOT = os.environ.get("T3_MD_REF_OUT", "/tmp/markdown-reference")


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=ROOT, **kwargs)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def do_PUT(self):
        name = self.path.lstrip("/")
        if not re.fullmatch(r"[\w.-]+\.json", name):
            self.send_error(403)
            return
        length = int(self.headers.get("Content-Length", "0"))
        with open(os.path.join(ROOT, name), "wb") as f:
            f.write(self.rfile.read(length))
        self.send_response(204)
        self.end_headers()


http.server.ThreadingHTTPServer(("0.0.0.0", int(sys.argv[1])), Handler).serve_forever()
