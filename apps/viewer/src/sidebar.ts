// A barra lateral do documento: miniaturas das páginas e o sumário (outline) do PDF.
//
// As miniaturas são desenhadas sob demanda — só as que entram na área visível da barra,
// uma de cada vez —, porque um PDF de mil páginas não pode desenhar mil canvases ao abrir.

import { clear, el } from "./dom";
import { icon } from "./icons";
import type { PDFDocumentProxy } from "./pdfjs";

const THUMB_WIDTH = 148;

export interface SidebarHost {
  goToPage(page: number): void;
  goToDestination(dest: unknown): void;
  openLink(url: string): void;
}

interface OutlineItem {
  title: string;
  dest: unknown;
  url: string | null;
  bold?: boolean;
  italic?: boolean;
  items: OutlineItem[];
}

export class Sidebar {
  readonly root: HTMLElement;
  private readonly thumbs = el("div", {
    class: "thumb-list",
    attrs: { role: "listbox", "aria-label": "Páginas" },
  });
  private readonly outline = el("div", { class: "outline", attrs: { role: "tree" } });
  private readonly tabs: Record<"pages" | "outline", HTMLButtonElement>;
  private observer: IntersectionObserver | null = null;
  private readonly queue: number[] = [];
  private rendering = false;
  private rotation = 0;
  private current = 1;
  private destroyed = false;

  constructor(
    private readonly pdf: PDFDocumentProxy,
    private readonly host: SidebarHost,
  ) {
    this.tabs = {
      pages: this.tabButton("pages", "Páginas", "layers"),
      outline: this.tabButton("outline", "Sumário", "list"),
    };
    this.root = el("aside", { class: "sidebar", attrs: { "aria-label": "Navegação" } }, [
      el("div", { class: "segmented", attrs: { role: "tablist" } }, [
        this.tabs.pages,
        this.tabs.outline,
      ]),
      el("div", { class: "sidebar-body" }, [this.thumbs, this.outline]),
    ]);
    this.show("pages");
    this.buildThumbnails();
    void this.buildOutline();
  }

  private tabButton(
    which: "pages" | "outline",
    label: string,
    iconName: "layers" | "list",
  ): HTMLButtonElement {
    return el(
      "button",
      {
        class: "segment",
        attrs: { type: "button", role: "tab" },
        on: { click: () => this.show(which) },
      },
      [icon(iconName, 15), el("span", { text: label })],
    );
  }

  private show(which: "pages" | "outline"): void {
    for (const [key, tab] of Object.entries(this.tabs)) {
      const active = key === which;
      tab.classList.toggle("active", active);
      tab.setAttribute("aria-selected", String(active));
    }
    this.thumbs.hidden = which !== "pages";
    this.outline.hidden = which !== "outline";
  }

  // ── Miniaturas ────────────────────────────────────────────────────────────

