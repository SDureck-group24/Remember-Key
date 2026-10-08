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
    });
    port = p;
    p.postMessage({ type: "hello", browser: await browserName() });
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

async function fill({ hosts, hostHint, username, password }) {
  const tabs = (await chrome.tabs.query({})).filter((t) => tabMatches(t, hosts, hostHint));
  if (!tabs.length) {
    throw new Error(`Kein offener Tab mit ${hostHint ?? hosts.join(", ")} in diesem Browser`);
  }
  // Aktiver Tab zuerst, sonst der zuletzt benutzte.
  tabs.sort((a, b) => Number(b.active) - Number(a.active) || (b.lastAccessed ?? 0) - (a.lastAccessed ?? 0));
  const tab = tabs[0];
  const [res] = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    func: fillInPage,
    args: [hosts, username, password],
  });
  const r = res?.result;
  if (!r?.ok) throw new Error(r?.error ?? "Ausfüllen fehlgeschlagen");
  return { host: new URL(tab.url).hostname, title: tab.title ?? "", filled: r.filled, submitted: r.submitted };
}

// Läuft in der Seite (isolierte Welt der Erweiterung). Muss in sich abgeschlossen sein.
function fillInPage(hosts, username, password) {
  const host = location.hostname.toLowerCase().replace(/\.$/, "");
  const allowed = hosts.some((h) =>
    h.startsWith("*.") ? host.endsWith(h.slice(1)) && host.length > h.length - 1 : h === host,
  );
  const loopback = ["localhost", "127.0.0.1", "[::1]"].includes(host);
  if (!allowed) return { ok: false, error: "Die Seite passt nicht zur Freigabe" };
  if (location.protocol !== "https:" && !(location.protocol === "http:" && loopback)) {
    return { ok: false, error: "Nur https-Seiten (http nur für localhost)" };
  }

  const usable = (el) => {
    if (el.disabled || el.readOnly) return false;
    const style = getComputedStyle(el);
    if (style.visibility === "hidden" || style.display === "none") return false;
    return el.getClientRects().length > 0;
  };
  const pw = [...document.querySelectorAll('input[type="password"]')].find(usable) ?? null;
  const scope = pw?.form ?? document;
  const userFields = [
    ...scope.querySelectorAll('input[type="email"], input[type="text"], input[type="tel"], input:not([type])'),
  ].filter(usable);

  let user = null;
  if (pw) {
    // Das letzte Textfeld vor dem Passwortfeld.
    user = userFields.filter((u) => u.compareDocumentPosition(pw) & Node.DOCUMENT_POSITION_FOLLOWING).pop() ?? null;
  } else {
    // Zweistufiger Login: erst der Benutzername.
    const hint = /user|mail|login|name|account|konto|benutzer|ident/i;
    user =
      userFields.find((u) => hint.test(`${u.name} ${u.id} ${u.autocomplete} ${u.placeholder ?? ""}`)) ??
      (userFields.length === 1 ? userFields[0] : null);
  }
  if (!pw && !user) return { ok: false, error: "Kein Login-Formular auf der Seite gefunden" };

  // Über den nativen Setter, damit Frameworks wie React die Änderung bemerken.
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  const set = (el, value) => {
    el.focus();
    setter.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  };
  const filled = [];
  if (user && username) {
    set(user, username);
    filled.push("username");
  }
  if (pw) {
    set(pw, password);
    filled.push("password");
  }

  // Sofort absenden, damit das Passwort möglichst kurz im Feld steht.
  const field = pw ?? user;
  const form = field.form;
  const button = [
    ...(form ?? document).querySelectorAll('button[type="submit"], input[type="submit"], button:not([type])'),
  ].find(usable);
  let submitted = false;
  if (form && typeof form.requestSubmit === "function") {
    try {
      form.requestSubmit(button && form.contains(button) ? button : undefined);
      submitted = true;
    } catch {}
  }
  if (!submitted && button) {
    button.click();
    submitted = true;
  }
  if (!submitted) {
    for (const type of ["keydown", "keypress", "keyup"]) {
      field.dispatchEvent(new KeyboardEvent(type, { key: "Enter", code: "Enter", keyCode: 13, bubbles: true }));
    }
    submitted = true;
  }
  return { ok: true, filled, submitted };
}
