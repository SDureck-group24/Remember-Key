# Privacy Policy / Datenschutzerklärung

**Remember Key** (Windows desktop app) and the **Remember Key browser extension**
Last updated / Stand: 8 October 2026 (version 0.2.0)

[English](#english) · [Deutsch](#deutsch)

---

## English

### Summary

Remember Key is a local password manager. The developer does not collect, receive or store any of your
data. There is no account, no telemetry, no analytics and no advertising. Data only leaves your computer
through features you switch on yourself: Google Drive sync, and AI access with the actions you confirm.

### What is stored on your computer

| Data | Where | Protection |
|---|---|---|
| Your vault (entries, passwords, API tokens, 2FA keys, notes, folders, settings) | `%APPDATA%\com.rememberkey.app\vault.rk` and a backup `vault.rk.bak` | Encrypted with a key derived from your master password (Argon2id, XChaCha20-Poly1305) |
| Log of AI requests (time, tool, entry title, host, result; never passwords) | `%APPDATA%\com.rememberkey.app\agent-log.jsonl`, at most 1,000 lines | Not encrypted, readable by your Windows account |
| Google Drive connection (client ID, sync status) | `%APPDATA%\com.rememberkey.app\sync.json` | Not encrypted, no secrets |
| Google Drive sign-in token | Windows Credential Manager | Protected by Windows |
| Browser connection files | `%APPDATA%\com.rememberkey.app\native\` and registry entries under `HKCU\Software\…\NativeMessagingHosts` | Only allow the Remember Key extension |

The master password is never stored. Copied passwords are excluded from the Windows clipboard history
and cloud clipboard and are cleared after 30 seconds by default.

### What can leave your computer and when

**Google Drive sync (optional, off by default).** If you connect Google Drive, only the encrypted file
`vault.rk` is uploaded to a hidden app folder in your own Google Drive. The app only requests access to
that folder (`drive.appdata`). Google cannot read your vault without your master password. Google's
privacy policy applies to the data held by Google.

**AI access (optional, off by default).** If you switch on AI access and allow individual entries:

- AI assistants on your computer (for example Claude) can see the **title, username and allowed hosts**
  of the entries you released. They never receive passwords, API tokens, notes or 2FA keys. What the AI
  sees is processed by the provider of that AI assistant under its own privacy policy.
- With `http_request`, Remember Key sends your API token or password to a website **you allowed for
  that entry**, and only after you confirm the request. The website's response is passed to the AI with
  your secrets removed.
- With `fill_login`, the browser extension fills in username, password and, if needed, the current 2FA
  code on a website **you allowed for that entry**, after you confirm, and submits the form. The website
  receives your login just as if you had typed it.

**No other connections.** The app does not contact the developer or any other server, does not check
for updates in the background and does not send crash reports.

### The browser extension

The extension collects no data and makes no network requests of its own. It communicates only with the
Remember Key app on your computer (native messaging). It receives username, password or 2FA code from
the app only for a login you have just confirmed, keeps them in memory just long enough to fill in the
form, and does not save them.

Permissions it uses:

- **Access to all websites:** to fill in login forms on the websites you allow. Other pages are not read
  or changed.
- **Tabs:** to find the tab whose address matches the entry.
- **Scripting:** to fill in the form in that tab.
- **Native messaging:** to talk to the Remember Key app.
- **Alarms:** to reconnect to the app when it is started.

### Your control

You can switch off AI access or Google Drive sync at any time in the app settings, release or withdraw
entries individually, and delete the files listed above. Uninstalling the app and the extension removes
the programs. Your vault and the files under `%APPDATA%\com.rememberkey.app` stay until you delete them.

### Contact

Questions about privacy: <https://github.com/SDureck-group24/Remember-Key/issues>

---

## Deutsch

### Kurzfassung

Remember Key ist ein lokaler Passwort-Manager. Der Entwickler erhebt, empfängt und speichert keine deiner
Daten. Es gibt kein Konto, keine Telemetrie, keine Analyse und keine Werbung. Daten verlassen deinen
Rechner nur über Funktionen, die du selbst einschaltest: den Abgleich mit Google Drive und den KI-Zugriff
mit den Aktionen, die du bestätigst.

### Was auf deinem Rechner gespeichert wird

| Daten | Ort | Schutz |
|---|---|---|
| Dein Tresor (Einträge, Passwörter, API-Tokens, 2FA-Schlüssel, Notizen, Ordner, Einstellungen) | `%APPDATA%\com.rememberkey.app\vault.rk` und eine Sicherung `vault.rk.bak` | Verschlüsselt mit einem Schlüssel aus deinem Master-Passwort (Argon2id, XChaCha20-Poly1305) |
| Protokoll der KI-Anfragen (Zeit, Tool, Eintragstitel, Host, Ergebnis; nie Passwörter) | `%APPDATA%\com.rememberkey.app\agent-log.jsonl`, höchstens 1.000 Zeilen | Nicht verschlüsselt, lesbar für dein Windows-Konto |
| Google-Drive-Verbindung (Client-ID, Abgleichstatus) | `%APPDATA%\com.rememberkey.app\sync.json` | Nicht verschlüsselt, keine Geheimnisse |
| Google-Anmeldetoken | Windows-Anmeldeinformationsverwaltung | Durch Windows geschützt |
| Dateien für die Browser-Verbindung | `%APPDATA%\com.rememberkey.app\native\` und Registry-Einträge unter `HKCU\Software\…\NativeMessagingHosts` | Erlauben nur die Remember-Key-Erweiterung |

Das Master-Passwort wird nie gespeichert. Kopierte Passwörter werden vom Windows-Zwischenablageverlauf und
der Cloud-Zwischenablage ausgeschlossen und standardmäßig nach 30 Sekunden gelöscht.

### Was deinen Rechner verlassen kann und wann

**Abgleich mit Google Drive (optional, standardmäßig aus).** Verbindest du Google Drive, wird nur die
verschlüsselte Datei `vault.rk` in einen versteckten App-Ordner in deinem eigenen Google Drive
hochgeladen. Die App fordert nur Zugriff auf diesen Ordner an (`drive.appdata`). Ohne dein Master-Passwort
kann Google den Tresor nicht lesen. Für die Daten bei Google gilt die Datenschutzerklärung von Google.

**KI-Zugriff (optional, standardmäßig aus).** Schaltest du den KI-Zugriff ein und gibst einzelne Einträge
frei:

- KI-Assistenten auf deinem Rechner (zum Beispiel Claude) sehen **Titel, Benutzername und erlaubte Hosts**
  der freigegebenen Einträge. Passwörter, API-Tokens, Notizen und 2FA-Schlüssel erhalten sie nie. Was die
  KI sieht, verarbeitet der Anbieter des KI-Assistenten nach seiner eigenen Datenschutzerklärung.
- Bei `http_request` sendet Remember Key deinen API-Token oder dein Passwort an eine Website, **die du für
  diesen Eintrag erlaubt hast**, und erst nachdem du die Anfrage bestätigt hast. Die Antwort der Website
  geht ohne deine Geheimnisse an die KI.
- Bei `fill_login` füllt die Browser-Erweiterung auf einer Website, **die du für diesen Eintrag erlaubt
  hast**, nach deiner Bestätigung Benutzername, Passwort und bei Bedarf den aktuellen 2FA-Code aus und
  sendet das Formular ab. Die Website erhält deine Anmeldung so, als hättest du sie selbst eingetippt.

**Keine weiteren Verbindungen.** Die App kontaktiert weder den Entwickler noch einen anderen Server, sucht
nicht im Hintergrund nach Updates und sendet keine Absturzberichte.

### Die Browser-Erweiterung

Die Erweiterung erhebt keine Daten und stellt selbst keine Verbindungen ins Internet her. Sie spricht nur
mit der Remember-Key-App auf deinem Rechner (Native Messaging). Benutzername, Passwort oder 2FA-Code
erhält sie von der App nur für einen Login, den du gerade bestätigt hast. Sie hält sie nur so lange im
Speicher, bis das Formular ausgefüllt ist, und speichert sie nicht.

Verwendete Berechtigungen:

- **Zugriff auf alle Websites:** um Login-Formulare auf den von dir erlaubten Websites auszufüllen. Andere
  Seiten werden weder gelesen noch verändert.
- **Tabs:** um den Tab zu finden, dessen Adresse zum Eintrag passt.
- **Skripte:** um das Formular in diesem Tab auszufüllen.
- **Native Messaging:** um mit der Remember-Key-App zu sprechen.
- **Alarme:** um sich wieder mit der App zu verbinden, sobald sie gestartet wird.

### Deine Kontrolle

Du kannst den KI-Zugriff und den Abgleich mit Google Drive jederzeit in den Einstellungen der App
ausschalten, Einträge einzeln freigeben oder die Freigabe zurücknehmen und die oben genannten Dateien
löschen. Beim Deinstallieren von App und Erweiterung werden die Programme entfernt. Dein Tresor und die
Dateien unter `%APPDATA%\com.rememberkey.app` bleiben erhalten, bis du sie löschst.

### Kontakt

Fragen zum Datenschutz: <https://github.com/SDureck-group24/Remember-Key/issues>
