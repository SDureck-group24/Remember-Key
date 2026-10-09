// Remember Key – Anmelde-Knopf auf Login-Seiten.
//
// Sucht auf der Seite ein Login-Formular und setzt in dessen Anmelde-Button (rechter Rand) das
// Remember-Key-Logo, bei zu wenig Platz direkt daneben. Wird kein Button erkannt, sitzt es im
// Passwortfeld. Ein Klick
// fragt über das Hintergrundskript bei der App nach den Zugangsdaten für genau diesen Host;
// ausgefüllt wird dann vom Hintergrundskript (fill.js), die Zugangsdaten kommen nie hierher.
//
// Der Knopf erscheint auf jeder Login-Seite, auch ohne passenden Eintrag: Sonst könnte die
// Seite am Knopf ablesen, dass man dort ein Konto hat. Er liegt in einem geschlossenen
// Shadow-DOM, den die Seite weder lesen noch umgestalten kann.

(() => {
  if (globalThis.__rkLoginButton) return;
  globalThis.__rkLoginButton = true;

  const LOOPBACK = ["localhost", "127.0.0.1", "[::1]"];
  const secure = location.protocol === "https:" || (location.protocol === "http:" && LOOPBACK.includes(location.hostname));
  if (!secure) return;

  const SUBMIT_TEXT = /^(anmelden|einloggen|login|log ?in|sign ?in|weiter|next|continue|fortfahren|absenden|submit|ok)$/i;
  const SUBMIT_HINT = /anmeld|einlog|login|log in|sign ?in/i;
  // Social-Logins, Registrieren, Passwort vergessen usw. sind nicht der Anmelde-Button.
  const NOT_SUBMIT =
    /registr|sign ?up|konto erstellen|create account|vergessen|forgot|zurücksetzen|reset|abbrechen|cancel|google|apple|microsoft|facebook|github|sso|passkey|anzeigen|show/i;

  const visible = (el) => {
    if (!el.isConnected || el.getClientRects().length === 0) return false;
    const s = getComputedStyle(el);
    return s.visibility !== "hidden" && s.display !== "none" && Number(s.opacity) > 0;
  };
  const usableInput = (el) => !el.disabled && !el.readOnly && visible(el);
  const label = (el) =>
    (el.innerText || el.value || el.getAttribute("aria-label") || el.title || "").trim().replace(/\s+/g, " ");

  /** Passwortfeld oder – bei zweistufigen Logins – das Benutzernamenfeld. */
  function findField() {
    const pw = [...document.querySelectorAll('input[type="password"]')].find(usableInput);
    if (pw) return pw;
    return (
      [...document.querySelectorAll("input")].find(
        (i) =>
          ["text", "email", "tel"].includes(i.type) &&
          (i.getAttribute("autocomplete") ?? "").toLowerCase().split(/\s+/).includes("username") &&
          usableInput(i),
      ) ?? null
    );
  }

  function findSubmit(field) {
    const selector = 'button, input[type="submit"], input[type="button"], [role="button"]';
    const score = (b) => {
      const text = label(b);
      if (NOT_SUBMIT.test(text)) return 0;
      const explicitSubmit = b.getAttribute("type") === "submit";
      let s = 0;
      if (SUBMIT_TEXT.test(text)) s += 3;
      else if (SUBMIT_HINT.test(text)) s += 2;
      if (explicitSubmit) s += 2;
      if (s === 0) return 0; // z. B. Auge-Symbol ohne Text und ohne type=submit
      if (field.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) s += 1;
      return s;
    };
    const best = (root) => {
      let top = null;
      let topScore = 0;
      for (const b of root.querySelectorAll(selector)) {
        if (!visible(b) || b.contains(field)) continue;
        const s = score(b);
        if (s > topScore) [top, topScore] = [b, s];
      }
      return top;
    };
    if (field.form) {
      const b = best(field.form);
      if (b) return b;
    }
    // Ohne Formular: die nächsten Vorfahren absuchen.
    let node = field.parentElement;
    for (let depth = 0; node && depth < 6; depth++, node = node.parentElement) {
      const b = best(node);
      if (b) return b;
    }
    return null;
  }

  // ── Oberfläche ────────────────────────────────────────────────────────────────────────

  // Remember-Key-Logo ohne Kachel (Quelle: src-tauri/icons/app-icon.svg); die Kachel ist der Knopf selbst.
  const LOGO_SVG =
    '<svg viewBox="4 4 56 56" fill="none" aria-hidden="true"><circle cx="32" cy="22" r="11" stroke="#9184d9" stroke-width="6"/><path d="M32 33v21M32 44h8M32 53h6" stroke="#9184d9" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/><circle cx="32" cy="22" r="3.5" fill="#e9e9ed"/></svg>';

  // Farben aus dem Nocturne-Designsystem der App, damit der Knopf als Remember Key erkennbar ist.
  const CSS = `
    :host { all: initial; }
    .wrap { position: relative; font: 500 13px/1.2 "Segoe UI", system-ui, sans-serif; color: #e9e9ed; }
    .rk {
      display: grid; place-items: center; box-sizing: border-box;
      width: var(--s, 24px); height: var(--s, 24px); padding: 0; margin: 0;
      background: #232532;
      border: 1px solid #595d6c; border-radius: 6px; cursor: pointer;
      box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
    }
    .rk svg { width: 78%; height: 78%; }
    .rk:hover { background: #2b2741; border-color: #9184d9; }
    .rk[aria-busy="true"] svg { animation: rk-pulse 0.9s ease-in-out infinite; }
    @keyframes rk-pulse { 50% { opacity: 0.35; } }
    @media (prefers-reduced-motion: reduce) { .rk[aria-busy="true"] svg { animation: none; opacity: 0.5; } }
    .rk:focus-visible { outline: 2px solid #9184d9; outline-offset: 2px; }
    .rk:disabled { cursor: progress; opacity: 0.75; }
    .pop {
      position: absolute; top: calc(100% + 6px); right: 0; min-width: 220px; max-width: 320px;
      padding: 6px; box-sizing: border-box; background: #232532; border-radius: 8px;
      box-shadow: 0 0 0 1px #595d6c, 0 10px 30px rgba(0, 0, 0, 0.5);
    }
    .pop[hidden] { display: none; }
    .msg { padding: 6px 8px; line-height: 1.4; white-space: normal; }
    .msg.err { color: #f0b3b8; }
    .choice {
      display: flex; flex-direction: column; gap: 2px; width: 100%; box-sizing: border-box;
      padding: 7px 8px; margin: 0; border: 0; border-radius: 6px; background: none;
      font: inherit; color: #e9e9ed; text-align: left; cursor: pointer;
    }
    .choice:hover, .choice:focus-visible { background: #2b2741; outline: none; }
    .choice .user { font-weight: 400; font-size: 12px; color: #b2b6ca; }
  `;

  let ui = null;

  function createUi() {
    const host = document.createElement("rk-login-button");
    const shadow = host.attachShadow({ mode: "closed" });
    shadow.innerHTML = `<style>${CSS}</style>
      <div class="wrap">
        <button class="rk" type="button" title="Mit Remember Key anmelden" aria-label="Mit Remember Key anmelden">
          ${LOGO_SVG}
        </button>
        <div class="pop" hidden role="status" aria-live="polite"></div>
      </div>`;
    const btn = shadow.querySelector(".rk");
    const pop = shadow.querySelector(".pop");
    btn.addEventListener("click", (e) => {
      // Nur echte Klicks: Ein Skript der Seite kann den Knopf nicht auslösen.
      if (e.isTrusted) login(null);
    });
    shadow.addEventListener("keydown", (e) => {
      if (e.key === "Escape") hidePop();
    });
    document.documentElement.appendChild(host);
    return { host, btn, pop, anchor: null, field: null, mode: "button" };
  }

  function setHostStyle(styles) {
    for (const [k, v] of Object.entries(styles)) ui.host.style.setProperty(k, v, "important");
  }

  function place() {
    if (!ui) return;
    const a = ui.anchor;
    if (!a || !visible(a)) {
      setHostStyle({ display: "none" });
      schedule();
      return;
    }
    const r = a.getBoundingClientRect();
    const size = Math.round(Math.min(28, Math.max(20, r.height - 10)));
    ui.btn.style.setProperty("--s", `${size}px`);
    setHostStyle({ display: "block", position: "fixed", "z-index": "2147483647", margin: "0", padding: "0" });
    const mid = r.top + (r.height - size) / 2;
    const inside = [r.right - size - (r.height - size) / 2, mid];
    let spot;
    if (ui.mode === "field" || r.right - contentRight(a) >= size + 12) {
      // Im Anmelde-Button am rechten Rand (bzw. im Passwortfeld), wo kein Text steht.
      spot = inside;
    } else {
      // Button zu schmal: direkt daneben, sonst darunter oder darüber, wo nichts anderes liegt.
      const spots = [
        [r.right + 6, mid],
        [r.right - size, r.bottom + 4],
        [r.right - size, r.top - size - 4],
      ].filter(([x, y]) => x >= 4 && y >= 4 && x + size <= innerWidth - 4 && y + size <= innerHeight - 4);
      spot = spots.find(([x, y]) => !covers(x, y, size, size, a)) ?? inside;
    }
    const [left, top] = spot;
    setHostStyle({ left: `${Math.round(left)}px`, top: `${Math.round(top)}px` });
  }

  /** Rechte Kante von Text und Symbolen im Button. */
  function contentRight(button) {
    if (button instanceof HTMLInputElement) {
      // <input type="submit">: Text steht meist zentriert, Breite über die Schrift messen.
      const s = getComputedStyle(button);
      const ctx = (contentRight.canvas ??= document.createElement("canvas")).getContext("2d");
      ctx.font = s.font;
      const textWidth = ctx.measureText(button.value).width;
      const r = button.getBoundingClientRect();
      return s.textAlign === "left" || s.textAlign === "start"
        ? r.left + parseFloat(s.paddingLeft) + textWidth
        : r.left + r.width / 2 + textWidth / 2;
    }
    const range = document.createRange();
    range.selectNodeContents(button);
    const rects = [...range.getClientRects()].filter((x) => x.width > 0);
    return rects.length ? Math.max(...rects.map((x) => x.right)) : button.getBoundingClientRect().left;
  }

  /** Liegt an dieser Stelle schon ein Bedienelement der Seite (z. B. „Registrieren“)? */
  function covers(left, top, w, h, anchor) {
    const points = [
      [left + 2, top + 2],
      [left + w - 2, top + 2],
      [left + 2, top + h - 2],
      [left + w - 2, top + h - 2],
      [left + w / 2, top + h / 2],
    ];
    return points.some(([x, y]) => {
      const el = document.elementsFromPoint(x, y).find((e) => e !== ui.host);
      const control = el?.closest('button, a, input, select, textarea, label, [role="button"], [role="link"]');
      return !!control && control !== anchor;
    });
  }

  function removeUi() {
    ui?.host.remove();
    ui = null;
  }

  function scan() {
    const field = findField();
    if (!field) return removeUi();
    const button = findSubmit(field);
    if (!ui) ui = createUi();
    ui.field = field;
    ui.anchor = button ?? field;
    ui.mode = button ? "button" : "field";
    place();
  }

  let scanTimer = 0;
  function schedule() {
    clearTimeout(scanTimer);
    scanTimer = setTimeout(scan, 250);
  }

  let frame = 0;
  const placeSoon = () => {
    if (!frame) frame = requestAnimationFrame(() => ((frame = 0), place()));
  };

  // ── Anmelden ──────────────────────────────────────────────────────────────────────────

  function hidePop() {
    if (ui) ui.pop.hidden = true;
  }

  function showMessage(text, isError) {
    ui.pop.replaceChildren();
    const p = document.createElement("div");
    p.className = isError ? "msg err" : "msg";
    p.textContent = text;
    ui.pop.append(p);
    ui.pop.hidden = false;
    clearTimeout(ui.popTimer);
    ui.popTimer = setTimeout(hidePop, isError ? 6000 : 2500);
  }

  function showChoices(choices) {
    clearTimeout(ui.popTimer);
    ui.pop.replaceChildren();
    for (const c of choices) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "choice";
      const t = document.createElement("span");
      t.textContent = c.title;
      const u = document.createElement("span");
      u.className = "user";
      u.textContent = c.username || "Kein Benutzername";
      b.append(t, u);
      b.addEventListener("click", (e) => {
        if (e.isTrusted) login(c.id);
      });
      ui.pop.append(b);
    }
    ui.pop.hidden = false;
    ui.pop.querySelector(".choice")?.focus();
  }

  async function login(entryId) {
    if (!ui) return;
    ui.btn.disabled = true;
    ui.btn.setAttribute("aria-busy", "true");
    hidePop();
    let r;
    try {
      r = await chrome.runtime.sendMessage({ type: "rk-login", entryId });
    } catch {
      r = { ok: false, error: "Die Remember-Key-Erweiterung ist nicht erreichbar. Bitte die Seite neu laden." };
    }
    if (!ui) return;
    ui.btn.disabled = false;
    ui.btn.removeAttribute("aria-busy");
    if (r?.ok) showMessage("Ausgefüllt und abgesendet", false);
    else if (r?.choices) showChoices(r.choices);
    else showMessage(r?.error ?? "Anmelden fehlgeschlagen", true);
  }

  // Klick außerhalb schließt die Auswahl.
  document.addEventListener(
    "pointerdown",
    (e) => {
      if (ui && e.target !== ui.host) hidePop();
    },
    true,
  );

  new MutationObserver((records) => {
    // Eigene Stiländerungen am Knopf lösen keinen neuen Suchlauf aus.
    if (records.some((r) => !ui || (r.target !== ui.host && !ui.host.contains(r.target)))) schedule();
  }).observe(document.documentElement, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["type", "style", "class", "hidden", "disabled"],
  });
  addEventListener("scroll", placeSoon, { capture: true, passive: true });
  addEventListener("resize", placeSoon, { passive: true });
  // Layout-Verschiebungen ohne Scrollen (Animationen, nachgeladene Inhalte).
  setInterval(() => ui && placeSoon(), 500);
  scan();
})();
