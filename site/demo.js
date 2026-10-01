// Interactive parts of the landing page. Everything here is a simulation with example numbers;
// nothing is measured. Motion uses transform/opacity, pauses off-screen and in hidden tabs, and
// is skipped for prefers-reduced-motion.
"use strict";

const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const finePointer = window.matchMedia("(hover: hover) and (pointer: fine)").matches;

/** Runs `step(dt)` on animation frames only while `el` is on screen and the tab is visible. */
function animateWhileVisible(el, step) {
  let onScreen = false, raf = 0, last = 0;
  const loop = (t) => { const dt = Math.min(0.05, (t - (last || t)) / 1000); last = t; step(dt); raf = requestAnimationFrame(loop); };
  const update = () => {
    const run = onScreen && !document.hidden;
    if (run && !raf) { last = 0; raf = requestAnimationFrame(loop); }
    if (!run && raf) { cancelAnimationFrame(raf); raf = 0; }
  };
  new IntersectionObserver(([e]) => { onScreen = e.isIntersecting; update(); }, { threshold: 0.1 }).observe(el);
  document.addEventListener("visibilitychange", update);
}

function fmt(seconds) {
  const s = Math.round(seconds), h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), r = s % 60;
  if (h) return `${h} h ${m} min`;
  if (m) return `${m} min ${r} s`;
  return `${r} s`;
}

