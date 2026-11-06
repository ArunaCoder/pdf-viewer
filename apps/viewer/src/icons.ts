// Ícones de traço em grade 24×24, desenhados como SVG pelo DOM (sem `innerHTML`). A cor é
// `currentColor`: o ícone herda a do botão, e o tema muda os dois juntos.

const SVG_NS = "http://www.w3.org/2000/svg";

type Shape =
  | { d: string }
  | { circle: [number, number, number] }
  | { rect: [number, number, number, number, number] };

const ICONS = {
  sidebar: [{ rect: [3, 4, 18, 16, 2.5] }, { d: "M9 4v16" }],
  "chevron-left": [{ d: "m15 18-6-6 6-6" }],
  "chevron-right": [{ d: "m9 18 6-6-6-6" }],
  "chevron-down": [{ d: "m6 9 6 6 6-6" }],
  "chevron-up": [{ d: "m18 15-6-6-6 6" }],
  "page-first": [{ d: "m17 18-6-6 6-6" }, { d: "M7 6v12" }],
  "page-last": [{ d: "m7 18 6-6-6-6" }, { d: "M17 6v12" }],
  plus: [{ d: "M12 5v14M5 12h14" }],
  minus: [{ d: "M5 12h14" }],
  x: [{ d: "M18 6 6 18M6 6l12 12" }],
  "rotate-ccw": [{ d: "M3 12a9 9 0 1 0 3-6.7L3 8" }, { d: "M3 3v5h5" }],
  "rotate-cw": [{ d: "M21 12a9 9 0 1 1-3-6.7L21 8" }, { d: "M21 3v5h-5" }],
  search: [{ circle: [11, 11, 7] }, { d: "m20 20-3.5-3.5" }],
  paperclip: [
    {
      d: "m21.4 11.6-9.2 9.2a6 6 0 0 1-8.5-8.5l9.2-9.2a4 4 0 0 1 5.7 5.7l-9.2 9.2a2 2 0 0 1-2.8-2.8l8.5-8.5",
    },
  ],
  more: [{ circle: [5, 12, 1.2] }, { circle: [12, 12, 1.2] }, { circle: [19, 12, 1.2] }],
  "folder-open": [
    {
      d: "M6 14 7.5 11.1A2 2 0 0 1 9.3 10H20a2 2 0 0 1 1.9 2.5l-1.5 6A2 2 0 0 1 18.4 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.7.9l.8 1.2a2 2 0 0 0 1.7.9H18a2 2 0 0 1 2 2v2",
    },
  ],
  file: [{ d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }, { d: "M14 2v6h6" }],
  "file-text": [
    { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" },
    { d: "M14 2v6h6M8 13h8M8 17h6" },
  ],
  "file-pdf": [
    { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" },
    { d: "M14 2v6h6" },
    { d: "M8 16v-4h1.5a1.3 1.3 0 0 1 0 2.6H8M13 12v4h1a2 2 0 0 0 0-4z" },
  ],
  image: [
    { rect: [3, 3, 18, 18, 2.5] },
    { circle: [9, 9, 2] },
    { d: "m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21" },
  ],
  archive: [
    { rect: [2, 3, 20, 5, 1.5] },
    { d: "M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8M10 12h4" },
  ],
  download: [{ d: "M12 3v12m-5-5 5 5 5-5M5 21h14" }],
  external: [{ d: "M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" }],
  eye: [{ d: "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z" }, { circle: [12, 12, 3] }],
  "shield-check": [{ d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" }, { d: "m9 12 2 2 4-4" }],
  "shield-alert": [
    { d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" },
    { d: "M12 8v4M12 16h.01" },
  ],
  shield: [{ d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" }],
  sun: [
    { circle: [12, 12, 4] },
    {
      d: "M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4",
    },
  ],
  moon: [{ d: "M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" }],
  maximize: [
    {
      d: "M8 3H5a2 2 0 0 0-2 2v3M21 8V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3M16 21h3a2 2 0 0 0 2-2v-3",
    },
  ],
  info: [{ circle: [12, 12, 9.5] }, { d: "M12 16v-4M12 8h.01" }],
  lock: [{ rect: [4, 11, 16, 10, 2] }, { d: "M8 11V7a4 4 0 0 1 8 0v4" }],
  alert: [
    { d: "M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z" },
    { d: "M12 9v4M12 17h.01" },
  ],
  check: [{ d: "M20 6 9 17l-5-5" }],
  layers: [{ rect: [4, 3, 16, 7, 1.5] }, { rect: [4, 14, 16, 7, 1.5] }],
  list: [{ d: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" }],
  clock: [{ circle: [12, 12, 9.5] }, { d: "M12 7v5l3 2" }],
  trash: [
    {
      d: "M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6",
    },
  ],
  "fit-width": [{ d: "M3 12h18M7 8l-4 4 4 4M17 8l4 4-4 4" }],
  "fit-page": [{ rect: [5, 3, 14, 18, 2] }, { d: "M9 8h6M9 12h6" }],
  "arrow-up-right": [{ d: "M7 17 17 7M8 7h9v9" }],
  sparkle: [{ d: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z" }],
  printer: [
    { d: "M6 9V2h12v7M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2" },
    { rect: [6, 14, 12, 8, 1] },
  ],
} satisfies Record<string, readonly Shape[]>;

export type IconName = keyof typeof ICONS;

/** O ícone `name`, no tamanho `size` (px). */
export function icon(name: IconName, size = 18): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.9");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("icon");
  for (const shape of ICONS[name] as readonly Shape[]) {
    svg.appendChild(draw(shape));
  }
  return svg;
}

function draw(shape: Shape): SVGElement {
  if ("d" in shape) {
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", shape.d);
    return path;
  }
  if ("circle" in shape) {
    const [cx, cy, r] = shape.circle;
    const circle = document.createElementNS(SVG_NS, "circle");
    circle.setAttribute("cx", String(cx));
    circle.setAttribute("cy", String(cy));
    circle.setAttribute("r", String(r));
    return circle;
  }
  const [x, y, width, height, rx] = shape.rect;
  const rect = document.createElementNS(SVG_NS, "rect");
  rect.setAttribute("x", String(x));
  rect.setAttribute("y", String(y));
  rect.setAttribute("width", String(width));
  rect.setAttribute("height", String(height));
  rect.setAttribute("rx", String(rx));
  return rect;
}
