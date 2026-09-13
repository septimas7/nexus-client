// First run, and "Change instance..." from the tray.
//
// The page owns no rules: it hands the typed text to the `connect` command,
// which normalizes it, probes GET /healthz, saves it, and loads the portal.
// Everything shown here on failure is the error string that command returned,
// so the wording lives once, in nexus-desktop-core.

const invoke = window.__TAURI__.core.invoke;

const form = document.getElementById("form");
const address = document.getElementById("address");
const connectButton = document.getElementById("connect");
const status = document.getElementById("status");
const error = document.getElementById("error");
const plaintextNote = document.getElementById("plaintext-note");

let busy = false;

/** The host part of what was typed, for the "Checking ..." line. */
function hostOf(value) {
  const withoutScheme = value.trim().replace(/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//, "");
  const host = withoutScheme.split(/[/?#]/, 1)[0];
  return host || value.trim();
}

function showError(message) {
  error.textContent = message;
  error.hidden = false;
}

function clearError() {
  error.textContent = "";
  error.hidden = true;
}

function setBusy(on, host) {
  busy = on;
  connectButton.disabled = on;
  address.readOnly = on;
  status.textContent = on ? `Checking ${host}...` : "";
}

function refreshPlaintextNote() {
  plaintextNote.hidden = !/^http:\/\//i.test(address.value.trim());
}

async function submit(event) {
  event.preventDefault();
  if (busy) {
    return;
  }
  const value = address.value.trim();
  clearError();
  setBusy(true, hostOf(value));
  try {
    // On success the shell navigates this window to the portal, so nothing
    // after this line is expected to render.
    await invoke("connect", { url: value });
  } catch (reason) {
    setBusy(false);
    showError(String(reason));
    address.focus();
    address.select();
  }
}

form.addEventListener("submit", submit);
address.addEventListener("input", () => {
  clearError();
  refreshPlaintextNote();
});

// Reached from "Change instance..." with an address already bound: prefill it so
// a small correction does not mean retyping the whole thing.
invoke("current_instance")
  .then((current) => {
    if (current && !address.value) {
      address.value = current;
      refreshPlaintextNote();
    }
  })
  .catch(() => {})
  .finally(() => {
    address.focus();
    address.select();
  });
