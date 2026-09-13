// Shown while an accepted update downloads and installs. The updater task in
// the shell emits progress; this page only renders it. The app restarts itself
// when the install finishes, so there is no success state to draw.

const listen = window.__TAURI__.event.listen;

const title = document.getElementById("title");
const bar = document.getElementById("bar");
const detail = document.getElementById("detail");
const error = document.getElementById("error");

function megabytes(bytes) {
  return (bytes / (1024 * 1024)).toFixed(1);
}

listen("nexus://update-progress", (event) => {
  const { version, downloaded, total, phase, reason } = event.payload ?? {};

  if (version) {
    title.textContent = `Installing Nexus Desktop v${version}...`;
    document.title = `Installing Nexus Desktop v${version}`;
  }

  if (phase === "failed") {
    bar.removeAttribute("value");
    detail.textContent = "";
    error.textContent = reason ?? "The update could not be installed. Try again later.";
    error.hidden = false;
    return;
  }

  if (phase === "installing") {
    bar.value = 100;
    detail.textContent = "Installing.";
    return;
  }

  if (typeof total === "number" && total > 0) {
    bar.value = Math.min(100, Math.round((downloaded / total) * 100));
    detail.textContent = `Downloading ${megabytes(downloaded)} of ${megabytes(total)} MB.`;
  } else if (typeof downloaded === "number") {
    // A server that sends no content length still shows movement.
    bar.removeAttribute("value");
    detail.textContent = `Downloading ${megabytes(downloaded)} MB.`;
  }
});
