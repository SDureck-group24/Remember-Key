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
  const [res] = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    func: fillInPage,
    args: [hosts, username, password, otp ?? null, !!passwordOnly],
  });
  const r = res?.result;
  if (!r?.ok) throw new Error(r?.error ?? "Ausfüllen fehlgeschlagen");
  return { host: new URL(tab.url).hostname, title: tab.title ?? "", filled: r.filled, submitted: r.submitted };
}

// Läuft in der Seite (isolierte Welt der Erweiterung). Muss in sich abgeschlossen sein.
function fillInPage(hosts, username, password, otp, passwordOnly) {
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
  // Ohne Passwortfeld könnte auf einer schon angemeldeten Seite ein beliebiges Feld getroffen werden.
  if (passwordOnly && !pw) return { ok: false, error: "Kein Login-Formular mit Passwortfeld auf der Seite" };
  const scope = pw?.form ?? document;
  const userFields = [
    ...scope.querySelectorAll('input[type="email"], input[type="text"], input[type="tel"], input:not([type])'),
  ].filter(usable);

  // Mehrstufige Logins: Seite mit Code-Abfrage (2FA), wenn kein Passwortfeld da ist.
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  const set = (el, value) => {
    el.focus();
    setter.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  };
  const submit = (field) => {
    const form = field.form;
    const button = [
      ...(form ?? document).querySelectorAll('button[type="submit"], input[type="submit"], button:not([type])'),
    ].find(usable);
    if (form && typeof form.requestSubmit === "function") {
      try {
        form.requestSubmit(button && form.contains(button) ? button : undefined);
        return true;
      } catch {}
    }
    if (button) {
      button.click();
      return true;
    }
    for (const type of ["keydown", "keypress", "keyup"]) {
      field.dispatchEvent(new KeyboardEvent(type, { key: "Enter", code: "Enter", keyCode: 13, bubbles: true }));
    }
    return true;
  };
  if (!pw && otp) {
    const textLike = (i) => ["text", "tel", "number"].includes(i.type);
    const inputs = [...document.querySelectorAll("input")].filter((i) => usable(i) && textLike(i));
    // Einzelne Kästchen je Ziffer
    const boxes = inputs.filter((i) => i.maxLength === 1);
    if (boxes.length >= otp.length) {
      [...otp].forEach((digit, k) => set(boxes[k], digit));
      return { ok: true, filled: ["otp"], submitted: submit(boxes[otp.length - 1]) };
    }
    const otpHint = /otp|totp|2fa|mfa|one.?time|verif|code|token|pin/i;
    const field = inputs.find(
      (i) =>
        i.autocomplete === "one-time-code" ||
        otpHint.test(`${i.name} ${i.id} ${i.placeholder ?? ""} ${i.getAttribute("aria-label") ?? ""}`),
    );
    if (field) {
      set(field, otp);
      return { ok: true, filled: ["otp"], submitted: submit(field) };
    }
  }

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
  return { ok: true, filled, submitted: submit(pw ?? user) };
}
