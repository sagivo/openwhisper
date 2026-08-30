// ----- interactive mic demo -----
const micBtn = document.getElementById("micBtn");
const waveform = document.getElementById("waveform");
const rawText = document.getElementById("rawText");
const cleanText = document.getElementById("cleanText");
const tClean = document.getElementById("tClean");
const sendPill = document.getElementById("sendPill");
const fillerStage = document.getElementById("fillerStage");
const demoHint = document.getElementById("demoHint");
const demo = document.getElementById("demo");
const tapSticker = document.querySelector(".sticker-tap");
const fnKey = document.getElementById("fnKey");

const SAMPLES = [
  {
    raw: "um, like, can you, uh, send him a message saying I'll be late",
    clean: "Can you send him a message saying I'll be late?",
    fillers: ["um", "like", "uh"],
  },
  {
    raw: "so yeah basically i wanna, you know, refactor the whole auth thing tomorrow morning",
    clean: "I want to refactor the entire auth module tomorrow morning.",
    fillers: ["so yeah", "basically", "you know"],
  },
  {
    raw: "hey can you uh remind me to like buy milk and also eggs on the way home",
    clean: "Remind me to buy milk and eggs on the way home.",
    fillers: ["uh", "like"],
  },
  {
    raw: "wait wait I mean tell sarah i'll be like ten minutes late, my dog ate my uh charger",
    clean: "Tell Sarah I'll be ten minutes late — my dog ate my charger.",
    fillers: ["wait wait", "I mean", "like", "uh"],
  },
  {
    raw: "ok so yeah can we kinda move that meeting to friday instead of thursday",
    clean: "Can we move that meeting to Friday instead of Thursday?",
    fillers: ["so yeah", "kinda"],
  },
];

const HINTS = [
  "I'll catch the ums. You just talk.",
  "Ramble freely. I'm the grown-up in this conversation.",
  "Your keyboard called. It's taking a nap.",
  "Hold forth. I'll do the punctuation.",
];

const BAR_COUNT = 36;
const bars = [];
for (let i = 0; i < BAR_COUNT; i++) {
  const b = document.createElement("span");
  b.className = "bar";
  waveform.appendChild(b);
  bars.push(b);
}

const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

let recording = false;
let waveTimer = null;
let idleTimer = null;
let autoTimer = null;
let typeTimers = [];
let sampleIdx = 0;
let userTookOver = false;
let idleT = 0;

function clearTimers() {
  typeTimers.forEach(clearTimeout);
  typeTimers = [];
}

function animateBars(active) {
  bars.forEach((b, i) => {
    const h = active ? 8 + Math.random() * 42 : 8;
    b.style.height = h + "px";
    if (active) b.style.transform = `translateY(${(Math.random() - 0.5) * 4}px)`;
    else b.style.transform = "";
  });
}

function idleWave() {
  idleT += 0.08;
  bars.forEach((b, i) => {
    const h = 10 + Math.sin(idleT + i * 0.35) * 8 + Math.sin(idleT * 0.6 + i) * 4;
    b.style.height = Math.max(6, h) + "px";
  });
  idleTimer = requestAnimationFrame(idleWave);
}

function startIdle() {
  if (reduceMotion) {
    animateBars(false);
    return;
  }
  cancelAnimationFrame(idleTimer);
  idleTimer = requestAnimationFrame(idleWave);
}

function stopIdle() {
  cancelAnimationFrame(idleTimer);
}

function typeOut(el, text, perChar, done) {
  el.textContent = "";
  let i = 0;
  function tick() {
    if (i <= text.length) {
      el.textContent = text.slice(0, i);
      i++;
      typeTimers.push(setTimeout(tick, perChar));
    } else if (done) {
      done();
    }
  }
  tick();
}

function spawnFillers(words) {
  if (reduceMotion || !fillerStage) return;
  words.forEach((word, i) => {
    typeTimers.push(
      setTimeout(() => {
        const chip = document.createElement("span");
        chip.className = "filler-chip";
        chip.textContent = word;
        const dx = (Math.random() * 160 - 80) | 0;
        const dy = -80 - ((Math.random() * 80) | 0);
        chip.style.setProperty("--dx", dx + "px");
        chip.style.setProperty("--dy", dy + "px");
        chip.style.left = 18 + Math.random() * 64 + "%";
        chip.style.top = "42%";
        fillerStage.appendChild(chip);
        setTimeout(() => chip.remove(), 1200);
      }, i * 90)
    );
  });
}

function startRecording() {
  recording = true;
  stopIdle();
  micBtn.classList.add("recording");
  micBtn.setAttribute("aria-pressed", "true");
  tClean.classList.remove("show");
  cleanText.textContent = "";
  if (sendPill) sendPill.hidden = true;
  rawText.textContent = "listening…";
  if (demoHint) demoHint.textContent = HINTS[sampleIdx % HINTS.length];
  if (tapSticker) tapSticker.style.display = "none";
  waveTimer = setInterval(() => animateBars(true), 90);
}

