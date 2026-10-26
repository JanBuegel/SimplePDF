# Changelog

## 0.1.5 – 2026-10-26

- Bildanpassung pro PDF: Ränder links/rechts/oben/unten beschneiden, A4-quer-auf-16:9-Vorgabe und optional auf die Ausgabe strecken. Vorschau und Ausgabe zeigen dieselbe Anpassung. Einstellungen werden während der Sitzung pro Datei gemerkt.
- Endlose Neuzeichenschleife unter Windows behoben: Die Vorschau aktualisiert sich auch bei fokussierter Vollbildausgabe.

## 0.1.3 – 2026-10-26

- Neu gebaut aus dem aktuellen Stand. Der Installer von 0.1.2 enthielt die Fixes vom Schluss möglicherweise nicht vollständig. Ein installiertes 0.1.2 wird damit sauber aktualisiert.

## 0.1.2 – 2026-10-25

- **Vorschau bleibt nicht mehr stehen:** Wenn Windows ein Bild verwirft (z. B. nach dem Maximieren oder wenn ein anderes Fenster auf einem anderen Monitor in den Vollbildmodus geht), zeichnet SimplePDF nach 100 ms neu. Das passiert auch, wenn ein Fenster wieder sichtbar wird oder den Fokus bekommt. Vorher zeigte die Vorschau nach Seitenwechseln per OSC oder Clicker teils den alten Stand.
- **Vorschau im Seitenverhältnis der Ausgabe:** Die Vorschau hat das Seitenverhältnis des Ausgabe-Monitors (16:9, wenn keiner bekannt ist), auch im maximierten Fenster. Die Vorschau der nächsten Seite nutzt die restliche Breite.
- **Skript `package-windows.ps1`:** Es bricht nicht mehr ab, wenn die Ausgabe umgeleitet wird.

## 0.1.1 – 2026-10-25

- **Clicker global (Windows):** Solange die Ausgabe läuft, blättern Bild auf/ab und ←/→ auch dann, wenn ein anderes Programm den Fokus hat. Das lässt sich über die Checkbox „Clicker global“ abschalten. Sind Tasten schon von einem anderen Programm belegt, zeigt die Leiste „nur 3/4 Tasten frei“.
- **Ausgabe bleibt sichtbar:** Liegt die Ausgabe auf einem anderen Monitor als das Steuerfenster, bleibt sie im Vordergrund.
- **Versionsnummer** im Fenstertitel.
- **Installer:** Velopack-Installer (`SimplePDF-win-Setup.exe`) und eine Portable-Version. Die Installation läuft pro Benutzer, ohne Adminrechte, und legt Verknüpfungen im Startmenü und auf dem Desktop an.

## 0.1.0 – 2026-10-25

- Erste Version: Steuerfenster mit Dateiliste, Live-Vorschau, nächster Seite und Seitenraster. Ausgabe im Vollbild mit den Modi Vollbild und Übersicht, Schwarzbild.
- Steuerung per Tastatur, Presenter-Clicker und OSC (UDP 9000), mit Status-Feedback an Companion (`--feedback`).
- PDF oder Ordner per Drag & Drop öffnen.
