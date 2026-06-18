// ----- interactive mic demo -----
const micBtn = document.getElementById("micBtn");
const waveform = document.getElementById("waveform");
const rawText = document.getElementById("rawText");
const cleanText = document.getElementById("cleanText");
const tClean = document.getElementById("tClean");

const SAMPLES = [
  {
    raw: "um, like, can you, uh, send him a message saying I'll be late",
    clean: "Can you send him a message saying I'll be late?",
  },
  {
    raw: "so yeah basically i wanna, you know, refactor the whole auth thing tomorrow morning",
    clean: "I want to refactor the entire auth module tomorrow morning.",
  },
  {
    raw: "hey can you uh remind me to like buy milk and also eggs on the way home",
    clean: "Remind me to buy milk and eggs on the way home.",
  },
];

const BAR_COUNT = 36;
const bars = [];
for (let i = 0; i < BAR_COUNT; i++) {
  const b = document.createElement("span");
  b.className = "bar";
  waveform.appendChild(b);
  bars.push(b);
}

let recording = false;
let waveTimer = null;
let typeTimers = [];
let sampleIdx = 0;

function clearTimers() {
  typeTimers.forEach(clearTimeout);
  typeTimers = [];
}

function animateBars(active) {
  bars.forEach((b) => {
    const h = active ? 8 + Math.random() * 42 : 8;
    b.style.height = h + "px";
  });
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

function startRecording() {
  recording = true;
  micBtn.classList.add("recording");
  micBtn.setAttribute("aria-pressed", "true");
  tClean.classList.remove("show");
  cleanText.textContent = "";
  rawText.textContent = "listening…";
  waveTimer = setInterval(() => animateBars(true), 110);
}

function stopRecording() {
  recording = false;
  micBtn.classList.remove("recording");
  micBtn.setAttribute("aria-pressed", "false");
  clearInterval(waveTimer);
  animateBars(false);

  const sample = SAMPLES[sampleIdx % SAMPLES.length];
  sampleIdx++;

  // transcribe (raw) then refine (clean)
  typeOut(rawText, sample.raw, 22, () => {
    typeTimers.push(
      setTimeout(() => {
        tClean.classList.add("show");
        typeOut(cleanText, sample.clean, 26);
      }, 450)
    );
  });
}

micBtn.addEventListener("click", () => {
  clearTimers();
  if (!recording) {
    startRecording();
    // auto-stop after a short "recording" window for the demo
    typeTimers.push(setTimeout(() => recording && stopRecording(), 2200));
  } else {
    stopRecording();
  }
});

// ----- scroll reveal -----
const revealEls = document.querySelectorAll(
  ".card, .step, .privacy-banner, .download-card, .section-title"
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

// gentle idle wave so it's not totally static before first click
animateBars(false);
