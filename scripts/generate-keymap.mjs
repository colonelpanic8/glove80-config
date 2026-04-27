import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const layoutPath = path.join(repoRoot, "config", "moergo-layout.json");
const outputPath = path.join(repoRoot, "config", "glove80.keymap");

const layout = JSON.parse(fs.readFileSync(layoutPath, "utf8"));
const rowLengths = [10, 12, 12, 12, 18, 16];

const layerNames = layout.layer_names.map(sanitizeLayerName);
const layerIdByIndex = new Map(layerNames.map((name, index) => [index, `LAYER_${name}`]));

const rgbLayerMaps = {
  Games: new Map([
    [24, "BLUE"],
    [35, "BLUE"],
    [36, "BLUE"],
    [37, "BLUE"],
    [69, "BLUE"],
  ]),
  Mac_Hyper: new Map([[72, "RED"]]),
};

function sanitizeLayerName(name) {
  const sanitized = String(name).replace(/[^A-Za-z0-9_]/g, "_");
  return /^[A-Za-z_]/.test(sanitized) ? sanitized : `L_${sanitized}`;
}

function formatParam(param) {
  const value = String(param.value);
  if (param.params?.length) {
    return `${value}(${param.params.map(formatParam).join(", ")})`;
  }
  if (/^\d+$/.test(value) && layerIdByIndex.has(Number(value))) {
    return layerIdByIndex.get(Number(value));
  }
  return value;
}

function formatBinding(binding) {
  const value = binding.value;
  if (value === "&magic") {
    return "&magic LAYER_Magic 0";
  }
  if (!binding.params?.length) {
    return value;
  }
  return [value, ...binding.params.map(formatParam)].join(" ");
}

function formatRows(items, indent = "                ") {
  const rows = [];
  let offset = 0;
  for (const length of rowLengths) {
    const row = items.slice(offset, offset + length);
    rows.push(`${indent}${row.join("  ")}`);
    offset += length;
  }
  return rows.join("\n");
}

function layerBlock(name, index, bindings) {
  return `        layer_${name} {
            bindings = <
${formatRows(bindings.map(formatBinding))}
            >;
        };`;
}

function rgbBinding(color) {
  return `&ug ${color ?? "___"}`;
}

function rgbLayerBlock(name, colors) {
  const bindings = Array.from({ length: 80 }, (_, index) => rgbBinding(colors.get(index)));
  return `        ${name.toLowerCase()} {
            bindings = <
${formatRows(bindings)}
            >;
            layer-id = <LAYER_${name}>;
        };`;
}

function generatedLayerDefines() {
  return layerNames.map((name, index) => `#define LAYER_${name} ${index}`).join("\n");
}

function generatedRgbLayers() {
  return Object.entries(rgbLayerMaps)
    .map(([name, colors]) => rgbLayerBlock(name, colors))
    .join("\n\n");
}

function generatedKeymapLayers() {
  return layout.layers
    .map((bindings, index) => layerBlock(layerNames[index], index, bindings))
    .join("\n\n");
}

