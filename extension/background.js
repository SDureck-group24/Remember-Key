// Remember Key – Browser-Erweiterung.
//
// Verbindet sich über Native Messaging mit Remember Key (remember-key-browser.exe). Auf
// Anweisung der App – also erst nachdem der Nutzer dort zugestimmt hat – füllt sie ein
// Login-Formular aus und sendet es ab. Die Erweiterung prüft dabei selbst noch einmal,
// dass die Seite zu den Hosts der Freigabe passt.
//
// Läuft unverändert in Chrome/Edge (Service Worker) und Firefox/Zen (Hintergrundskript).

const HOST = "com.rememberkey.browser";
const LOOPBACK = ["localhost", "127.0.0.1", "[::1]"];

let port = null;
let connecting = false;

async function browserName() {
  try {
    if (globalThis.browser?.runtime?.getBrowserInfo) {
      return (await globalThis.browser.runtime.getBrowserInfo()).name;
    }
  } catch {}
  const brands = (navigator.userAgentData?.brands ?? []).map((b) => b.brand);
  if (brands.some((b) => /Edge/i.test(b))) return "Edge";
  if (brands.some((b) => /Chrome/i.test(b))) return "Chrome";
  return "Browser";
}

async function connect() {
  if (port || connecting) return;
  connecting = true;
  try {
    const p = chrome.runtime.connectNative(HOST);
    p.onMessage.addListener((msg) => onMessage(p, msg));
    p.onDisconnect.addListener(() => {
      void chrome.runtime.lastError; // App läuft nicht oder Host fehlt – später erneut versuchen
      if (port === p) port = null;
      failLogins(NOT_RUNNING);
    });
    port = p;
    // `features`: Die App schickt automatische Logins (Aufrufen) nur an Erweiterungen mit passwordOnly.
    p.postMessage({ type: "hello", browser: await browserName(), features: ["passwordOnly"] });
  } catch {
    port = null;
  } finally {
    connecting = false;
  }
}

// Wiederverbinden, z. B. nachdem Remember Key gestartet wurde.
chrome.alarms.create("reconnect", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener(connect);
chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);
connect();

async function onMessage(p, msg) {
  if (msg?.type === "loginResult") return settleLogin(msg.reqId, msg.result);
  if (msg?.type === "error") return failLogins(msg.message ?? NOT_RUNNING);
  if (msg?.type !== "fill") return;
  let result;
  try {
    result = { Ok: await fill(msg) };
  } catch (e) {
    result = { Err: String(e?.message ?? e) };
  }
  try {
    p.postMessage({ type: "result", id: msg.id, result });
  } catch {}
}

// ── Anmelde-Knopf (content.js) ────────────────────────────────────────────────────────────

const NOT_RUNNING = "Remember Key läuft nicht. Bitte die App starten und erneut klicken.";
const loginWaiting = new Map();
let nextLoginId = 1;

function settleLogin(reqId, result) {
  const w = loginWaiting.get(reqId);
  if (!w) return;
  loginWaiting.delete(reqId);
  clearTimeout(w.timer);
  w.resolve(result ?? { Err: "Ungültige Antwort von Remember Key" });
}

function failLogins(message) {
  for (const id of [...loginWaiting.keys()]) settleLogin(id, { Err: message });
}

/** Fragt die App nach den Zugangsdaten. Wartet länger als beim Ausfüllen: ggf. Windows Hello. */
function askApp(payload) {
  return new Promise((resolve) => {
    if (!port) return resolve({ Err: NOT_RUNNING });
    const reqId = nextLoginId++;
    const timer = setTimeout(() => settleLogin(reqId, { Err: "Remember Key hat nicht rechtzeitig geantwortet" }), 90_000);
    loginWaiting.set(reqId, { resolve, timer });
    try {
      port.postMessage({ ...payload, reqId });
    } catch {
      settleLogin(reqId, { Err: NOT_RUNNING });
    }
  });
}

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg?.type !== "rk-login") return;
  login(msg, sender).then(sendResponse, (e) => sendResponse({ ok: false, error: String(e?.message ?? e) }));
  return true; // Antwort kommt asynchron
});

async function login(msg, sender) {
  // Host aus der Adresse des Frames, nicht aus der Nachricht: Die Seite bestimmt nicht, wofür
  // sie Zugangsdaten bekommt.
  if (!sender.tab?.id || !sender.url) throw new Error("Unbekannte Seite");
  const url = new URL(sender.url);
  const host = url.hostname.toLowerCase().replace(/\.$/, "");
  const secure = url.protocol === "https:" || (url.protocol === "http:" && LOOPBACK.includes(host));
  if (!secure) throw new Error("Nur auf https-Seiten");
  if (!port) await connect();
  const entryId = typeof msg.entryId === "string" ? msg.entryId : null;
  const result = await askApp({ type: "login", host, entryId });
  if ("Err" in result) throw new Error(result.Err);
  const v = result.Ok ?? {};
  if (Array.isArray(v.choices)) return { ok: false, choices: v.choices };
  const target = { tabId: sender.tab.id, frameIds: [sender.frameId ?? 0] };
  await chrome.scripting.executeScript({ target, files: ["fill.js"] });
  const [res] = await chrome.scripting.executeScript({
    target,
    func: (...args) => globalThis.rkFill(...args),
    args: [[host], v.username ?? "", v.password ?? "", null, false],
  });
  const r = res?.result;
  if (!r?.ok) throw new Error(r?.error ?? "Ausfüllen fehlgeschlagen");
  return { ok: true };
}

/** Gleiche Regel wie in der App: exakter Host oder `*.domain` für Subdomains. */
function hostAllowed(hosts, host) {
  host = host.toLowerCase().replace(/\.$/, "");
  return hosts.some((h) => {
    if (h.startsWith("*.")) {
      const base = h.slice(1); // ".domain"
      return host.endsWith(base) && host.length > base.length;
    }
    return h === host;
  });
}

function tabMatches(tab, hosts, hint) {
  try {
    const u = new URL(tab.url);
    const host = u.hostname.toLowerCase();
    const secure = u.protocol === "https:" || (u.protocol === "http:" && LOOPBACK.includes(host));
    return secure && hostAllowed(hosts, host) && (!hint || host === hint);
  } catch {
    return false;
  }
}

async function fill({ hosts, hostHint, username, password, otp, passwordOnly }) {
  const tabs = (await chrome.tabs.query({})).filter((t) => tabMatches(t, hosts, hostHint));
  if (!tabs.length) {
    throw new Error(`Kein offener Tab mit ${hostHint ?? hosts.join(", ")} in diesem Browser`);
  }
  // Aktiver Tab zuerst, sonst der zuletzt benutzte.
  tabs.sort((a, b) => Number(b.active) - Number(a.active) || (b.lastAccessed ?? 0) - (a.lastAccessed ?? 0));
  const tab = tabs[0];
  // Aufrufen aus der App: Die App versucht es erneut, bis die Seite fertig geladen ist.
  if (passwordOnly && tab.status !== "complete") throw new Error("Die Seite lädt noch");
  await chrome.scripting.executeScript({ target: { tabId: tab.id }, files: ["fill.js"] });
  const [res] = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    func: (...args) => globalThis.rkFill(...args),
    args: [hosts, username, password, otp ?? null, !!passwordOnly],
  });
  const r = res?.result;
  if (!r?.ok) throw new Error(r?.error ?? "Ausfüllen fehlgeschlagen");
  return { host: new URL(tab.url).hostname, title: tab.title ?? "", filled: r.filled, submitted: r.submitted };
}
