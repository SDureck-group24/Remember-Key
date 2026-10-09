// Remember Key – Ausfüllen eines Login-Formulars in der Seite.
//
// Läuft in der isolierten Welt der Erweiterung: als Inhaltsskript auf jeder Seite (für den
// Anmelde-Knopf) und per scripting.executeScript (Befehle der App). Die Seite selbst sieht
// diese Funktion nicht.

globalThis.rkFill = function rkFill(hosts, username, password, otp, passwordOnly) {
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
};

// Ergebnis des Skripts für scripting.executeScript({ files }): Firefox überträgt den Wert der
// letzten Anweisung und scheitert an einer Funktion ("non-structured-clonable data").
undefined;