function stopRecording() {
  recording = false;
  micBtn.classList.remove("recording");
  micBtn.setAttribute("aria-pressed", "false");
  clearInterval(waveTimer);
  animateBars(false);

  const sample = SAMPLES[sampleIdx % SAMPLES.length];
  sampleIdx++;

  typeOut(rawText, sample.raw, 18, () => {
    spawnFillers(sample.fillers);
    typeTimers.push(
      setTimeout(() => {
        tClean.classList.add("show");
        typeOut(cleanText, sample.clean, 22, () => {
          if (sendPill) sendPill.hidden = false;
          if (!userTookOver && !reduceMotion) {
            autoTimer = setTimeout(runDemo, 2800);
          }
        });
      }, 380)
    );
  });
}

function runDemo() {
  if (recording) return;
  clearTimers();
  startRecording();
  typeTimers.push(setTimeout(() => recording && stopRecording(), 2000));
}

micBtn.addEventListener("click", () => {
  userTookOver = true;
  clearTimeout(autoTimer);
  clearTimers();
  if (!recording) {
    startRecording();
    typeTimers.push(setTimeout(() => recording && stopRecording(), 2200));
  } else {
    stopRecording();
  }
});

if (fnKey) {
  fnKey.addEventListener("click", () => {
    fnKey.classList.add("pressed");
    setTimeout(() => fnKey.classList.remove("pressed"), 180);
    document.getElementById("demo")?.scrollIntoView({ behavior: reduceMotion ? "auto" : "smooth", block: "center" });
    userTookOver = true;
    clearTimeout(autoTimer);
    if (!recording) runDemo();
  });
}

// ----- scroll reveal -----
const revealEls = document.querySelectorAll(
  ".card, .step, .privacy-banner, .download-card, .section-title, .graveyard, .docs-feature"
);
revealEls.forEach((el) => el.classList.add("reveal"));

const io = new IntersectionObserver(
  (entries) => {
    entries.forEach((e) => {
      if (e.isIntersecting) {
        e.target.classList.add("visible");
        io.unobserve(e.target);
      }
    });
  },
  { threshold: 0.12 }
);
revealEls.forEach((el) => io.observe(el));

// ----- copy buttons -----
document.querySelectorAll(".copy-btn").forEach((btn) => {
  btn.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(btn.dataset.copy);
      const orig = btn.textContent;
      btn.textContent = "copied ✓";
      btn.classList.add("copied");
      setTimeout(() => {
        btn.textContent = orig;
        btn.classList.remove("copied");
      }, 1600);
    } catch {
      btn.textContent = "copy failed";
    }
  });
});

startIdle();

if (!reduceMotion) {
  autoTimer = setTimeout(runDemo, 1400);
}

// ----- analytics: download clicks -----
function trackDownloadCTA(location) {
  window.posthog?.capture("download_cta_clicked", { location });
}

document.querySelectorAll('a[href="#download"]').forEach((el) => {
  el.addEventListener("click", () => {
    const loc = el.classList.contains("nav-cta") ? "nav" : el.closest(".hero") ? "hero" : "footer";
    trackDownloadCTA(loc);
  });
});

const RELEASES_API = "https://api.github.com/repos/sagivo/openwhisper/releases/latest";
const RELEASES_PAGE = "https://github.com/sagivo/openwhisper/releases/";
const ARCH_LABELS = { arm64: "Apple Silicon", x64: "Intel" };
const ARCH_TOKENS = { arm64: ["aarch64", "arm64"], x64: ["x86_64", "x64", "amd64"] };

function isMacDesktop() {
  const ua = navigator.userAgent || "";
  const platform = navigator.platform || "";
  const uaPlatform = navigator.userAgentData?.platform;
  if (/iPhone|iPad|iPod/.test(ua)) return false;
  if (platform === "MacIntel" && navigator.maxTouchPoints > 1) return false;
  if (uaPlatform) return uaPlatform === "macOS";
  return /Mac/i.test(platform) || /Mac OS X/i.test(ua);
}

function webglRenderer() {
  try {
    const gl = document.createElement("canvas").getContext("webgl");
    const ext = gl?.getExtension("WEBGL_debug_renderer_info");
    return ext ? String(gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) || "") : "";
  } catch {
    return "";
  }
}

async function detectMacArch() {
  if (navigator.userAgentData?.getHighEntropyValues) {
    try {
      const { architecture } = await navigator.userAgentData.getHighEntropyValues(["architecture"]);
      if (architecture === "arm") return "arm64";
      if (architecture === "x86") return "x64";
    } catch {
      /* fall through */
    }
  }

  const renderer = webglRenderer();
  if (/Apple\s+M\d|Apple GPU|Apple silicon/i.test(renderer)) return "arm64";
  if (/Intel|AMD|Radeon|NVIDIA/i.test(renderer)) return "x64";
  if (/Apple/i.test(renderer)) return "arm64";

  return "arm64";
}