/** Animates a number from its previous value: el.textContent = render(value). */
function tween(el, to, render, ms = 500) {
  const from = el._v ?? to; el._v = to;
  if (reduced || from === to) { el.textContent = render(to); return; }
  const t0 = performance.now(), ease = (t) => 1 - Math.pow(1 - t, 3);
  const tick = (t) => { const k = Math.min(1, (t - t0) / ms); el.textContent = render(from + (to - from) * ease(k)); if (k < 1 && el._v === to) requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
}

/** Pointer-smoothed values: call set(target), read on each frame via onFrame(current). */
function smoother(initial, onFrame, k = 0.12) {
  const cur = { ...initial }, target = { ...initial };
  let raf = 0;
  const loop = () => {
    let moving = false;
    for (const key in cur) { cur[key] += (target[key] - cur[key]) * k; if (Math.abs(target[key] - cur[key]) > 0.01) moving = true; }
    onFrame(cur);
    raf = moving ? requestAnimationFrame(loop) : 0;
  };
  return { set(next) { Object.assign(target, next); if (!raf) raf = requestAnimationFrame(loop); }, rest() { this.set(initial); } };
}

/* ---------- Text reveals ---------- */
function letters() {
  if (reduced) return;
  document.querySelectorAll(".letters").forEach((h) => {
    // <br> carries no text, so count it as a space when building the accessible name.
    h.setAttribute("aria-label", [...h.childNodes].map((n) => (n.nodeName === "BR" ? " " : n.textContent)).join("").replace(/\s+/g, " ").trim());
    let i = 0;
    const out = document.createDocumentFragment();
    h.childNodes.forEach((n) => {
      if (n.nodeName === "BR") { out.appendChild(document.createElement("br")); return; }
      n.textContent.split(/(\s+)/).forEach((part) => {
        if (!part) return;
        if (/^\s+$/.test(part)) { out.appendChild(document.createTextNode(" ")); return; }
        const word = document.createElement("span"); word.className = "word"; word.setAttribute("aria-hidden", "true");
        for (const c of part) { const ch = document.createElement("span"); ch.className = "ch"; ch.style.setProperty("--i", i++); ch.textContent = c; word.appendChild(ch); }
        out.appendChild(word);
      });
    });
    h.replaceChildren(out);
  });
}

function fades() {
  const els = [...document.querySelectorAll(".fade")];
  if (reduced || !els.length) return;
  document.documentElement.classList.add("reveal-ready");
  const io = new IntersectionObserver((entries) => entries.forEach((e) => {
    if (!e.isIntersecting) return;
    const delay = e.target.closest(".hero") ? 500 + els.indexOf(e.target) * 120 : 0;
    setTimeout(() => e.target.classList.add("in"), delay);
    io.unobserve(e.target);
  }), { threshold: 0.2 });
  els.forEach((el) => io.observe(el));
}

/* ---------- Magnetic primary buttons (rAF-smoothed, no jitter) ---------- */
function magnetic() {
  if (!finePointer || reduced) return;
  document.querySelectorAll(".magnetic").forEach((b) => {
    const s = smoother({ x: 0, y: 0 }, (c) => { b.style.transform = Math.abs(c.x) + Math.abs(c.y) < 0.05 ? "" : `translate3d(${c.x.toFixed(2)}px, ${c.y.toFixed(2)}px, 0)`; }, 0.16);
    b.addEventListener("pointermove", (e) => { const r = b.getBoundingClientRect(); s.set({ x: ((e.clientX - r.left) / r.width - 0.5) * 14, y: ((e.clientY - r.top) / r.height - 0.5) * 10 }); });
    b.addEventListener("pointerleave", () => s.rest());
  });
}

/* ---------- Calculator ---------- */
function calculator() {
  const $ = (id) => document.getElementById(id);
  if (!$("calc-form")) return;
  const SIZES = [[5, "Linux ISO"], [25, "4K movie"], [80, "Big game"], [150, "Game + updates"]];
  let size = 2;
  const on = { eth: true, wifi: true, cell: true };
  const sliders = { eth: $("in-eth"), wifi: $("in-wifi"), cell: $("in-cell") };
  const fill = (el) => el.style.setProperty("--p", `${((el.value - el.min) / (el.max - el.min)) * 100}%`);
  const update = () => {
    $("size-out").textContent = `${SIZES[size][0]} GB`; $("size-name").textContent = SIZES[size][1];
    $("size-down").disabled = size === 0; $("size-up").disabled = size === SIZES.length - 1;
    const speeds = [];
    for (const [k, el] of Object.entries(sliders)) {
      $(`out-${k}`).textContent = `${el.value} Mbps`; fill(el); el.disabled = !on[k];
      if (on[k]) speeds.push(Number(el.value));
    }
    const bits = SIZES[size][0] * 8000;
    if (!speeds.length) { for (const id of ["t-solo", "t-bond"]) { $(id).textContent = "—"; $(id)._v = undefined; } $("t-saved").textContent = "Switch a connection on"; return; }
    const best = Math.max(...speeds), sum = speeds.reduce((a, b) => a + b, 0) * 0.9;
    const solo = bits / best, bond = bits / Math.max(sum, best);
    tween($("t-solo"), solo, fmt); tween($("t-bond"), bond, fmt);
    const ratio = solo / bond;
    $("t-saved").textContent = ratio > 1.05 ? `${ratio.toFixed(1)}× faster · saves ${fmt(solo - bond)}` : "One link does the work here";
  };
  $("size-down").addEventListener("click", () => { size = Math.max(0, size - 1); update(); });
  $("size-up").addEventListener("click", () => { size = Math.min(SIZES.length - 1, size + 1); update(); });
  document.querySelectorAll(".toggle[data-calc]").forEach((t) => t.addEventListener("click", () => {
    const k = t.dataset.calc; on[k] = !on[k]; t.setAttribute("aria-pressed", String(on[k])); update();
  }));
  $("calc-form").addEventListener("input", update);
  $("calc-form").addEventListener("submit", (e) => e.preventDefault());
  update();
}

/* ---------- Pull the plug: chunk scheduler simulation ---------- */
function plug() {
  const grid = document.getElementById("chunks");
  if (!grid) return;
  const KEYS = ["eth", "wifi", "cell"], N = 192, SLOTS = 2, CHUNK_MB = 4;
  const dur = { eth: 0.4, wifi: 0.6, cell: 1.2 };
  const cells = Array.from({ length: N }, () => grid.appendChild(document.createElement("span")));
  const progressEl = document.getElementById("plug-progress"), stateEl = document.getElementById("plug-state");
  const buttons = [...document.querySelectorAll(".sw[data-link]")];
  let state, on;
  const reset = () => {
    state = { status: new Array(N).fill("todo"), inflight: [], done: 0, recovered: 0, finished: false };
    on = { eth: true, wifi: true, cell: true };
    buttons.forEach((b) => b.setAttribute("aria-pressed", "true"));
    cells.forEach((c) => (c.className = ""));
    render();
  };
  const render = () => {
    progressEl.textContent = `${Math.floor((state.done / N) * 100)}%`;
    for (const k of KEYS) {
      const busy = state.inflight.filter((f) => f.link === k).length;
      document.querySelector(`[data-speed="${k}"]`).textContent = on[k] && busy ? `${Math.round((busy * CHUNK_MB) / dur[k])} MB/s` : on[k] ? "idle" : "off";
    }
    if (state.finished) return;
    if (!on.eth && !on.wifi && !on.cell) stateEl.textContent = "All connections off: paused, nothing lost. Switch one back on.";
    else stateEl.textContent = state.recovered ? `Downloading… ${state.recovered} chunk${state.recovered > 1 ? "s" : ""} rescheduled after a drop` : "Downloading… try switching a connection off";
  };
  const step = (dt) => {
    if (state.finished) return;
    for (const k of KEYS) {
      if (!on[k]) continue;
      while (state.inflight.filter((f) => f.link === k).length < SLOTS) {
        const i = state.status.indexOf("todo");
        if (i < 0) break;
        state.status[i] = "busy"; cells[i].className = `busy-${k}`;
        state.inflight.push({ i, link: k, left: dur[k] * (0.8 + Math.random() * 0.4) });
      }
    }
    for (const f of state.inflight) f.left -= dt;
    for (const f of state.inflight.filter((f) => f.left <= 0)) { state.status[f.i] = "done"; cells[f.i].className = f.link; state.done++; }
    state.inflight = state.inflight.filter((f) => f.left > 0);
    if (state.done === N) { state.finished = true; stateEl.textContent = `Done. ${N} chunks, 0 corrupted${state.recovered ? `, ${state.recovered} rescheduled after drops` : ""}.`; }
    render();
  };
  buttons.forEach((b) => b.addEventListener("click", () => {
    const k = b.dataset.link; on[k] = !on[k]; b.setAttribute("aria-pressed", String(on[k]));
    if (!on[k]) {
      for (const f of state.inflight.filter((f) => f.link === k)) {
        state.status[f.i] = "lost"; cells[f.i].className = "lost"; state.recovered++;
        const i = f.i; setTimeout(() => { if (state.status[i] === "lost") { state.status[i] = "todo"; cells[i].className = ""; } }, 450);
      }
      state.inflight = state.inflight.filter((f) => f.link !== k);
    }
    render();
  }));
  document.getElementById("plug-restart")?.addEventListener("click", reset);
  reset();
  animateWhileVisible(grid, step);
}

/* ---------- Copy buttons ---------- */
function copyButtons() {
  document.querySelectorAll(".copy[data-copy]").forEach((b) => b.addEventListener("click", async () => {
    const text = document.getElementById(b.dataset.copy)?.textContent ?? "";
    try { await navigator.clipboard.writeText(text); b.textContent = "Copied"; b.classList.add("done"); }
    catch { b.textContent = "Select & copy"; }
    setTimeout(() => { b.textContent = "Copy"; b.classList.remove("done"); }, 1600);
  }));
}

/* ---------- Hover tilt for glass surfaces: [data-tilt="max degrees"] ---------- */
function glassTilt() {
  if (!finePointer || reduced) return;
  document.querySelectorAll("[data-tilt]").forEach((el) => {
    const max = Number(el.dataset.tilt) || 3;
    // Big surfaces lift less than small cards.
    const lift = el.offsetWidth > 900 ? -3 : -6;
    const s = smoother({ rx: 0, ry: 0, lift: 0, glare: 0, gx: 50, gy: 50 }, (c) => {
      el.style.setProperty("--rx", `${c.rx.toFixed(3)}deg`);
      el.style.setProperty("--ry", `${c.ry.toFixed(3)}deg`);
      el.style.setProperty("--lift", `${c.lift.toFixed(2)}px`);
      el.style.setProperty("--glare", c.glare.toFixed(3));
      el.style.setProperty("--gx", `${c.gx.toFixed(1)}%`);
      el.style.setProperty("--gy", `${c.gy.toFixed(1)}%`);
    }, 0.09);
    el.addEventListener("pointermove", (e) => {
      const r = el.getBoundingClientRect(), x = (e.clientX - r.left) / r.width, y = (e.clientY - r.top) / r.height;
      s.set({ rx: (0.5 - y) * max * 2, ry: (x - 0.5) * max * 2, lift, glare: 1, gx: x * 100, gy: y * 100 });
    });
    el.addEventListener("pointerleave", () => s.set({ rx: 0, ry: 0, lift: 0, glare: 0 }));
  });
}

/* ---------- Fit the whole hero into the first screen ---------- */
// The hero is one viewport tall; this lowers --fit (which scales everything inside it) until
// the content fits that height and the headline fits its width. If even the smallest readable
// scale does not fit (very short screens, phones in landscape), the facts row is dropped first.
function fitHero() {
  const hero = document.querySelector(".hero");
  const h1 = hero?.querySelector("h1");
  if (!hero || !h1) return;
  const viewH = () => Math.min(window.innerHeight, document.documentElement.clientHeight);
  const fits = () => hero.getBoundingClientRect().height <= viewH() && h1.scrollWidth <= h1.clientWidth + 1;
  const tryFit = (lo, hi) => {
    hero.style.setProperty("--fit", hi);
    if (fits()) return true;
    hero.style.setProperty("--fit", lo);
    if (!fits()) return false;
    for (let i = 0; i < 9; i++) { // binary search for the largest scale that fits
      const mid = (lo + hi) / 2;
      hero.style.setProperty("--fit", mid.toFixed(4));
      if (fits()) lo = mid; else hi = mid;
    }
    hero.style.setProperty("--fit", lo.toFixed(4));
    return true;
  };
  // Fonts have readability floors in CSS, so once those are hit only the compact layout helps.
  const compact = (on) => { hero.classList.toggle("compact", on); document.documentElement.classList.toggle("hero-compact", on); };
  const run = () => {
    compact(false);
    if (tryFit(0.6, 1)) return;
    compact(true);
    if (!tryFit(0.5, 1)) hero.style.setProperty("--fit", "0.5");
  };
  run();
  document.fonts?.ready.then(run); // webfont metrics change line breaks
  // Refit on width changes and real height changes (rotation), not when a mobile browser's
  // toolbar slides away while scrolling, which would make the hero jump.
  let lastW = window.innerWidth, lastH = viewH(), raf = 0;
  window.addEventListener("resize", () => {
    const w = window.innerWidth, h = viewH();
    if (w === lastW && Math.abs(h - lastH) < lastH * 0.15) return;
    lastW = w; lastH = h;
    cancelAnimationFrame(raf); raf = requestAnimationFrame(run);
  });
}

letters();
fitHero();
fades();
glassTilt();
magnetic();
calculator();
plug();
copyButtons();