  private buildThumbnails(): void {
    this.observer?.disconnect();
    clear(this.thumbs);
    this.queue.length = 0;
    this.observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const page = Number((entry.target as HTMLElement).dataset.page);
          this.observer?.unobserve(entry.target);
          this.enqueue(page);
        }
      },
      { root: this.root.querySelector(".sidebar-body"), rootMargin: "400px 0px" },
    );
    for (let page = 1; page <= this.pdf.numPages; page += 1) {
      const frame = el("div", { class: "thumb-frame" });
      const item = el(
        "button",
        {
          class: "thumb",
          dataset: { page: String(page) },
          attrs: { type: "button", role: "option", "aria-label": `Página ${page}` },
          on: { click: () => this.host.goToPage(page) },
        },
        [frame, el("span", { class: "thumb-label", text: String(page) })],
      );
      this.thumbs.append(item);
      this.observer.observe(item);
    }
    this.markCurrent(this.current);
  }

  private enqueue(page: number): void {
    this.queue.push(page);
    void this.drain();
  }

  /** Desenha as miniaturas da fila, uma por vez, para não disputar o worker com a página. */
  private async drain(): Promise<void> {
    if (this.rendering) return;
    this.rendering = true;
    try {
      for (let page = this.queue.shift(); page !== undefined; page = this.queue.shift()) {
        if (this.destroyed) return;
        await this.renderThumbnail(page);
      }
    } finally {
      this.rendering = false;
    }
  }

  private async renderThumbnail(pageNumber: number): Promise<void> {
    const item = this.thumbs.querySelector<HTMLElement>(`[data-page="${pageNumber}"]`);
    const frame = item?.querySelector<HTMLElement>(".thumb-frame");
    if (!frame) return;
    try {
      const page = await this.pdf.getPage(pageNumber);
      const unit = page.getViewport({ scale: 1, rotation: page.rotate + this.rotation });
      const ratio = window.devicePixelRatio || 1;
      const viewport = page.getViewport({
        scale: (THUMB_WIDTH / unit.width) * ratio,
        rotation: page.rotate + this.rotation,
      });
      const canvas = el("canvas");
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      canvas.style.width = `${THUMB_WIDTH}px`;
      canvas.style.height = `${Math.floor(viewport.height / ratio)}px`;
      await page.render({ canvas, viewport }).promise;
      if (this.destroyed) return;
      clear(frame);
      frame.append(canvas);
      frame.classList.add("ready");
    } catch {
      // Página que não desenha fica com o quadro vazio; o visualizador mostra o erro dela.
      frame.classList.add("failed");
    }
  }

  /** Destaca a página atual e a traz para a área visível da barra. */
  markCurrent(page: number): void {
    this.current = page;
    for (const node of this.thumbs.querySelectorAll(".thumb.current")) {
      node.classList.remove("current");
      node.setAttribute("aria-selected", "false");
    }
    const item = this.thumbs.querySelector<HTMLElement>(`[data-page="${page}"]`);
    if (item) {
      item.classList.add("current");
      item.setAttribute("aria-selected", "true");
      if (!this.thumbs.hidden) item.scrollIntoView({ block: "nearest" });
    }
  }

  /** As miniaturas acompanham a rotação do documento. */
  setRotation(rotation: number): void {
    if (rotation === this.rotation) return;
    this.rotation = rotation;
    this.buildThumbnails();
  }

  // ── Sumário ───────────────────────────────────────────────────────────────

  private async buildOutline(): Promise<void> {
    const items = (await this.pdf.getOutline()) as OutlineItem[] | null;
    if (this.destroyed) return;
    if (!items || items.length === 0) {
      this.outline.append(
        el("div", { class: "empty-note" }, [
          icon("list", 22),
          el("p", { text: "Este PDF não tem sumário." }),
        ]),
      );
      return;
    }
    this.outline.append(this.outlineLevel(items, 0));
  }

  private outlineLevel(items: OutlineItem[], depth: number): HTMLElement {
    const group = el("div", {
      class: "outline-group",
      attrs: { role: depth === 0 ? "presentation" : "group" },
    });
    for (const item of items) {
      const children = item.items.length > 0 ? this.outlineLevel(item.items, depth + 1) : null;
      // Os níveis de cima abertos, os de baixo fechados — como os leitores costumam fazer.
      if (children) children.hidden = depth >= 1;
      group.append(this.outlineRow(item, children));
      if (children) group.append(children);
    }
    return group;
  }

  private outlineRow(item: OutlineItem, children: HTMLElement | null): HTMLElement {
    const classes = ["outline-title"];
    if (item.bold) classes.push("bold");
    if (item.italic) classes.push("italic");
    return el(
      "div",
      {
        class: "outline-item",
        attrs: { role: "treeitem", tabindex: "0" },
        title: item.title,
        on: {
          click: () => this.follow(item),
          keydown: (event) => {
            if (event.key === "Enter") this.follow(item);
          },
        },
      },
      [
        children ? outlineToggle(children) : el("span", { class: "outline-spacer" }),
        el("span", { class: classes.join(" "), text: item.title }),
      ],
    );
  }

  private follow(item: OutlineItem): void {
    if (item.dest !== null && item.dest !== undefined) {
      this.host.goToDestination(item.dest);
    } else if (item.url) {
      this.host.openLink(item.url);
    }
  }

  destroy(): void {
    this.destroyed = true;
    this.observer?.disconnect();
    this.queue.length = 0;
  }
}

/** O botão que abre e fecha o nível `children` do sumário. */
function outlineToggle(children: HTMLElement): HTMLButtonElement {
  const toggle = el(
    "button",
    {
      class: children.hidden ? "outline-toggle" : "outline-toggle open",
      attrs: {
        type: "button",
        "aria-label": "Expandir",
        "aria-expanded": String(!children.hidden),
      },
      on: {
        click: (event) => {
          event.stopPropagation();
          children.hidden = !children.hidden;
          toggle.classList.toggle("open", !children.hidden);
          toggle.setAttribute("aria-expanded", String(!children.hidden));
        },
      },
    },
    [icon("chevron-right", 14)],
  );
  return toggle;
}
