#!/usr/bin/env python3
"""Generates a Bitfocus Companion page export (.companionconfig) for SimplePDF.

Creates a "Generic: OSC" connection and one page of buttons. Import it in Companion via
Import / Export -> Import. Tested against Companion 5.1 with generic-osc 3.0.0.

    scripts/companion-page.py --host <simplepdf-pc-ip> [--port 9000] [--feedback-port 9001] [-o out.companionconfig]

Start SimplePDF with `--feedback <companion-ip>:<feedback-port>` for the button highlights.
"""

import argparse
import gzip
import itertools
import json

CONN = "simplepdf"
# generic-osc has 4 upgrade scripts; marking entities as current skips them.
UPGRADE_INDEX = 3

WHITE, BLACK = 0xFFFFFF, 0x000000
GRAY = 0x303030
GREEN = 0x00A000
RED = 0xC00000
BLUE = 0x0060C0
ORANGE = 0xE08000

_ids = (f"spdf{i:03d}" for i in itertools.count())


def opts(**values):
    """Companion 5 stores every entity option as a (non-)expression value."""
    return {k: {"isExpression": False, "value": v} for k, v in values.items()}


def action(definition, **options):
    return {
        "type": "action",
        "id": next(_ids),
        "definitionId": definition,
        "connectionId": CONN,
        "options": opts(**options),
        "upgradeIndex": UPGRADE_INDEX,
    }


def when_int(path, value, bgcolor, color=WHITE):
    return {
        "type": "feedback",
        "id": next(_ids),
        "definitionId": "osc_feedback_int",
        "connectionId": CONN,
        "options": opts(path=path, arguments=value, comparison="equal"),
        "isInverted": False,
        "style": {"bgcolor": bgcolor, "color": color},
        "upgradeIndex": UPGRADE_INDEX,
    }


def when_str(path, value, bgcolor, color=WHITE):
    return {
        "type": "feedback",
        "id": next(_ids),
        "definitionId": "osc_feedback_string",
        "connectionId": CONN,
        "options": opts(path=path, arguments=value, comparison="equal"),
        "isInverted": False,
        "style": {"bgcolor": bgcolor, "color": color},
        "upgradeIndex": UPGRADE_INDEX,
    }


def button(text, actions, feedbacks=(), bgcolor=GRAY, size="auto"):
    """Classic (pre-layered) button; Companion converts it on import."""
    return {
        "type": "button",
        "style": {
            "text": text,
            "size": size,
            "alignment": "center:center",
            "pngalignment": "center:center",
            "color": WHITE,
            "bgcolor": bgcolor,
            "show_topbar": "default",
            "png64": None,
        },
        "options": {"stepProgression": "auto", "rotaryActions": False},
        "feedbacks": list(feedbacks),
        "steps": {"0": {"action_sets": {"down": list(actions), "up": []}, "options": {"runWhileHeld": []}}},
        "localVariables": [],
    }


def blank(path):
    return action("send_blank", path=path)


def build(host, port, feedback_port):
    controls = {}

    def put(row, col, control):
        controls.setdefault(str(row), {})[str(col)] = control

    # Companion's page navigation in the first column.
    put(0, 0, {"type": "pageup"})
    put(1, 0, {"type": "pagenum"})
    put(2, 0, {"type": "pagedown"})

    put(0, 1, button("<\\nZurück", [blank("/pdf/prev")], size="18"))
    put(0, 2, button("Weiter\\n>", [blank("/pdf/next")], size="18"))
    put(0, 3, button("Anfang", [blank("/pdf/first")]))
    put(0, 4, button("Ende", [blank("/pdf/last")]))
    put(0, 5, button("Vollbild", [blank("/pdf/mode/fullscreen")],
                     [when_str("/pdf/status/mode", "fullscreen", GREEN)]))
    put(0, 6, button("Übersicht", [blank("/pdf/mode/overview")],
                     [when_str("/pdf/status/mode", "overview", GREEN)]))
    put(0, 7, button("Schwarz", [blank("/pdf/black/toggle")],
                     [when_int("/pdf/status/black", 1, RED)]))

    for n in range(1, 8):
        put(1, n, button(f"Datei\\n{n}", [action("send_int", path="/pdf/file/select", int=n)],
                         [when_int("/pdf/status/file_index", n, BLUE)]))

    put(2, 1, button("Datei\\n<", [blank("/pdf/file/prev")]))
    put(2, 2, button("Datei\\n>", [blank("/pdf/file/next")]))
    put(2, 3, button("Ordner\\nneu lesen", [blank("/pdf/rescan")]))
    put(2, 4, button("Modus\\nwechseln", [blank("/pdf/mode/toggle")]))
    put(2, 5, button("Ausgabe", [blank("/pdf/output/toggle")],
                     [when_int("/pdf/status/output", 1, GREEN)]))
    put(2, 6, button("Ausgabe\\nAN", [action("send_int", path="/pdf/output", int=1)], bgcolor=0x004000))
    put(2, 7, button("Ausgabe\\nAUS", [action("send_int", path="/pdf/output", int=0)], bgcolor=0x400000))

    return {
        "version": 12,
        "type": "page",
        "companionBuild": "5.1.1",
        "page": {
            "name": "SimplePDF",
            "controls": controls,
            "gridSize": {"minColumn": 0, "maxColumn": 7, "minRow": 0, "maxRow": 3},
            "pageVariables": [],
        },
        "oldPageNumber": 1,
        "instances": {
            CONN: {
                "moduleInstanceType": "connection",
                "moduleId": "generic-osc",
                "moduleVersionId": "3.0.0",
                "updatePolicy": "stable",
                "label": "SimplePDF",
                "sortOrder": 0,
                "enabled": True,
                "lastUpgradeIndex": UPGRADE_INDEX,
                "config": {
                    "host": host,
                    "targetPort": port,
                    "protocol": "udp",
                    "listen": True,
                    "feedbackPort": str(feedback_port),
                },
                "secrets": {},
            }
        },
        "connectionCollections": [],
    }


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--host", required=True, help="IP of the PC running SimplePDF")
    p.add_argument("--port", type=int, default=9000, help="SimplePDF OSC port")
    p.add_argument("--feedback-port", type=int, default=9001, help="port Companion listens on for status")
    p.add_argument("-o", "--output", default="simplepdf.companionconfig")
    args = p.parse_args()

    data = json.dumps(build(args.host, args.port, args.feedback_port), ensure_ascii=False).encode()
    with gzip.open(args.output, "wb") as f:
        f.write(data)
    print(args.output)


if __name__ == "__main__":
    main()
