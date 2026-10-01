// Interactive illustrations for the landing page: the hero packet stream, the lane race, the
// "pull the plug" chunk simulation and the speed calculator. Everything here is a simulation with
// example numbers; nothing is measured. Animations pause off-screen, in hidden tabs, and honour
// prefers-reduced-motion.
"use strict";

const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const LINKS = [
  { key: "eth", name: "Ethernet", color: "#38bdf8", speed: 1.0 },
  { key: "wifi", name: "Wi-Fi 6", color: "#a78bfa", speed: 0.66 },
  { key: "cell", name: "5G tether", color: "#34d399", speed: 0.36 },
];

/** Runs `step(dt)` on animation frames only while `el` is on screen and the tab is visible. */
function animateWhileVisible(el, step) {
  let onScreen = false, raf = 0, last = 0;
  const loop = (t) => {
    const dt = Math.min(0.05, (t - (last || t)) / 1000);
    last = t;
    step(dt);
    raf = requestAnimationFrame(loop);
  };
  const update = () => {
    const run = onScreen && !document.hidden;
    if (run && !raf) { last = 0; raf = requestAnimationFrame(loop); }
    if (!run && raf) { cancelAnimationFrame(raf); raf = 0; }
  };
  new IntersectionObserver(([e]) => { onScreen = e.isIntersecting; update(); }, { threshold: 0.15 }).observe(el);
  document.addEventListener("visibilitychange", update);
}