const output = `/*
 * Copyright (c) 2020 The ZMK Contributors
 * Copyright (c) 2023 Innaworks Development Limited, trading as MoErgo
 *
 * SPDX-License-Identifier: MIT
 */

/* Generated from config/moergo-layout.json by scripts/generate-keymap.mjs. */

#include <behaviors.dtsi>
#include <dt-bindings/zmk/outputs.h>
#include <dt-bindings/zmk/keys.h>
#include <dt-bindings/zmk/bt.h>
#include <dt-bindings/zmk/rgb.h>
#include <dt-bindings/zmk/rgb_colors.h>

${generatedLayerDefines()}

#ifndef LAYER_Lower
#define LAYER_Lower 0
#endif

/ {
    underglow-layer {
        compatible = "zmk,underglow-layer";

${generatedRgbLayers()}
    };
};

/ {
    macros {
        rgb_ug_status_macro: rgb_ug_status_macro {
            label = "RGB_UG_STATUS";
            compatible = "zmk,behavior-macro";
            #binding-cells = <0>;
            bindings = <&rgb_ug RGB_STATUS>;
        };
    };
};

/ {
#ifdef BT_DISC_CMD
    behaviors {
        bt_0: bt_0 {
            compatible = "zmk,behavior-tap-dance";
            label = "BT_0";
            #binding-cells = <0>;
            tapping-term-ms = <200>;
            bindings = <&bt_select_0>, <&bt BT_DISC 0>;
        };
        bt_1: bt_1 {
            compatible = "zmk,behavior-tap-dance";
            label = "BT_1";
            #binding-cells = <0>;
            tapping-term-ms = <200>;
            bindings = <&bt_select_1>, <&bt BT_DISC 1>;
        };
        bt_2: bt_2 {
            compatible = "zmk,behavior-tap-dance";
            label = "BT_2";
            #binding-cells = <0>;
            tapping-term-ms = <200>;
            bindings = <&bt_select_2>, <&bt BT_DISC 2>;
        };
        bt_3: bt_3 {
            compatible = "zmk,behavior-tap-dance";
            label = "BT_3";
            #binding-cells = <0>;
            tapping-term-ms = <200>;
            bindings = <&bt_select_3>, <&bt BT_DISC 3>;
        };
    };
    macros {
        bt_select_0: bt_select_0 {
            label = "BT_SELECT_0";
            compatible = "zmk,behavior-macro";
            #binding-cells = <0>;
            bindings = <&out OUT_BLE>, <&bt BT_SEL 0>;
        };
        bt_select_1: bt_select_1 {
            label = "BT_SELECT_1";
            compatible = "zmk,behavior-macro";
            #binding-cells = <0>;
            bindings = <&out OUT_BLE>, <&bt BT_SEL 1>;
        };
        bt_select_2: bt_select_2 {
            label = "BT_SELECT_2";
            compatible = "zmk,behavior-macro";
            #binding-cells = <0>;
            bindings = <&out OUT_BLE>, <&bt BT_SEL 2>;
        };
        bt_select_3: bt_select_3 {
            label = "BT_SELECT_3";
            compatible = "zmk,behavior-macro";
            #binding-cells = <0>;
            bindings = <&out OUT_BLE>, <&bt BT_SEL 3>;
        };
    };
#else
    macros {
        bt_0: bt_0 {
            label = "BT_0";
            compatible = "zmk,behavior-macro";
            #binding-cells = <0>;
            bindings = <&out OUT_BLE>, <&bt BT_SEL 0>;
        };
        bt_1: bt_1 {
            label = "BT_1";
            compatible = "zmk,behavior-macro";
            #binding-cells = <0>;
            bindings = <&out OUT_BLE>, <&bt BT_SEL 1>;
        };
        bt_2: bt_2 {
            label = "BT_2";
            compatible = "zmk,behavior-macro";
            #binding-cells = <0>;
            bindings = <&out OUT_BLE>, <&bt BT_SEL 2>;
        };
        bt_3: bt_3 {
            label = "BT_3";
            compatible = "zmk,behavior-macro";
            #binding-cells = <0>;
            bindings = <&out OUT_BLE>, <&bt BT_SEL 3>;
        };
    };
#endif
};

/ {
    behaviors {
        magic: magic {
            compatible = "zmk,behavior-hold-tap";
            label = "MAGIC_HOLD_TAP";
            #binding-cells = <2>;
            flavor = "tap-preferred";
            tapping-term-ms = <200>;
            bindings = <&mo>, <&rgb_ug_status_macro>;
        };
    };
};

/ {
    keymap {
        compatible = "zmk,keymap";

${generatedKeymapLayers()}
    };
};
`;

fs.writeFileSync(outputPath, output);
