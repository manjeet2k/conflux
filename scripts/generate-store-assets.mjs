#!/usr/bin/env node
/**
 * Conflux Chrome Web Store & Edge Add-ons Asset Generator
 *
 * Generates pixel-perfect store assets according to Google Chrome Web Store specifications:
 * - Store Icon (128x128)
 * - Promo Tile Small (440x280)
 * - Screenshots 1-4 (1280x800)
 * - Marquee Tile Large (1400x560)
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const OUT_DIR = path.join(ROOT, "assets", "store");
const WIN_DOWNLOADS_DIR = "/mnt/c/Users/pc/Downloads/conflux-store-assets";

fs.mkdirSync(OUT_DIR, { recursive: true });
if (fs.existsSync("/mnt/c/Users/pc/Downloads")) {
  fs.mkdirSync(WIN_DOWNLOADS_DIR, { recursive: true });
}

// Dynamically resolve resvg from /tmp/asset-gen or local
let Resvg;
try {
  const mod = await import("/tmp/asset-gen/node_modules/@resvg/resvg-js/index.js");
  Resvg = mod.Resvg;
} catch {
  const mod = await import("@resvg/resvg-js");
  Resvg = mod.Resvg;
}

const icon128B64 = fs.readFileSync(path.join(ROOT, "extensions/conflux-browser/icons/icon-128.png")).toString("base64");

// -------------------------------------------------------------
// 1. Promo Tile Small: 440 x 280
// -------------------------------------------------------------
const promoSmallSvg = `
<svg xmlns="http://www.w3.org/2000/svg" width="440" height="280" viewBox="0 0 440 280">
  <defs>
    <radialGradient id="bgGrad" cx="0.8" cy="0.9" r="0.8">
      <stop offset="0%" stop-color="#5c31ff" stop-opacity="0.85"/>
      <stop offset="50%" stop-color="#231053" stop-opacity="0.95"/>
      <stop offset="100%" stop-color="#0a051d" stop-opacity="1"/>
    </radialGradient>
    <radialGradient id="pinkGlow" cx="0.2" cy="0.1" r="0.6">
      <stop offset="0%" stop-color="#f88cd4" stop-opacity="0.35"/>
      <stop offset="100%" stop-color="#f88cd4" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="cardGrad" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.14"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="0.03"/>
    </linearGradient>
    <linearGradient id="speedGrad" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0%" stop-color="#5c31ff"/>
      <stop offset="50%" stop-color="#f88cd4"/>
      <stop offset="100%" stop-color="#def141"/>
    </linearGradient>
  </defs>

  <!-- Background -->
  <rect width="440" height="280" fill="url(#bgGrad)"/>
  <rect width="440" height="280" fill="url(#pinkGlow)"/>

  <!-- Subtle grid lines -->
  <path d="M0 70 H440 M0 140 H440 M0 210 H440 M110 0 V280 M220 0 V280 M330 0 V280" stroke="#ffffff" stroke-opacity="0.04" stroke-width="1"/>

  <!-- Branding Top Left -->
  <g transform="translate(28, 28)">
    <rect width="36" height="36" rx="8" fill="#181136" stroke="#ffffff" stroke-opacity="0.15"/>
    <image x="4" y="4" width="28" height="28" href="data:image/png;base64,${icon128B64}"/>
    <text x="46" y="24" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="19" fill="#ffffff" letter-spacing="1.5">CONFLUX</text>
  </g>

  <!-- Title & Subtitle -->
  <text x="28" y="105" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="27" fill="#ffffff" letter-spacing="-0.5">Download Accelerator</text>
  <text x="28" y="132" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="13" fill="#cbd5e1" letter-spacing="0.2">Multi-Interface Bandwidth Aggregation</text>

  <!-- Feature Pills -->
  <g transform="translate(28, 152)">
    <!-- Ethernet Pill -->
    <rect x="0" y="0" width="105" height="24" rx="12" fill="#5c31ff" fill-opacity="0.35" stroke="#5c31ff" stroke-opacity="0.8"/>
    <circle cx="12" cy="12" r="3.5" fill="#a78bfa"/>
    <text x="22" y="16" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="10.5" fill="#f5f3ff" letter-spacing="0.5">ETHERNET</text>

    <!-- Wi-Fi Pill -->
    <rect x="113" y="0" width="85" height="24" rx="12" fill="#f88cd4" fill-opacity="0.25" stroke="#f88cd4" stroke-opacity="0.7"/>
    <circle cx="125" cy="12" r="3.5" fill="#f472b6"/>
    <text x="135" y="16" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="10.5" fill="#fdf2f8" letter-spacing="0.5">WI-FI 6</text>

    <!-- 5G Tethering Pill -->
    <rect x="206" y="0" width="80" height="24" rx="12" fill="#def141" fill-opacity="0.2" stroke="#def141" stroke-opacity="0.7"/>
    <circle cx="218" cy="12" r="3.5" fill="#def141"/>
    <text x="228" y="16" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="10.5" fill="#fefce8" letter-spacing="0.5">USB 5G</text>
  </g>

  <!-- Speed Card Bottom -->
  <g transform="translate(28, 194)">
    <rect width="384" height="62" rx="12" fill="url(#cardGrad)" stroke="#ffffff" stroke-opacity="0.18"/>
    <!-- Left Speed Metric -->
    <text x="18" y="38" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="28" fill="#ffffff" letter-spacing="-1">193.4</text>
    <text x="100" y="27" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="11" fill="#f88cd4" letter-spacing="1">MB/S</text>
    <text x="100" y="42" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="10" fill="#94a3b8">BONDED SPEED</text>

    <!-- Right Side Badges -->
    <rect x="238" y="14" width="130" height="34" rx="8" fill="#181136" stroke="#ffffff" stroke-opacity="0.12"/>
    <text x="303" y="35" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="11" fill="#38bdf8">1-CLICK BROWSER</text>
  </g>
</svg>
`;

// -------------------------------------------------------------
// 2. Screenshot 1: Extension Popup & Instant Handoff (1280x800)
// -------------------------------------------------------------
const screenshot1Svg = `
<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="800" viewBox="0 0 1280 800">
  <defs>
    <radialGradient id="s1Bg" cx="0.8" cy="0.1" r="0.9">
      <stop offset="0%" stop-color="#4318c4" stop-opacity="0.5"/>
      <stop offset="60%" stop-color="#140a33" stop-opacity="0.95"/>
      <stop offset="100%" stop-color="#0a051d" stop-opacity="1"/>
    </radialGradient>
    <filter id="shadow" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="16" stdDeviation="24" flood-color="#000000" flood-opacity="0.6"/>
    </filter>
  </defs>

  <rect width="1280" height="800" fill="url(#s1Bg)"/>

  <!-- Top Hero Header -->
  <text x="640" y="80" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="34" fill="#ffffff" letter-spacing="-0.5">Seamless One-Click Browser Integration</text>
  <text x="640" y="115" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="17" fill="#cbd5e1">Automatically captures browser downloads and routes them to Conflux Desktop</text>

  <!-- Browser Window Mockup -->
  <g transform="translate(140, 160)" filter="url(#shadow)">
    <!-- Browser Window Frame -->
    <rect width="1000" height="580" rx="14" fill="#1e1e24" stroke="#ffffff" stroke-opacity="0.14" stroke-width="1"/>
    
    <!-- Titlebar / Tabs -->
    <path d="M0 14 Q0 0 14 0 H986 Q1000 0 1000 14 V44 H0 Z" fill="#141418"/>
    <!-- Window controls -->
    <circle cx="24" cy="22" r="6" fill="#ff5f56"/>
    <circle cx="44" cy="22" r="6" fill="#ffbd2e"/>
    <circle cx="64" cy="22" r="6" fill="#27c93f"/>

    <!-- Active Tab -->
    <path d="M100 44 L114 12 H290 L304 44 Z" fill="#1e1e24"/>
    <text x="145" y="32" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#ffffff">Ubuntu 24.04 LTS Download</text>

    <!-- Navigation / Omnibox Bar -->
    <rect x="0" y="44" width="1000" height="46" fill="#1a1a20" stroke="#ffffff" stroke-opacity="0.08" stroke-width="1"/>
    <!-- Back/Forward/Reload icons -->
    <path d="M22 67 H38 M28 61 L22 67 L28 73" stroke="#94a3b8" stroke-width="1.8" fill="none"/>
    <path d="M52 67 H68 M62 61 L68 67 L62 73" stroke="#475569" stroke-width="1.8" fill="none"/>
    <path d="M92 62 A6 6 0 1 1 86 68" stroke="#94a3b8" stroke-width="1.8" fill="none"/>
    
    <!-- URL Omnibox -->
    <rect x="120" y="52" width="760" height="30" rx="15" fill="#101014" stroke="#ffffff" stroke-opacity="0.1"/>
    <text x="145" y="72" font-family="'Segoe UI', system-ui, sans-serif" font-size="13" fill="#cbd5e1">https://releases.ubuntu.com/24.04/ubuntu-24.04.1-desktop-amd64.iso</text>

    <!-- Extension Icon in Toolbar -->
    <rect x="910" y="52" width="30" height="30" rx="6" fill="#5c31ff" fill-opacity="0.3" stroke="#5c31ff" stroke-opacity="0.6"/>
    <image x="915" y="57" width="20" height="20" href="data:image/png;base64,${icon128B64}"/>

    <!-- Page Content Background (Subtle) -->
    <rect x="40" y="120" width="560" height="420" rx="10" fill="#121217" stroke="#ffffff" stroke-opacity="0.06"/>
    <text x="70" y="165" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="22" fill="#ffffff">Ubuntu 24.04.1 LTS (Noble Numbat)</text>
    <text x="70" y="195" font-family="'Segoe UI', system-ui, sans-serif" font-size="14" fill="#94a3b8">Desktop image · 5.8 GB · 64-bit PC (AMD64)</text>
    
    <rect x="70" y="225" width="240" height="44" rx="8" fill="#e95420"/>
    <text x="190" y="252" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="14" fill="#ffffff">Download ISO (5.8 GB)</text>

    <!-- Conflux Extension Popup (Anchored from toolbar icon) -->
    <g transform="translate(640, 100)" filter="url(#shadow)">
      <rect width="320" height="420" rx="12" fill="#202026" stroke="#ffffff" stroke-opacity="0.22" stroke-width="1.2"/>
      
      <!-- Popup Header -->
      <g transform="translate(20, 20)">
        <image x="0" y="0" width="26" height="26" href="data:image/png;base64,${icon128B64}"/>
        <text x="36" y="18" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="16" fill="#ffffff">Conflux</text>
        
        <!-- Status Badge -->
        <rect x="180" y="0" width="96" height="24" rx="12" fill="#107c10" fill-opacity="0.2" stroke="#107c10" stroke-opacity="0.5"/>
        <circle cx="192" cy="12" r="3.5" fill="#54b054"/>
        <text x="202" y="16" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="11" fill="#86efac">Connected</text>
      </g>

      <!-- Auto-Intercept Card -->
      <g transform="translate(20, 70)">
        <rect width="280" height="90" rx="10" fill="#2b2b34" stroke="#ffffff" stroke-opacity="0.12"/>
        <text x="16" y="28" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="14" fill="#ffffff">Auto-Intercept</text>
        <text x="16" y="48" font-family="'Segoe UI', system-ui, sans-serif" font-size="11.5" fill="#94a3b8">Hand off matching browser</text>
        <text x="16" y="66" font-family="'Segoe UI', system-ui, sans-serif" font-size="11.5" fill="#94a3b8">downloads to Conflux Desktop</text>

        <!-- Toggle Switch ON -->
        <rect x="220" y="20" width="44" height="24" rx="12" fill="#5c31ff"/>
        <circle cx="250" cy="32" r="9" fill="#ffffff"/>
      </g>

      <!-- Connection Status Card -->
      <g transform="translate(20, 175)">
        <rect width="280" height="80" rx="10" fill="#2b2b34" stroke="#ffffff" stroke-opacity="0.12"/>
        <text x="16" y="26" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="13" fill="#ffffff">Native Bridge Active</text>
        <text x="16" y="46" font-family="'Segoe UI', system-ui, sans-serif" font-size="11" fill="#38bdf8">com.conflux.desktop</text>
        <text x="16" y="64" font-family="'Segoe UI', system-ui, sans-serif" font-size="11" fill="#94a3b8">Latency: &lt; 1 ms (stdio pipe)</text>
      </g>

      <!-- Open Conflux Desktop Button -->
      <rect x="20" y="275" width="280" height="42" rx="8" fill="#5c31ff"/>
      <text x="160" y="301" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="13.5" fill="#ffffff">Open Conflux Desktop</text>

      <!-- Footer -->
      <text x="20" y="380" font-family="'Segoe UI', system-ui, sans-serif" font-size="11.5" fill="#60cdff">Extension Settings</text>
      <text x="280" y="380" text-anchor="end" font-family="'Segoe UI', system-ui, sans-serif" font-size="11.5" fill="#94a3b8">v0.2.0</text>
    </g>
  </g>
</svg>
`;

// -------------------------------------------------------------
// 3. Screenshot 2: Context Menu Download (1280x800)
// -------------------------------------------------------------
const screenshot2Svg = `
<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="800" viewBox="0 0 1280 800">
  <defs>
    <radialGradient id="s2Bg" cx="0.2" cy="0.1" r="0.9">
      <stop offset="0%" stop-color="#c026d3" stop-opacity="0.4"/>
      <stop offset="60%" stop-color="#140a33" stop-opacity="0.95"/>
      <stop offset="100%" stop-color="#0a051d" stop-opacity="1"/>
    </radialGradient>
    <filter id="shadow2" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="16" stdDeviation="24" flood-color="#000000" flood-opacity="0.65"/>
    </filter>
  </defs>

  <rect width="1280" height="800" fill="url(#s2Bg)"/>

  <!-- Top Hero Header -->
  <text x="640" y="80" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="34" fill="#ffffff" letter-spacing="-0.5">Right-Click Download Acceleration</text>
  <text x="640" y="115" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="17" fill="#cbd5e1">Hands links and files straight to the Conflux desktop accelerator</text>

  <!-- Browser Window Mockup -->
  <g transform="translate(140, 160)" filter="url(#shadow2)">
    <rect width="1000" height="580" rx="14" fill="#1e1e24" stroke="#ffffff" stroke-opacity="0.14" stroke-width="1"/>
    
    <!-- Titlebar -->
    <path d="M0 14 Q0 0 14 0 H986 Q1000 0 1000 14 V44 H0 Z" fill="#141418"/>
    <circle cx="24" cy="22" r="6" fill="#ff5f56"/>
    <circle cx="44" cy="22" r="6" fill="#ffbd2e"/>
    <circle cx="64" cy="22" r="6" fill="#27c93f"/>

    <rect x="0" y="44" width="1000" height="46" fill="#1a1a20" stroke="#ffffff" stroke-opacity="0.08" stroke-width="1"/>
    <rect x="120" y="52" width="760" height="30" rx="15" fill="#101014" stroke="#ffffff" stroke-opacity="0.1"/>
    <text x="145" y="72" font-family="'Segoe UI', system-ui, sans-serif" font-size="13" fill="#cbd5e1">https://drive.google.com/uc?export=download&amp;id=1A2b3C4D_LargeDataset.zip</text>

    <!-- Webpage Content -->
    <rect x="60" y="120" width="880" height="420" rx="10" fill="#14141a" stroke="#ffffff" stroke-opacity="0.06"/>
    <text x="100" y="170" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="24" fill="#ffffff">Google Drive — Large File Confirmation</text>
    <text x="100" y="202" font-family="'Segoe UI', system-ui, sans-serif" font-size="14" fill="#94a3b8">DeepLearning_Dataset_2026.zip (14.2 GB) is too large for Google to scan for viruses.</text>
    
    <!-- Download Link / Button -->
    <rect x="100" y="235" width="220" height="42" rx="6" fill="#2563eb"/>
    <text x="210" y="261" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="14" fill="#ffffff">Download anyway</text>

    <!-- Cursor Pointer -->
    <path d="M260 270 L280 290 L273 294 L282 312 L275 315 L266 297 L258 304 Z" fill="#ffffff" stroke="#000000" stroke-width="1.5"/>

    <!-- Context Menu Displayed Right at Cursor -->
    <g transform="translate(275, 260)" filter="url(#shadow2)">
      <rect width="250" height="230" rx="8" fill="#23232b" stroke="#ffffff" stroke-opacity="0.22" stroke-width="1.2"/>
      
      <text x="18" y="28" font-family="'Segoe UI', system-ui, sans-serif" font-size="13" fill="#cbd5e1">Open link in new tab</text>
      <text x="18" y="58" font-family="'Segoe UI', system-ui, sans-serif" font-size="13" fill="#cbd5e1">Open link in new window</text>
      <text x="18" y="88" font-family="'Segoe UI', system-ui, sans-serif" font-size="13" fill="#cbd5e1">Save link as...</text>
      <text x="18" y="118" font-family="'Segoe UI', system-ui, sans-serif" font-size="13" fill="#cbd5e1">Copy link address</text>
      
      <line x1="12" y1="134" x2="238" y2="134" stroke="#ffffff" stroke-opacity="0.12"/>

      <!-- Highlighted Conflux Action Item -->
      <rect x="6" y="144" width="238" height="38" rx="6" fill="#5c31ff"/>
      <image x="16" y="152" width="20" height="20" href="data:image/png;base64,${icon128B64}"/>
      <text x="46" y="168" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="13.5" fill="#ffffff">Download with Conflux</text>

      <line x1="12" y1="192" x2="238" y2="192" stroke="#ffffff" stroke-opacity="0.12"/>
      <text x="18" y="214" font-family="'Segoe UI', system-ui, sans-serif" font-size="13" fill="#cbd5e1">Inspect</text>
    </g>

    <!-- Callout Annotation Pill -->
    <g transform="translate(560, 310)">
      <rect width="340" height="110" rx="10" fill="#181136" stroke="#f88cd4" stroke-opacity="0.6" stroke-width="1.5"/>
      <text x="20" y="32" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="14" fill="#f88cd4">Zero Website Access</text>
      <text x="20" y="56" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#e2e8f0">No host permissions, no cookies,</text>
      <text x="20" y="74" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#e2e8f0">and no tab access: only the link,</text>
      <text x="20" y="94" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#38bdf8">referrer and User-Agent go to Conflux.</text>
    </g>
  </g>
</svg>
`;

// -------------------------------------------------------------
// 4. Screenshot 3: Multi-Interface Speed Aggregation (1280x800)
// -------------------------------------------------------------
const screenshot3Svg = `
<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="800" viewBox="0 0 1280 800">
  <defs>
    <radialGradient id="s3Bg" cx="0.5" cy="0.1" r="0.9">
      <stop offset="0%" stop-color="#1e1b4b" stop-opacity="0.8"/>
      <stop offset="60%" stop-color="#0f0c29" stop-opacity="0.95"/>
      <stop offset="100%" stop-color="#050314" stop-opacity="1"/>
    </radialGradient>
    <linearGradient id="ethGrad" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0%" stop-color="#5c31ff"/>
      <stop offset="100%" stop-color="#8b5cf6"/>
    </linearGradient>
    <linearGradient id="wifiGrad" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0%" stop-color="#ec4899"/>
      <stop offset="100%" stop-color="#f472b6"/>
    </linearGradient>
    <linearGradient id="celGrad" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0%" stop-color="#84cc16"/>
      <stop offset="100%" stop-color="#def141"/>
    </linearGradient>
  </defs>

  <rect width="1280" height="800" fill="url(#s3Bg)"/>

  <!-- Top Hero Header -->
  <text x="640" y="80" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="34" fill="#ffffff" letter-spacing="-0.5">Multi-Interface Bandwidth Aggregation</text>
  <text x="640" y="115" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="17" fill="#cbd5e1">Bonds Wi-Fi, Ethernet, and Mobile Tethering into one high-performance stream</text>

  <!-- Conflux Desktop Window -->
  <g transform="translate(100, 160)" filter="url(#shadow)">
    <rect width="1080" height="580" rx="14" fill="#181328" stroke="#ffffff" stroke-opacity="0.16" stroke-width="1.2"/>
    
    <!-- Titlebar -->
    <path d="M0 14 Q0 0 14 0 H1066 Q1080 0 1080 14 V44 H0 Z" fill="#120e20"/>
    <image x="18" y="12" width="20" height="20" href="data:image/png;base64,${icon128B64}"/>
    <text x="46" y="27" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="13" fill="#ffffff">Conflux Desktop — Multi-Network Download Accelerator</text>
    
    <!-- Window controls -->
    <rect x="990" y="16" width="12" height="2" fill="#94a3b8"/>
    <rect x="1022" y="12" width="10" height="10" stroke="#94a3b8" stroke-width="1.5" fill="none"/>
    <path d="M1054 12 L1064 22 M1064 12 L1054 22" stroke="#94a3b8" stroke-width="1.5"/>

    <!-- Active Task Card -->
    <g transform="translate(40, 70)">
      <rect width="1000" height="180" rx="12" fill="#201a35" stroke="#ffffff" stroke-opacity="0.12"/>
      
      <!-- File Name & Meta -->
      <text x="28" y="38" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="20" fill="#ffffff">ubuntu-24.04.1-desktop-amd64.iso</text>
      <text x="28" y="62" font-family="'Segoe UI', system-ui, sans-serif" font-size="13.5" fill="#94a3b8">3.94 GB of 5.80 GB (68%) · 8 Range Workers Active · Sparse File Allocated</text>
      
      <!-- Right Speed Metric -->
      <text x="960" y="44" text-anchor="end" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="32" fill="#ffffff" letter-spacing="-1">194.2 <tspan font-size="16" fill="#f88cd4">MB/s</tspan></text>
      <text x="960" y="66" text-anchor="end" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#86efac">Time Remaining: 9s</text>

      <!-- Multi-Color Progress Bar (Divided by interface contribution) -->
      <g transform="translate(28, 86)">
        <rect width="944" height="14" rx="7" fill="#ffffff" fill-opacity="0.08"/>
        <!-- Ethernet 60% of completed -->
        <rect width="386" height="14" rx="7" fill="url(#ethGrad)"/>
        <!-- Wi-Fi 30% of completed -->
        <rect x="386" width="193" height="14" fill="url(#wifiGrad)"/>
        <!-- 5G 10% of completed -->
        <rect x="579" width="63" height="14" rx="7" fill="url(#celGrad)"/>
      </g>

      <!-- Chunk Visualization Map -->
      <g transform="translate(28, 120)">
        <text x="0" y="14" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="11" fill="#94a3b8">ACTIVE CHUNK SLICES [64 MB EACH]:</text>
        <g transform="translate(0, 24)">
          <!-- 32 chunk blocks -->
          <rect x="0" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="30" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="60" width="26" height="18" rx="3" fill="#ec4899"/>
          <rect x="90" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="120" width="26" height="18" rx="3" fill="#84cc16"/>
          <rect x="150" width="26" height="18" rx="3" fill="#ec4899"/>
          <rect x="180" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="210" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="240" width="26" height="18" rx="3" fill="#ec4899"/>
          <rect x="270" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="300" width="26" height="18" rx="3" fill="#84cc16"/>
          <rect x="330" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="360" width="26" height="18" rx="3" fill="#ec4899"/>
          <rect x="390" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="420" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="450" width="26" height="18" rx="3" fill="#84cc16"/>
          <rect x="480" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="510" width="26" height="18" rx="3" fill="#ec4899"/>
          <rect x="540" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="570" width="26" height="18" rx="3" fill="#5c31ff"/>
          <rect x="600" width="26" height="18" rx="3" fill="#ec4899"/>
          <rect x="630" width="26" height="18" rx="3" fill="#def141" stroke="#ffffff" stroke-width="1.5"/>
          <rect x="660" width="26" height="18" rx="3" fill="#ec4899" stroke="#ffffff" stroke-width="1.5"/>
          <rect x="690" width="26" height="18" rx="3" fill="#5c31ff" stroke="#ffffff" stroke-width="1.5"/>
          <rect x="720" width="26" height="18" rx="3" fill="#ffffff" fill-opacity="0.1"/>
          <rect x="750" width="26" height="18" rx="3" fill="#ffffff" fill-opacity="0.1"/>
          <rect x="780" width="26" height="18" rx="3" fill="#ffffff" fill-opacity="0.1"/>
          <rect x="810" width="26" height="18" rx="3" fill="#ffffff" fill-opacity="0.1"/>
          <rect x="840" width="26" height="18" rx="3" fill="#ffffff" fill-opacity="0.1"/>
          <rect x="870" width="26" height="18" rx="3" fill="#ffffff" fill-opacity="0.1"/>
          <rect x="900" width="26" height="18" rx="3" fill="#ffffff" fill-opacity="0.1"/>
          <rect x="930" width="26" height="18" rx="3" fill="#ffffff" fill-opacity="0.1"/>
        </g>
      </g>
    </g>

    <!-- 3 Active Network Interface Cards -->
    <g transform="translate(40, 275)">
      <!-- Interface 1: 2.5 GbE Ethernet -->
      <g transform="translate(0, 0)">
        <rect width="315" height="260" rx="12" fill="#201a35" stroke="#5c31ff" stroke-opacity="0.7" stroke-width="1.5"/>
        <circle cx="28" cy="32" r="6" fill="#8b5cf6"/>
        <text x="44" y="36" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="15" fill="#ffffff">Ethernet</text>
        <text x="28" y="62" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#94a3b8">Realtek PCIe 2.5GbE Family</text>
        <text x="28" y="80" font-family="'Segoe UI', system-ui, sans-serif" font-size="11" fill="#64748b">Bound IP: 192.168.1.140</text>

        <text x="28" y="145" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="34" fill="#ffffff">114.5</text>
        <text x="135" y="130" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="13" fill="#8b5cf6">MB/S</text>
        <text x="135" y="148" font-family="'Segoe UI', system-ui, sans-serif" font-size="11" fill="#94a3b8">4 Active Chunks</text>

        <rect x="28" y="175" width="259" height="6" rx="3" fill="#ffffff" fill-opacity="0.1"/>
        <rect x="28" y="175" width="180" height="6" rx="3" fill="url(#ethGrad)"/>
        <text x="28" y="210" font-family="'Segoe UI', system-ui, sans-serif" font-size="11.5" fill="#cbd5e1">Received: 2.36 GB · Metric: 25</text>
      </g>

      <!-- Interface 2: Wi-Fi 6 -->
      <g transform="translate(342, 0)">
        <rect width="315" height="260" rx="12" fill="#201a35" stroke="#ec4899" stroke-opacity="0.6" stroke-width="1.5"/>
        <circle cx="28" cy="32" r="6" fill="#f472b6"/>
        <text x="44" y="36" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="15" fill="#ffffff">Wi-Fi 6E</text>
        <text x="28" y="62" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#94a3b8">Intel Wi-Fi 6E AX211 160MHz</text>
        <text x="28" y="80" font-family="'Segoe UI', system-ui, sans-serif" font-size="11" fill="#64748b">Bound IP: 192.168.1.185</text>

        <text x="28" y="145" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="34" fill="#ffffff">61.2</text>
        <text x="115" y="130" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="13" fill="#f472b6">MB/S</text>
        <text x="115" y="148" font-family="'Segoe UI', system-ui, sans-serif" font-size="11" fill="#94a3b8">3 Active Chunks</text>

        <rect x="28" y="175" width="259" height="6" rx="3" fill="#ffffff" fill-opacity="0.1"/>
        <rect x="28" y="175" width="120" height="6" rx="3" fill="url(#wifiGrad)"/>
        <text x="28" y="210" font-family="'Segoe UI', system-ui, sans-serif" font-size="11.5" fill="#cbd5e1">Received: 1.22 GB · Metric: 35</text>
      </g>

      <!-- Interface 3: USB 5G Tethering -->
      <g transform="translate(684, 0)">
        <rect width="315" height="260" rx="12" fill="#201a35" stroke="#84cc16" stroke-opacity="0.6" stroke-width="1.5"/>
        <circle cx="28" cy="32" r="6" fill="#a3e635"/>
        <text x="44" y="36" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="15" fill="#ffffff">USB 5G Tether</text>
        <text x="28" y="62" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#94a3b8">Remote NDIS Compatible Device</text>
        <text x="28" y="80" font-family="'Segoe UI', system-ui, sans-serif" font-size="11" fill="#64748b">Bound IP: 192.168.42.12</text>

        <text x="28" y="145" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="34" fill="#ffffff">18.5</text>
        <text x="110" y="130" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="13" fill="#a3e635">MB/S</text>
        <text x="110" y="148" font-family="'Segoe UI', system-ui, sans-serif" font-size="11" fill="#94a3b8">1 Active Chunk</text>

        <rect x="28" y="175" width="259" height="6" rx="3" fill="#ffffff" fill-opacity="0.1"/>
        <rect x="28" y="175" width="60" height="6" rx="3" fill="url(#celGrad)"/>
        <text x="28" y="210" font-family="'Segoe UI', system-ui, sans-serif" font-size="11.5" fill="#cbd5e1">Received: 360 MB · Metric: 50</text>
      </g>
    </g>
  </g>
</svg>
`;

// -------------------------------------------------------------
// 5. Screenshot 4: Advanced Extension Settings & Filters (1280x800)
// -------------------------------------------------------------
const screenshot4Svg = `
<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="800" viewBox="0 0 1280 800">
  <defs>
    <radialGradient id="s4Bg" cx="0.8" cy="0.8" r="0.9">
      <stop offset="0%" stop-color="#3b82f6" stop-opacity="0.4"/>
      <stop offset="60%" stop-color="#140a33" stop-opacity="0.95"/>
      <stop offset="100%" stop-color="#0a051d" stop-opacity="1"/>
    </radialGradient>
  </defs>

  <rect width="1280" height="800" fill="url(#s4Bg)"/>

  <!-- Top Hero Header -->
  <text x="640" y="80" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="34" fill="#ffffff" letter-spacing="-0.5">Flexible Download Rules &amp; Filters</text>
  <text x="640" y="115" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="17" fill="#cbd5e1">Customize file extension filters, minimum file size thresholds, and domain exclusions</text>

  <!-- Extension Options Page Mockup -->
  <g transform="translate(240, 160)" filter="url(#shadow)">
    <rect width="800" height="580" rx="14" fill="#202026" stroke="#ffffff" stroke-opacity="0.14" stroke-width="1.2"/>
    
    <!-- Header inside options -->
    <g transform="translate(40, 36)">
      <image x="0" y="0" width="36" height="36" href="data:image/png;base64,${icon128B64}"/>
      <text x="48" y="22" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="20" fill="#ffffff">Conflux Browser Settings</text>
      <text x="48" y="40" font-family="'Segoe UI', system-ui, sans-serif" font-size="13" fill="#94a3b8">Configure automatic capture, file extensions, and native host integration</text>
    </g>

    <!-- Card 1: Automatic Download Interception Switch -->
    <g transform="translate(40, 105)">
      <rect width="720" height="74" rx="10" fill="#2b2b34" stroke="#ffffff" stroke-opacity="0.1"/>
      <text x="24" y="32" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="14.5" fill="#ffffff">Automatic Download Interception</text>
      <text x="24" y="52" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#94a3b8">Automatically route matching browser downloads to Conflux Desktop</text>
      
      <!-- Toggle ON -->
      <rect x="646" y="24" width="48" height="26" rx="13" fill="#0078d4"/>
      <circle cx="681" cy="37" r="10" fill="#ffffff"/>
    </g>

    <!-- Card 2: Interception Filters -->
    <g transform="translate(40, 200)">
      <rect width="720" height="240" rx="10" fill="#2b2b34" stroke="#ffffff" stroke-opacity="0.1"/>
      <text x="24" y="30" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="14.5" fill="#ffffff">Interception Filters</text>
      
      <!-- Field 1: Extensions -->
      <text x="24" y="60" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="12.5" fill="#cbd5e1">File Extensions (comma-separated)</text>
      <rect x="24" y="70" width="672" height="34" rx="6" fill="#1b1b22" stroke="#0078d4" stroke-width="1.5"/>
      <text x="36" y="92" font-family="'Segoe UI', system-ui, sans-serif" font-size="12.5" fill="#ffffff">.zip, .rar, .7z, .iso, .img, .bin, .exe, .msi, .tar.gz, .mp4, .mkv, .pdf</text>
      
      <!-- Field 2: Min File Size -->
      <text x="24" y="130" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="12.5" fill="#cbd5e1">Minimum File Size Threshold (MB)</text>
      <rect x="24" y="140" width="200" height="34" rx="6" fill="#1b1b22" stroke="#ffffff" stroke-opacity="0.15"/>
      <text x="36" y="162" font-family="'Segoe UI', system-ui, sans-serif" font-size="13" fill="#ffffff">10</text>
      <text x="235" y="162" font-family="'Segoe UI', system-ui, sans-serif" font-size="12" fill="#94a3b8">MB (Files smaller than this will download via browser)</text>

      <!-- Field 3: Excluded Domains -->
      <text x="24" y="200" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="12.5" fill="#cbd5e1">Excluded Domains: <tspan fill="#38bdf8">localhost, 127.0.0.1, internal.corp</tspan></text>
    </g>

    <!-- Card 3: Connection Status & Save -->
    <g transform="translate(40, 460)">
      <rect width="720" height="70" rx="10" fill="#2b2b34" stroke="#ffffff" stroke-opacity="0.1"/>
      <circle cx="36" cy="35" r="5" fill="#54b054"/>
      <text x="52" y="39" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="13.5" fill="#86efac">Connected to Conflux Desktop Native Messaging Host</text>

      <rect x="580" y="18" width="116" height="34" rx="6" fill="#0078d4"/>
      <text x="638" y="39" text-anchor="middle" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="13" fill="#ffffff">Save Settings</text>
    </g>
  </g>
</svg>
`;

// -------------------------------------------------------------
// 6. Marquee Promo Tile: 1400 x 560 (Large Featured Banner)
// -------------------------------------------------------------
const marqueeSvg = `
<svg xmlns="http://www.w3.org/2000/svg" width="1400" height="560" viewBox="0 0 1400 560">
  <defs>
    <radialGradient id="mBg" cx="0.8" cy="0.9" r="0.9">
      <stop offset="0%" stop-color="#5c31ff" stop-opacity="0.8"/>
      <stop offset="50%" stop-color="#231053" stop-opacity="0.95"/>
      <stop offset="100%" stop-color="#0a051d" stop-opacity="1"/>
    </radialGradient>
    <radialGradient id="mPink" cx="0.3" cy="0.1" r="0.6">
      <stop offset="0%" stop-color="#f88cd4" stop-opacity="0.4"/>
      <stop offset="100%" stop-color="#f88cd4" stop-opacity="0"/>
    </radialGradient>
  </defs>

  <rect width="1400" height="560" fill="url(#mBg)"/>
  <rect width="1400" height="560" fill="url(#mPink)"/>

  <!-- Left Branding & Typography -->
  <g transform="translate(100, 100)">
    <g transform="translate(0, 0)">
      <rect width="52" height="52" rx="12" fill="#181136" stroke="#ffffff" stroke-opacity="0.2"/>
      <image x="6" y="6" width="40" height="40" href="data:image/png;base64,${icon128B64}"/>
      <text x="66" y="36" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="28" fill="#ffffff" letter-spacing="2">CONFLUX</text>
    </g>

    <text x="0" y="115" font-family="'Segoe UI', system-ui, sans-serif" font-weight="900" font-size="52" fill="#ffffff" letter-spacing="-1.5">ONE DOWNLOAD.</text>
    <text x="0" y="175" font-family="'Segoe UI', system-ui, sans-serif" font-weight="900" font-size="52" fill="#f88cd4" letter-spacing="-1.5">EVERY CONNECTION.</text>
    
    <text x="0" y="225" font-family="'Segoe UI', system-ui, sans-serif" font-weight="500" font-size="20" fill="#cbd5e1">Bonds Wi-Fi, Ethernet, and Mobile Tethering for maximum download speed</text>

    <!-- Network Pills -->
    <g transform="translate(0, 260)">
      <rect x="0" y="0" width="130" height="36" rx="18" fill="#5c31ff" fill-opacity="0.3" stroke="#5c31ff" stroke-width="1.5"/>
      <circle cx="18" cy="18" r="5" fill="#a78bfa"/>
      <text x="32" y="23" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="13" fill="#ffffff">ETHERNET</text>

      <rect x="145" y="0" width="115" height="36" rx="18" fill="#f88cd4" fill-opacity="0.25" stroke="#f88cd4" stroke-width="1.5"/>
      <circle cx="163" cy="18" r="5" fill="#f472b6"/>
      <text x="177" y="23" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="13" fill="#ffffff">WI-FI 6</text>

      <rect x="275" y="0" width="125" height="36" rx="18" fill="#def141" fill-opacity="0.2" stroke="#def141" stroke-width="1.5"/>
      <circle cx="293" cy="18" r="5" fill="#def141"/>
      <text x="307" y="23" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="13" fill="#ffffff">USB 5G</text>
    </g>
  </g>

  <!-- Right Floating Speed Glass Card -->
  <g transform="translate(860, 130)" filter="url(#shadow)">
    <rect width="440" height="300" rx="20" fill="#ffffff" fill-opacity="0.08" stroke="#ffffff" stroke-opacity="0.25" stroke-width="1.5"/>
    <text x="36" y="52" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="18" fill="#cbd5e1">CONFLUX ACCELERATOR</text>
    <text x="400" y="52" text-anchor="end" font-family="'Segoe UI', system-ui, sans-serif" font-weight="700" font-size="14" fill="#38bdf8">1-CLICK BROWSER</text>
    
    <text x="34" y="160" font-family="'Segoe UI', system-ui, sans-serif" font-weight="900" font-size="82" fill="#ffffff" letter-spacing="-3">193</text>
    <text x="210" y="132" font-family="'Segoe UI', system-ui, sans-serif" font-weight="800" font-size="24" fill="#f88cd4">MB/S</text>
    <text x="210" y="156" font-family="'Segoe UI', system-ui, sans-serif" font-weight="600" font-size="14" fill="#94a3b8">BONDED SPEED</text>

    <!-- Speed Bar -->
    <rect x="36" y="215" width="368" height="10" rx="5" fill="#ffffff" fill-opacity="0.1"/>
    <rect x="36" y="215" width="180" height="10" rx="5" fill="#5c31ff"/>
    <rect x="216" y="215" width="120" height="10" fill="#f88cd4"/>
    <rect x="336" y="215" width="68" height="10" rx="5" fill="#def141"/>

    <text x="36" y="255" font-family="'Segoe UI', system-ui, sans-serif" font-size="12.5" fill="#cbd5e1">100% Free &amp; Open Source · Windows 10/11</text>
  </g>
</svg>
`;

// Render map
const items = [
  { name: "promo-small-440x280.png", svg: promoSmallSvg, width: 440, height: 280 },
  { name: "screenshot-1-popup.png", svg: screenshot1Svg, width: 1280, height: 800 },
  { name: "screenshot-2-context-menu.png", svg: screenshot2Svg, width: 1280, height: 800 },
  { name: "screenshot-3-desktop-aggregation.png", svg: screenshot3Svg, width: 1280, height: 800 },
  { name: "screenshot-4-filters.png", svg: screenshot4Svg, width: 1280, height: 800 },
  { name: "marquee-1400x560.png", svg: marqueeSvg, width: 1400, height: 560 }
];

console.log("Generating Chrome Web Store Graphic Assets...");

for (const item of items) {
  const resvg = new Resvg(item.svg, {
    font: { loadSystemFonts: true },
    fitTo: { mode: "width", value: item.width }
  });
  const pngData = resvg.render().asPng();
  
  const localPath = path.join(OUT_DIR, item.name);
  fs.writeFileSync(localPath, pngData);
  console.log(`✓ Rendered ${item.name} (${item.width}x${item.height})`);

  if (fs.existsSync(WIN_DOWNLOADS_DIR)) {
    const winPath = path.join(WIN_DOWNLOADS_DIR, item.name);
    fs.writeFileSync(winPath, pngData);
  }
}

// Copy store icon to outputs
const iconDest = path.join(OUT_DIR, "icon-128x128.png");
fs.copyFileSync(path.join(ROOT, "extensions/conflux-browser/icons/icon-128.png"), iconDest);
if (fs.existsSync(WIN_DOWNLOADS_DIR)) {
  fs.copyFileSync(path.join(ROOT, "extensions/conflux-browser/icons/icon-128.png"), path.join(WIN_DOWNLOADS_DIR, "icon-128x128.png"));
}
console.log("✓ Copied icon-128x128.png");

console.log(`\nAll assets generated in ${OUT_DIR}`);
if (fs.existsSync(WIN_DOWNLOADS_DIR)) {
  console.log(`All assets copied to Windows Downloads: ${WIN_DOWNLOADS_DIR}`);
}
