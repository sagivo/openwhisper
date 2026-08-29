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
          startIdle();
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
