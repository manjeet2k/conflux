#!/usr/bin/env node
// Minimal range-capable static file server for Conflux benchmarks. No dependencies.
//
//   node range-server.mjs <file-or-dir> [--port 8080] [--host 0.0.0.0]
//                         [--throttle-mbps 0] [--etag] [--no-ranges]
//
// --throttle-mbps N  cap EACH connection at N megabit/s (0 = unlimited). Per-connection
//                    caps let a LAN server imitate slow links and make bonding gains visible.
// --etag             send a strong ETag and Last-Modified (default on; --no-etag to drop).
// --no-ranges        do not advertise or honour Range (tests the single-stream fallback).
//
// Every request is logged to stdout: time, peer IP, method, path, Range, status, bytes sent.
// The peer IP shows WHICH local adapter address Conflux bound for each connection.
import http from "node:http";
import fs from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const flag = (n) => args.includes(`--${n}`);
const opt = (n, d) => {
  const i = args.indexOf(`--${n}`);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : d;
};
const target = args[0] && !args[0].startsWith("--") ? args[0] : null;
if (!target) {
  console.error("usage: node range-server.mjs <file-or-dir> [--port N] [--host H] [--throttle-mbps N] [--no-etag] [--no-ranges]");
  process.exit(2);
}
const port = Number(opt("port", "8080"));
const host = opt("host", "0.0.0.0");
const mbps = Number(opt("throttle-mbps", "0"));
const sendValidators = !flag("no-etag");
const ranges = !flag("no-ranges");
const root = path.resolve(target);
const isDir = fs.statSync(root).isDirectory();
const bytesPerSec = mbps > 0 ? (mbps * 1e6) / 8 : 0;

function log(req, status, range, sent) {
  const ip = (req.socket.remoteAddress || "?").replace(/^::ffff:/, "");
  console.log(`${new Date().toISOString()} peer=${ip}:${req.socket.remotePort} ${req.method} ${req.url} range=${range || "-"} -> ${status} sent=${sent}`);
}

function resolveFile(urlPath) {
  if (!isDir) return root;
  const rel = path.normalize(decodeURIComponent(urlPath)).replace(/^([/\\])+/, "");
  const full = path.join(root, rel);
  if (!full.startsWith(root + path.sep)) return null; // path traversal
  return full;
}

// Parse a single "bytes=a-b" / "bytes=a-" / "bytes=-n" range. Returns null if absent,
// "bad" if unsatisfiable or malformed.
function parseRange(h, size) {
  if (!h) return null;
  const m = /^bytes=(\d*)-(\d*)$/.exec(h.trim());
  if (!m || (m[1] === "" && m[2] === "")) return "bad";
  let start, end;
  if (m[1] === "") {
    const n = Number(m[2]);
    if (n === 0) return "bad";
    start = Math.max(0, size - n);
    end = size - 1;
  } else {
    start = Number(m[1]);
    end = m[2] === "" ? size - 1 : Math.min(Number(m[2]), size - 1);
  }
  if (start >= size || start > end) return "bad";
  return { start, end };
}

// Stream [start,end] to res, optionally capped at bytesPerSec using 100 ms ticks.
function sendRange(file, start, end, req, res, status, rangeHdr) {
  let sent = 0;
  const stream = fs.createReadStream(file, { start, end, highWaterMark: 64 * 1024 });
  let tickBudget = bytesPerSec / 10;
  let tick = null;
  if (bytesPerSec) {
    tick = setInterval(() => {
      tickBudget = bytesPerSec / 10;
      stream.resume();
    }, 100);
  }
  const done = () => {
    if (tick) clearInterval(tick);
    log(req, status, rangeHdr, sent);
  };
  stream.on("data", (chunk) => {
    sent += chunk.length;
    const ok = res.write(chunk);
    if (bytesPerSec) {
      tickBudget -= chunk.length;
      if (tickBudget <= 0) stream.pause();
    }
    if (!ok) {
      stream.pause();
      res.once("drain", () => {
        if (!bytesPerSec || tickBudget > 0) stream.resume();
      });
    }
  });
  stream.on("end", () => {
    res.end();
    done();
  });
  stream.on("error", () => {
    res.destroy();
    done();
  });
  res.on("close", () => {
    stream.destroy();
    if (tick) clearInterval(tick);
    if (!res.writableEnded) log(req, `${status}(aborted)`, rangeHdr, sent);
  });
}

const server = http.createServer((req, res) => {
  if (req.method !== "GET" && req.method !== "HEAD") {
    res.writeHead(405, { Allow: "GET, HEAD" }).end();
    return log(req, 405, req.headers.range, 0);
  }
  let file;
  try {
    file = resolveFile(new URL(req.url, "http://x").pathname);
  } catch {
    file = null;
  }
  let st;
  try {
    st = file && fs.statSync(file);
  } catch {
    st = null;
  }
  if (!st || !st.isFile()) {
    res.writeHead(404).end();
    return log(req, 404, req.headers.range, 0);
  }
  const size = st.size;
  const headers = {
    "Content-Type": "application/octet-stream",
    "Cache-Control": "no-store",
  };
  if (ranges) headers["Accept-Ranges"] = "bytes";
  if (sendValidators) {
    headers.ETag = `"${size.toString(16)}-${Math.floor(st.mtimeMs).toString(16)}"`;
    headers["Last-Modified"] = st.mtime.toUTCString();
  }
  const rangeHdr = ranges ? req.headers.range : undefined;
  const r = parseRange(rangeHdr, size);
  if (r === "bad") {
    res.writeHead(416, { ...headers, "Content-Range": `bytes */${size}`, "Content-Length": 0 }).end();
    return log(req, 416, rangeHdr, 0);
  }
  // If-Range: only honour the range when the validator matches; otherwise send the whole file.
  const ifRange = req.headers["if-range"];
  const stale = r && ifRange && ifRange !== headers.ETag && ifRange !== headers["Last-Modified"];
  if (r && !stale) {
    headers["Content-Range"] = `bytes ${r.start}-${r.end}/${size}`;
    headers["Content-Length"] = r.end - r.start + 1;
    res.writeHead(206, headers);
    if (req.method === "HEAD") return res.end(), log(req, 206, rangeHdr, 0);
    return sendRange(file, r.start, r.end, req, res, 206, rangeHdr);
  }
  headers["Content-Length"] = size;
  res.writeHead(200, headers);
  if (req.method === "HEAD" || size === 0) return res.end(), log(req, 200, rangeHdr, 0);
  sendRange(file, 0, size - 1, req, res, 200, rangeHdr);
});

server.listen(port, host, () => {
  console.log(`serving ${root} on http://${host}:${port}/  throttle=${mbps || "off"} Mbit/s per connection  ranges=${ranges}  validators=${sendValidators}`);
});
