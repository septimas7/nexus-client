// The instance did not answer. The page never serves stale portal content
// (FR-002): it says so, counts down, and retries. A successful retry is a
// navigation the shell performs, so this page simply disappears.

const invoke = window.__TAURI__.core.invoke;

const RETRY_SECONDS = 15;

const title = document.getElementById("title");
const countdown = document.getElementById("countdown");
const error = document.getElementById("error");
const retryButton = document.getElementById("retry");
const changeButton = document.getElementById("change");
const openBrowser = document.getElementById("open-browser");

let instance = null;
let remaining = RETRY_SECONDS;
let busy = false;
let ticker = null;

function hostOf(url) {
  try {
    const parsed = new URL(url);
    return parsed.port ? `${parsed.hostname}:${parsed.port}` : parsed.hostname;
  } catch {
    return url;
  }
}

function tick() {
  if (busy) {
    return;
  }
  remaining -= 1;
  if (remaining <= 0) {
    retry();
    return;
  }
  countdown.textContent = `Nexus Desktop keeps trying. Retrying in ${remaining} s.`;
}

function restartCountdown() {
  remaining = RETRY_SECONDS;
  countdown.textContent = `Nexus Desktop keeps trying. Retrying in ${remaining} s.`;
}

async function retry() {
  if (busy) {
    return;
  }
  busy = true;
  retryButton.disabled = true;
  countdown.textContent = "Nexus Desktop keeps trying. Checking now...";
  try {
    // On success the shell navigates this window back to the portal.
    await invoke("retry");
  } catch (reason) {
    error.textContent = String(reason);
    error.hidden = false;
    busy = false;
    retryButton.disabled = false;
    restartCountdown();
  }
}

retryButton.addEventListener("click", retry);

// A local page, so this is an ordinary navigation rather than a command.
changeButton.addEventListener("click", () => {
  window.location.href = "connect.html";
});

function openInBrowser() {
  if (instance) {
    invoke("open_external", { url: instance }).catch(() => {});
  }
}

openBrowser.addEventListener("click", openInBrowser);
openBrowser.addEventListener("keydown", (event) => {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    openInBrowser();
  }
});

invoke("current_instance")
  .then((current) => {
    if (current) {
      instance = current;
      const host = hostOf(current);
      title.textContent = `${host} is not reachable`;
      document.title = `${host} is not reachable`;
      openBrowser.textContent = `Open ${host} in your browser`;
      openBrowser.hidden = false;
    }
  })
  .catch(() => {});

restartCountdown();
ticker = window.setInterval(tick, 1000);
window.addEventListener("beforeunload", () => window.clearInterval(ticker));
