# Meshyard testen

Das Paket `Meshyard_0.2.0_aarch64.dmg` liegt neben dieser Anleitung.
DMG-Prüfsumme, schreibgeschütztes Einhängen und die strenge Signaturprüfung
der kopierten App wurden geprüft. Der erste produktive Keychain-Paketstart
bleibt der unten beschriebene persönliche Checkpoint.

1. DMG öffnen, Meshyard nach `/Applications` ziehen, DMG auswerfen.
2. Meshyard in Programme mit Rechtsklick → Öffnen starten. Bei einer Quarantäne-
   Sperre nur für das bewusst geprüfte lokale Paket bei Bedarf im Terminal
   `xattr -dr com.apple.quarantine /Applications/Meshyard.app` ausführen.
   Ad-hoc signierte Pakete sind nicht notarisiert; eine Gatekeeper-Ablehnung ist erwartet.
3. Workspace und Anzeigenamen anlegen. Die App verwendet standardmäßig das
   Tauri-App-Datenverzeichnis und den macOS-Schlüsselbund. Eventuelle Zugriffsfragen
   prüfen und freigeben; bei gesperrtem Schlüsselbund entsperren und neu starten.
   Es gibt keinen stillen Wechsel auf eine Schlüsseldatei.
4. Einen Claude-Code- oder Codex-Agent registrieren. **Connect agents** öffnen,
   Agent-Typ wählen, eigenen Agent auswählen und **Review settings** drücken.
5. Den vollständigen Hook-Diff und die Pfade prüfen. Erst **Install** schreibt.
   Vorhandene Einstellungen werden gesichert; bei Änderungen seit der Vorschau
   muss eine neue Vorschau erstellt werden.
6. In Codex `/hooks` öffnen und die genauen neuen Meshyard-Hooks prüfen und als
   vertrauenswürdig bestätigen. Nach Änderungen erneut bestätigen. Projekt-Hooks
   brauchen zusätzlich ein vertrauenswürdiges Projekt. CLI neu starten.
7. Eine Nachricht senden, eine Datei bearbeiten oder Tests ausführen. Meshyard
   offen lassen. Registrierung allein ist keine Verbindung: erst ein empfangenes
   Ereignis erzeugt Status und Herkunft. Namen im Yard anklicken oder in der Liste
   den Agent-Namen wählen, um den Inspector zu öffnen.
8. Anzeigenamen und Datenschutzhinweise unter **Settings and members** prüfen.
9. Zum Entfernen **Connect agents** → Typ wählen → **Review hook removal** →
   Diff prüfen → **Remove hook**. Für beide Typen wiederholen, bevor die App gelöscht wird.

Claude-Konfiguration: `~/.claude/settings.json`; Codex: `~/.codex/hooks.json`.
Sicherungen bleiben als `*.meshyard-backup-*` erhalten. Neuere fremde Änderungen
nicht mit einer alten Sicherung überschreiben.

„Verified“ bedeutet ein Hook-Ereignis desselben Betriebssystem-Benutzers, keinen
Nachweis erfolgreicher Arbeit. „Inferred“ ist eine Tool-/Befehlsklassifikation.
Review, Fix und Waiting sind nicht unterstützt. Codex-Modellfehler werden nicht
als belegter StopFailure ausgegeben. Ohne Ereignisse erscheint kein erfundener
Status. Aktive Zustände werden nach zehn Minuten als unbekannt markiert;
Finished geht nach einer Minute auf Idle. P2P-Pairing ist noch nicht verfügbar.

Bei Problemen: ursprüngliche Identität behalten, Schlüsselbund entsperren,
CLI-Hook-Vertrauen kontrollieren, die App aus Programme neu öffnen. Fehler beim
Hook-JSON lassen die Datei unverändert. „DataDirectoryBusy“ bedeutet, dass eine
zweite Instanz dasselbe Profil benutzt. Grafikfehler verdecken die Agent-Liste nicht.

Entwicklungstests benutzen ausschließlich temporäre HOME-/Projekt-/Profildaten im
APFS-Image; reale Agent-Konfigurationen werden dabei nicht verändert.

## Schlüsselbund: noch offener Paket-Checkpoint

Der isolierte Test mit echtem HOME hat einen zufällig benannten Testeintrag
erstellt, gelesen und gelöscht (exit 0, 3,297 s). Er prüft nicht den ersten Start
des Pakets mit dem produktiven Dienst. Diesen Start führt Dion selbst durch:
App aus Programme ohne Testvariablen öffnen, Workspace anlegen und einen
eventuellen Schlüsselbunddialog prüfen und „Immer erlauben“ wählen. Ad-hoc-
Signaturen ändern sich mit jedem Build; macOS kann nach Updates erneut fragen.
Dieser produktive Erststart ist noch nicht validiert. Keine Identität löschen.

## Tastatur und Bewegung

Mit Tab zu den Agent-Namen in der Liste gehen und Enter drücken. Der Inspector
zeigt dieselben Zustände wie der Yard; Schließen gibt den Fokus zurück. Bei
„Bewegung reduzieren“ springen Avatare direkt zur echten Zielstation. Review,
Fix und Waiting sind sichtbar reserviert und melden keine erfundene Aktivität.
Browser-Screenshots verwenden isolierte simulierte IPC-Daten und sind kein
Nachweis echter CLI-Ereignisse oder nativer Paketdarstellung.

## Ansichten

**Yard**, **Agent list** und **Settings** sind getrennte, per Tab/Enter erreichbare
Ansichten. Im Yard stehen auch Review, Fix und Waiting als schraffierte, noch
nicht von Hooks gemeldete Plätze. Die Karte bleibt bei drei Agenten an drei
verschiedenen Stationen eine einzige Karte. Nur tatsächlich volle Stationen
erhalten zusätzliche, mit „+“ bezeichnete Plätze; die Liste zeigt alle Namen.
Der Inspector erscheint als Dialog direkt im Fenster; Escape oder Schließen
gibt den Fokus zurück. In Settings steht die Profilbearbeitung oben.
Beim Installieren bleibt nur der genaue Diff scrollbar; Install/Cancel/Close
bleiben separat erreichbar. Die technische Basishash-Angabe ist aufklappbar.
