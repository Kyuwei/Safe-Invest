#!/usr/bin/env python3
"""Generates the partial-class members the WinUI XAML compiler would emit.

The XAML compiler only runs on Windows, so off Windows the page code-behind cannot be
compiled at all: every x:Name field and InitializeComponent() is missing. This recreates
just enough of that generated code to let the C# compiler type-check the code-behind
locally, which turns a three-minute CI round trip into a thirty-second one.

It is a development aid, not a build step: the real generated code always wins on Windows.
"""

from __future__ import annotations

import pathlib
import re
import sys
import xml.etree.ElementTree as ET

X_NAMESPACE = "{http://schemas.microsoft.com/winfx/2006/xaml}"

# Element name -> namespace of the corresponding type.
SHAPES = {"Ellipse", "Polyline", "Polygon", "Rectangle", "Path", "Line"}
PRIMITIVES = {"ToggleButton", "RepeatButton", "RangeBase", "ButtonBase"}


def type_of(tag: str) -> str:
    local = tag.rsplit("}", 1)[-1]
    if local in SHAPES:
        return f"global::Microsoft.UI.Xaml.Shapes.{local}"
    if local in PRIMITIVES:
        return f"global::Microsoft.UI.Xaml.Controls.Primitives.{local}"
    return f"global::Microsoft.UI.Xaml.Controls.{local}"


def stub_for(path: pathlib.Path) -> str | None:
    text = path.read_text(encoding="utf-8")

    root_match = re.search(r"<(\w+)[\s>]", text)
    class_match = re.search(r'x:Class="([\w.]+)"', text)
    if not root_match or not class_match:
        return None

    full_name = class_match.group(1)
    namespace, _, class_name = full_name.rpartition(".")

    try:
        tree = ET.fromstring(text)
    except ET.ParseError as error:
        print(f"  XAML illisible : {path.name} — {error}", file=sys.stderr)
        return None

    fields: list[str] = []
    for element in tree.iter():
        name = element.attrib.get(f"{X_NAMESPACE}Name")
        if name:
            fields.append(f"    internal {type_of(element.tag)} {name} = null!;")

    base = {"Page": "global::Microsoft.UI.Xaml.Controls.Page",
            "Window": "global::Microsoft.UI.Xaml.Window",
            "Application": "global::Microsoft.UI.Xaml.Application"}.get(root_match.group(1))

    body = "\n".join(fields)
    return f"""// Généré pour la vérification de types hors Windows — ne pas committer dans le projet.
namespace {namespace};

partial class {class_name}
{{
{body}

    public void InitializeComponent()
    {{
    }}
}}
"""


def main() -> int:
    source = pathlib.Path(sys.argv[1])
    output = pathlib.Path(sys.argv[2])
    output.mkdir(parents=True, exist_ok=True)

    written = 0
    for xaml in sorted(source.rglob("*.xaml")):
        if xaml.name == "App.xaml":
            # App.xaml declares resources, not named fields; only InitializeComponent.
            stub = f"""namespace SafeInvest.App;

partial class App
{{
    public void InitializeComponent()
    {{
    }}
}}
"""
        else:
            stub = stub_for(xaml)

        if stub is None:
            continue

        (output / f"{xaml.stem}.stub.cs").write_text(stub, encoding="utf-8")
        written += 1

    print(f"{written} stubs générés dans {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