function assetArch(name) {
  const lower = name.toLowerCase();
  if (ARCH_TOKENS.arm64.some((token) => lower.includes(token))) return "arm64";
  if (ARCH_TOKENS.x64.some((token) => lower.includes(token))) return "x64";
  return null;
}

function pickDmg(assets, arch) {
  const dmgs = (assets || []).filter((asset) => /\.dmg$/i.test(asset.name));
  return dmgs.find((asset) => assetArch(asset.name) === arch) || null;
}

async function fetchLatestRelease() {
  const cacheKey = "ow-latest-release";
  try {
    const cached = JSON.parse(sessionStorage.getItem(cacheKey) || "");
    if (cached?.data?.assets && Date.now() - cached.at < 10 * 60 * 1000) return cached.data;
  } catch {
    /* ignore */
  }
  const res = await fetch(RELEASES_API, { headers: { Accept: "application/vnd.github+json" } });
  if (!res.ok) throw new Error("release fetch failed");
  const data = await res.json();
  try {
    sessionStorage.setItem(cacheKey, JSON.stringify({ at: Date.now(), data }));
  } catch {
    /* ignore quota / private mode */
  }
  return data;
}

const downloadBtn = document.getElementById("downloadBtn");
const downloadBtnLabel = document.getElementById("downloadBtnLabel");
const downloadAlt = document.getElementById("downloadAlt");

function guestPlatformName() {
  const platform = navigator.userAgentData?.platform || navigator.platform || "";
  const ua = navigator.userAgent || "";
  if (/Win/i.test(platform) || /Windows/i.test(ua)) return "Windows";
  if (/Android/i.test(ua)) return "Android";
  if (/iPhone|iPad|iPod/.test(ua) || (platform === "MacIntel" && navigator.maxTouchPoints > 1)) return "iOS";
  if (/Linux/i.test(platform) || /Linux/i.test(ua)) return "Linux";
  return null;
}

function setComingSoon(message) {
  if (!downloadBtn) return;
  downloadBtn.href = "#download";
  downloadBtn.setAttribute("aria-disabled", "true");
  downloadBtn.dataset.comingSoon = "true";
  if (downloadBtnLabel) downloadBtnLabel.textContent = "Coming soon";
  if (downloadAlt) {
    downloadAlt.hidden = false;
    downloadAlt.textContent = message;
  }
}

if (downloadBtn) {
  downloadBtn.addEventListener("click", (e) => {
    if (downloadBtn.dataset.comingSoon === "true") {
      e.preventDefault();
      window.posthog?.capture("download_clicked", {
        href: downloadBtn.href,
        coming_soon: true,
        $current_url: window.location.href,
      });
      return;
    }
    window.posthog?.capture("download_clicked", {
      href: downloadBtn.href,
      arch: downloadBtn.dataset.arch || null,
      $current_url: window.location.href,
    });
    e.preventDefault();
    setTimeout(() => {
      window.location.href = downloadBtn.href;
    }, 150);
  });

  (async () => {
    if (!isMacDesktop()) {
      const platform = guestPlatformName();
      setComingSoon(
        platform
          ? `${platform} isn't ready yet — OpenWhisper is macOS-only for now.`
          : "OpenWhisper is macOS-only for now. Windows and Linux are coming soon."
      );
      return;
    }

    try {
      const [arch, release] = await Promise.all([detectMacArch(), fetchLatestRelease()]);
      const matched = pickDmg(release.assets, arch);
      const otherArch = arch === "arm64" ? "x64" : "arm64";
      const other = pickDmg(release.assets, otherArch);

      if (matched?.browser_download_url) {
        downloadBtn.href = matched.browser_download_url;
        downloadBtn.dataset.arch = arch;
        if (downloadBtnLabel) {
          downloadBtnLabel.textContent = `Download free for Mac (${ARCH_LABELS[arch]})`;
        }
      }

      if (other?.browser_download_url && downloadAlt) {
        downloadAlt.hidden = false;
        downloadAlt.replaceChildren();
        downloadAlt.append("Need the ", ARCH_LABELS[otherArch], " version instead? ");
        const link = document.createElement("a");
        link.href = other.browser_download_url;
        link.textContent = `Download for ${ARCH_LABELS[otherArch]}`;
        link.addEventListener("click", () => {
          window.posthog?.capture("download_clicked", {
            href: link.href,
            arch: otherArch,
            source: "alt",
            $current_url: window.location.href,
          });
        });
        downloadAlt.append(link);
      }
    } catch {
      downloadBtn.href = RELEASES_PAGE;
    }
  })();
}