/* ---------- Hero: packets streaming from three connections into one file ---------- */
function heroStream() {
  const canvas = document.getElementById("stream");
  if (!canvas || !canvas.getContext) return;
  const ctx = canvas.getContext("2d");
  let W = 0, H = 0, dpr = 1;
  let particles = [], fill = 0, doneFor = 0;
  const spawnAcc = [0, 0, 0];

  const resize = () => {
    const r = canvas.getBoundingClientRect();
    dpr = Math.min(2, window.devicePixelRatio || 1);
    W = r.width; H = r.height;
    canvas.width = Math.round(W * dpr); canvas.height = Math.round(H * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  };
  new ResizeObserver(resize).observe(canvas);
  resize();

  const node = (i) => ({ x: 150, y: H * (0.22 + i * 0.28) }); // labels sit to the left of the node
  const file = () => ({ x: W - 120, y: H * 0.5, w: 84, h: 112 });
  const point = (i, t) => {
    const a = node(i), f = file();
    const bx = f.x - 6, by = f.y - 30 + i * 30;
    const c1x = a.x + (bx - a.x) * 0.45, c2x = a.x + (bx - a.x) * 0.6;
    const u = 1 - t;
    return {
      x: u * u * u * a.x + 3 * u * u * t * c1x + 3 * u * t * t * c2x + t * t * t * bx,
      y: u * u * u * a.y + 3 * u * u * t * a.y + 3 * u * t * t * by + t * t * t * by,
    };
  };

  const draw = () => {
    ctx.clearRect(0, 0, W, H);
    const f = file();
    // Paths
    LINKS.forEach((l, i) => {
      ctx.beginPath();
      for (let s = 0; s <= 40; s++) { const p = point(i, s / 40); s ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y); }
      ctx.strokeStyle = l.color + "33"; ctx.lineWidth = 2; ctx.stroke();
    });
    // Packets
    for (const p of particles) {
      const q = point(p.i, p.t), l = LINKS[p.i];
      ctx.shadowColor = l.color; ctx.shadowBlur = 12;
      ctx.fillStyle = l.color;
      ctx.beginPath(); ctx.roundRect(q.x - 6, q.y - 3, 12, 6, 3); ctx.fill();
    }
    ctx.shadowBlur = 0;
    // Adapter nodes
    LINKS.forEach((l, i) => {
      const a = node(i);
      ctx.fillStyle = "#0c1122"; ctx.strokeStyle = l.color; ctx.lineWidth = 2;
      ctx.beginPath(); ctx.arc(a.x, a.y, 22, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
      ctx.fillStyle = l.color; ctx.beginPath(); ctx.arc(a.x, a.y, 7, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = "#eef1fb"; ctx.font = "600 14px system-ui, Segoe UI, sans-serif"; ctx.textAlign = "right";
      ctx.fillText(l.name, a.x - 34, a.y - 2);
      ctx.fillStyle = "#9aa4bf"; ctx.font = "12px ui-monospace, Consolas, monospace";
      ctx.fillText(`${Math.round(l.speed * 94)} MB/s`, a.x - 34, a.y + 15);
    });
    // File being filled bottom-up, coloured in bands
    const x = f.x, y = f.y - f.h / 2;
    ctx.fillStyle = "rgba(255,255,255,0.05)"; ctx.strokeStyle = "rgba(255,255,255,0.25)"; ctx.lineWidth = 1.5;
    ctx.beginPath(); ctx.roundRect(x, y, f.w, f.h, 12); ctx.fill(); ctx.stroke();
    ctx.save(); ctx.beginPath(); ctx.roundRect(x, y, f.w, f.h, 12); ctx.clip();
    const g = ctx.createLinearGradient(x, 0, x + f.w, 0);
    g.addColorStop(0, "#38bdf8"); g.addColorStop(0.5, "#a78bfa"); g.addColorStop(1, "#34d399");
    ctx.fillStyle = g; ctx.globalAlpha = 0.85;
    ctx.fillRect(x, y + f.h * (1 - fill), f.w, f.h * fill);
    ctx.restore(); ctx.globalAlpha = 1;
    ctx.fillStyle = "#eef1fb"; ctx.textAlign = "center"; ctx.font = "800 20px ui-monospace, Consolas, monospace";
    ctx.fillText(doneFor > 0 ? "✓" : `${Math.round(fill * 100)}%`, x + f.w / 2, y + f.h / 2 + 7);
    ctx.fillStyle = "#9aa4bf"; ctx.font = "12px system-ui, Segoe UI, sans-serif";
    ctx.fillText("game.iso", x + f.w / 2, y + f.h + 22);
    const total = LINKS.reduce((s, l) => s + l.speed * 94, 0);
    ctx.fillStyle = "#eef1fb"; ctx.font = "700 15px ui-monospace, Consolas, monospace";
    ctx.fillText(`${Math.round(total)} MB/s`, x + f.w / 2, y - 16);
  };

  const step = (dt) => {
    if (doneFor > 0) { doneFor -= dt; if (doneFor <= 0) { fill = 0; particles = []; } draw(); return; }
    LINKS.forEach((l, i) => {
      spawnAcc[i] += dt * l.speed * 6;
      while (spawnAcc[i] >= 1) { spawnAcc[i] -= 1; particles.push({ i, t: 0 }); }
    });
    for (const p of particles) p.t += dt * (0.35 + LINKS[p.i].speed * 0.25);
    const arrived = particles.filter((p) => p.t >= 1).length;
    particles = particles.filter((p) => p.t < 1);
    fill = Math.min(1, fill + arrived * 0.006);
    if (fill >= 1) doneFor = 1.4;
    draw();
  };

  // Start mid-download (packets already in flight) so the very first frame tells the story;
  // with reduced motion this is the only frame.
  LINKS.forEach((l, i) => { for (let k = 1; k < 7; k++) particles.push({ i, t: k / 7 - i * 0.04 + 0.04 }); });
  fill = reduced ? 0.62 : 0.24;
  draw();
  if (reduced) { new ResizeObserver(draw).observe(canvas); return; }
  animateWhileVisible(canvas, step);
}

/* ---------- Race: one lane vs three ---------- */
function race() {
  const lanes = document.getElementById("lanes");
  if (!lanes) return;
  const solo = lanes.querySelector('[data-race="solo"]');
  const bond = lanes.querySelector('[data-race="bond"]');
  // 80 GB at 300 Mbps vs (300 + 200 + 100) Mbps x 0.9 overhead; the CSS durations use the same ratio.
  const SOLO = (80 * 8000) / 300, BOND = (80 * 8000) / (600 * 0.9);
  const start = () => {
    lanes.classList.remove("go"); void lanes.offsetWidth; lanes.classList.add("go");
    solo.textContent = "…"; bond.textContent = "…";
    setTimeout(() => (bond.textContent = fmt(BOND)), reduced ? 0 : 3333);
    setTimeout(() => (solo.textContent = fmt(SOLO)), reduced ? 0 : 6000);
  };
  let started = false;
  new IntersectionObserver(([e]) => { if (e.isIntersecting && !started) { started = true; start(); } }, { threshold: 0.4 }).observe(lanes);
  document.getElementById("race-replay")?.addEventListener("click", start);
}

/* ---------- Pull the plug: chunk scheduler simulation ---------- */
function plug() {
  const grid = document.getElementById("chunks");
  if (!grid) return;
  const N = 192, SLOTS = 2, CHUNK_MB = 4;
  const dur = { eth: 0.4, wifi: 0.6, cell: 1.2 };
  const cells = Array.from({ length: N }, () => grid.appendChild(document.createElement("span")));
  const progressEl = document.getElementById("plug-progress");
  const stateEl = document.getElementById("plug-state");
  const buttons = [...document.querySelectorAll(".sw[data-link]")];
  let state, on;

  const reset = () => {
    state = { status: new Array(N).fill("todo"), inflight: [], done: 0, recovered: 0, finished: false };
    on = { eth: true, wifi: true, cell: true };
    buttons.forEach((b) => b.setAttribute("aria-pressed", "true"));
    cells.forEach((c) => (c.className = ""));
    render();
  };
  const nextTodo = () => state.status.indexOf("todo");
  const render = () => {
    const pct = Math.floor((state.done / N) * 100);
    progressEl.textContent = `${pct}%`;
    for (const l of LINKS) {
      const busy = state.inflight.filter((f) => f.link === l.key).length;
      document.querySelector(`[data-speed="${l.key}"]`).textContent =
        on[l.key] && busy ? `${Math.round((busy * CHUNK_MB) / dur[l.key])} MB/s` : on[l.key] ? "idle" : "off";
    }
    if (state.finished) return;
    if (!on.eth && !on.wifi && !on.cell) stateEl.textContent = "All connections off: paused, nothing lost. Switch one back on.";
    else stateEl.textContent = state.recovered
      ? `Downloading… ${state.recovered} chunk${state.recovered > 1 ? "s" : ""} rescheduled after a drop`
      : "Downloading… try switching a connection off";
  };
  const step = (dt) => {
    if (state.finished) return;
    // Fill free slots on every active link.
    for (const l of LINKS) {
      if (!on[l.key]) continue;
      while (state.inflight.filter((f) => f.link === l.key).length < SLOTS) {
        const i = nextTodo();
        if (i < 0) break;
        state.status[i] = "busy";
        cells[i].className = `busy-${l.key}`;
        state.inflight.push({ i, link: l.key, left: dur[l.key] * (0.8 + Math.random() * 0.4) });
      }
    }
    for (const f of state.inflight) f.left -= dt;
    for (const f of state.inflight.filter((f) => f.left <= 0)) {
      state.status[f.i] = "done"; cells[f.i].className = f.link; state.done++;
    }
    state.inflight = state.inflight.filter((f) => f.left > 0);
    if (state.done === N) {
      state.finished = true;
      stateEl.textContent = `Done. ${N} chunks, 0 corrupted${state.recovered ? `, ${state.recovered} rescheduled after drops` : ""}.`;
    }
    render();
  };
  buttons.forEach((b) => b.addEventListener("click", () => {
    const key = b.dataset.link;
    on[key] = !on[key];
    b.setAttribute("aria-pressed", String(on[key]));
    if (!on[key]) {
      // In-flight chunks on a dropped link go back to the queue, flashing red first.
      for (const f of state.inflight.filter((f) => f.link === key)) {
        state.status[f.i] = "lost"; cells[f.i].className = "lost"; state.recovered++;
        const i = f.i;
        setTimeout(() => { if (state.status[i] === "lost") { state.status[i] = "todo"; cells[i].className = ""; } }, 450);
      }
      state.inflight = state.inflight.filter((f) => f.link !== key);
    }
    render();
  }));
  document.getElementById("plug-restart")?.addEventListener("click", reset);
  reset();
  animateWhileVisible(grid, step);
}

/* ---------- Speed calculator ---------- */
function calculator() {
  const $ = (id) => document.getElementById(id);
  if (!$("calc-form")) return;
  const inputs = ["eth", "wifi", "cell"].map((k) => [k, $(`in-${k}`), $(`out-${k}`)]);
  const update = () => {
    const gb = Number($("calc-size").value);
    const speeds = inputs.map(([, input, out]) => { out.textContent = `${input.value} Mbps`; return Number(input.value); });
    const best = Math.max(...speeds), sum = speeds.reduce((a, b) => a + b, 0) * 0.9;
    const bits = gb * 8000; // GB -> megabits
    if (!best) { $("t-solo").textContent = $("t-bond").textContent = "—"; $("t-saved").textContent = "Turn at least one connection up."; return; }
    const solo = bits / best, bond = bits / Math.max(sum, best);
    $("t-solo").textContent = fmt(solo);
    $("t-bond").textContent = fmt(bond);
    const ratio = solo / bond;
    $("t-saved").textContent = ratio > 1.05 ? `${ratio.toFixed(1)}× faster · ${fmt(solo - bond)} saved` : "One connection does most of the work here.";
  };
  $("calc-form").addEventListener("input", update);
  $("calc-form").addEventListener("submit", (e) => e.preventDefault());
  update();
}

function fmt(seconds) {
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), r = s % 60;
  if (h) return `${h} h ${m} min`;
  if (m) return `${m} min ${r} s`;
  return `${r} s`;
}

heroStream();
race();
plug();
calculator();
