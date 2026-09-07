"""A byte source and a page that downloads from it, for measuring browsers.

Committed rather than left in a scratch directory, because the numbers it
produces decide how a stitched download is delivered and a measurement nobody
can repeat is an assertion.

Four cases, all driven by the page itself so that no automation framework is
needed. That matters: the reason this exists at all is that Playwright's own
browser builds gave figures thirteen times off from the same browsers as
shipped, so the harness has to be able to drive whatever is installed.

  download  a real `Content-Disposition: attachment`, written by the browser
            to disk with no script involved
  plain     `fetch` and drain in JavaScript
  sw        the same, fetched through a service worker that streams it
  opfs      the same, written into OPFS from an ordinary worker

`delay` throttles the source to one megabyte every `delay` milliseconds. It is
not decoration: unthrottled, four gigabytes arrive in four seconds and a
sampler cannot separate the browser's floor from the download's cost. It is
also what makes a service worker's lifetime observable, which is how Firefox's
thirty-second kill was found.

    python3 server.py <bytes> <delay-ms> <port>
"""

import http.server
import json
import socketserver
import sys
import threading
import time

SIZE = int(sys.argv[1]) if len(sys.argv) > 1 else 512 * 1024 * 1024
DELAY_MS = int(sys.argv[2]) if len(sys.argv) > 2 else 0
PORT = int(sys.argv[3]) if len(sys.argv) > 3 else 8731

RESULT: dict[str, object] = {}
DONE = threading.Event()
CHUNK = 1024 * 1024
PAYLOAD = b"\0" * CHUNK

PAGE = """<!doctype html><meta charset=utf-8><title>t</title>
<body><pre id=out>starting</pre><script>
const kase = new URLSearchParams(location.search).get('case') || 'plain';
const size = %d;
const out = document.getElementById('out');
const say = (t) => { out.textContent = t; };
const report = (r) => {
  // `sendBeacon` is not delivered by every headless build, so the report goes
  // by fetch with a beacon as a second try.
  fetch('/report', { method: 'POST', body: JSON.stringify(r), keepalive: true })
    .catch(() => navigator.sendBeacon('/report', JSON.stringify(r)));
};

async function drain(response) {
  if (!response.ok) throw new Error('answered ' + response.status);
  const reader = response.body.getReader();
  let got = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    got += value.length;
    say(got);
  }
  return got;
}

(async () => {
  const started = performance.now();
  const seconds = () => (performance.now() - started) / 1000;
  try {
    if (kase === 'plain') {
      report({ case: kase, got: await drain(await fetch('/bytes')), expected: size, seconds: seconds() });
    } else if (kase === 'sw') {
      const reg = await navigator.serviceWorker.register('/sw.js');
      await navigator.serviceWorker.ready;
      // `ready` says the worker is active, not that it controls this page.
      // Fetching before it does goes straight to the server, which answers 404
      // and reads back as a download of nothing.
      for (let i = 0; i < 100 && !navigator.serviceWorker.controller; i++) {
        await new Promise((r) => setTimeout(r, 100));
      }
      if (!navigator.serviceWorker.controller) throw new Error('worker never took control');
      report({ case: kase, got: await drain(await fetch('/stitched')), expected: size, seconds: seconds() });
      await reg.unregister();
    } else if (kase === 'opfs') {
      const root = await navigator.storage.getDirectory();
      const file = await root.getFileHandle('t.bin', { create: true });
      const handle = await file.createSyncAccessHandle?.();
      const reader = (await fetch('/bytes')).body.getReader();
      let at = 0;
      if (handle) {
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          // The return value matters: over quota it is 0 or a garbage number
          // and nothing throws.
          const wrote = handle.write(value, { at });
          if (wrote !== value.length) throw new Error('short write ' + wrote);
          at += wrote;
          say(at);
        }
        handle.flush(); handle.close();
      } else {
        const writable = await file.createWritable();
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          await writable.write(value);
          at += value.length;
          say(at);
        }
        await writable.close();
      }
      const onDisk = (await file.getFile()).size;
      report({ case: kase, got: at, on_disk: onDisk, expected: size, seconds: seconds() });
      await root.removeEntry('t.bin');
    }
  } catch (error) {
    report({ case: kase, error: String(error), seconds: seconds() });
  }
})();
</script>
"""

SW = """
self.addEventListener('install', (e) => e.waitUntil(self.skipWaiting()));
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));
self.addEventListener('fetch', (event) => {
  if (!event.request.url.endsWith('/stitched')) return;
  event.respondWith((async () => {
    const upstream = await fetch('/bytes');
    return new Response(upstream.body, {
      headers: { 'content-type': 'application/octet-stream' },
    });
  })());
});
"""


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        # On, because a browser that never loaded the page and one that loaded
        # it and failed look identical from the outside otherwise. That is not
        # hypothetical: two runs were reported as measurements before anyone
        # noticed the browser had requested nothing.
        print("  <-", self.command, self.path, flush=True)

    def _stream(self, attachment: bool) -> int:
        self.send_response(200)
        self.send_header("content-type", "application/octet-stream")
        if attachment:
            self.send_header("content-disposition", 'attachment; filename="t.bin"')
        self.send_header("content-length", str(SIZE))
        self.end_headers()
        sent = 0
        try:
            while sent < SIZE:
                n = min(CHUNK, SIZE - sent)
                self.wfile.write(PAYLOAD[:n])
                sent += n
                # Progress, so that a case which does not finish can be read.
                # "Stalled at zero" and "slower than the budget allowed" look
                # the same from the outside and mean entirely different things:
                # the first is a browser waiting on something, the second is a
                # measurement that needed longer.
                if sent % (256 * CHUNK) == 0:
                    print(f"  sent {sent // CHUNK} MiB", flush=True)
                if DELAY_MS:
                    time.sleep(DELAY_MS / 1000)
        except (BrokenPipeError, ConnectionResetError):
            print(f"  the client went away after {sent // CHUNK} MiB", flush=True)
        return sent

    def _send(self, body: bytes, kind: str) -> None:
        self.send_response(200)
        self.send_header("content-type", kind)
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = self.path.split("?")[0]
        if path == "/":
            self._send((PAGE % SIZE).encode(), "text/html")
        elif path == "/sw.js":
            self._send(SW.encode(), "text/javascript")
        elif path == "/bytes":
            self._stream(attachment=False)
        elif path == "/attachment":
            # No script runs in this case, so the server having sent every byte
            # is the completion signal.
            sent = self._stream(attachment=True)
            RESULT.update({"case": "download", "got": sent, "expected": SIZE})
            DONE.set()
        else:
            self.send_response(404)
            self.send_header("content-length", "0")
            self.end_headers()

    def do_POST(self):
        length = int(self.headers.get("content-length", 0))
        RESULT.update(json.loads(self.rfile.read(length) or b"{}"))
        print("  report:", json.dumps(RESULT), flush=True)
        self.send_response(204)
        self.send_header("content-length", "0")
        self.end_headers()
        DONE.set()


class Threaded(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


if __name__ == "__main__":
    server = Threaded(("127.0.0.1", PORT), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    print(f"listening on {PORT}", flush=True)
    DONE.wait(timeout=600)
    print("RESULT " + json.dumps(RESULT), flush=True)
