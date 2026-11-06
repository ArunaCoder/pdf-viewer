// Uma aba de documento: o visualizador do pdf.js, a barra lateral e o painel de anexos.
//
// O pdf.js desenha e busca; o Rust lê os anexos. Os dois leem os mesmos bytes, e por isso
// a senha que o usuário dá ao pdf.js é repassada ao Rust quando o painel pede a lista.

import { AttachmentsPanel } from "./attachments";
import { askPassword, showProperties } from "./dialogs";
import { el } from "./dom";
import { icon } from "./icons";
import {
  type DocumentInfo,
  documentBytes,
  type IpcFailure,
  listAttachments,
  openExternalLink,
  toFailure,
} from "./ipc";
import {
  ASSET_OPTIONS,
  type EventBus,
  type PDFDocumentLoadingTask,
  type PDFDocumentProxy,
  type PDFFindController,
  type PDFLinkService,
  type PDFViewer,
  pdfjsLib,
  viewerLib,
} from "./pdfjs";
import { Sidebar } from "./sidebar";
import { button, toast } from "./ui";

export type FindStatus = "idle" | "found" | "not_found" | "wrapped" | "pending";

export interface ViewState {
  loaded: boolean;
  page: number;
  pages: number;
  scale: number;
  scaleValue: string;
  rotation: number;
  scrollMode: number;
  spreadMode: number;
  sidebarOpen: boolean;
  drawerOpen: boolean;
  nightMode: boolean;
  attachmentCount: number | null;
  attachmentsFailed: boolean;
  find: { status: FindStatus; current: number; total: number };
}

export interface FindOptions {
  caseSensitive: boolean;
  entireWord: boolean;
}

export interface DocumentHost {
  openDocument(info: DocumentInfo): void;
  changed(view: DocumentView): void;
  close(view: DocumentView): void;
}

const FIND_STATUS: Record<number, FindStatus> = {
  [viewerLib.FindState.FOUND]: "found",
  [viewerLib.FindState.NOT_FOUND]: "not_found",
  [viewerLib.FindState.WRAPPED]: "wrapped",
  [viewerLib.FindState.PENDING]: "pending",
};

export class DocumentView {
  readonly root: HTMLElement;
  readonly state: ViewState;
  readonly findOptions: FindOptions = { caseSensitive: false, entireWord: false };
  private readonly container: HTMLDivElement;
  private readonly eventBus: EventBus;
  private readonly linkService: PDFLinkService;
  private readonly findController: PDFFindController;
  private readonly viewer: PDFViewer;
  private readonly panel: AttachmentsPanel;
  private readonly sidebarSlot = el("div", { class: "sidebar-slot" });
  private loadingTask: PDFDocumentLoadingTask | null = null;
  private pdf: PDFDocumentProxy | null = null;
  private sidebar: Sidebar | null = null;
  private password: string | null = null;
  private lastQuery = "";
  private destroyed = false;

  constructor(
    readonly info: DocumentInfo,
    private readonly host: DocumentHost,
  ) {
    this.state = {
      loaded: false,
      page: 1,
      pages: 0,
      scale: 1,
      scaleValue: "auto",
      rotation: 0,
      scrollMode: viewerLib.ScrollMode.VERTICAL,
      spreadMode: viewerLib.SpreadMode.NONE,
      sidebarOpen: window.innerWidth >= 1100,
      drawerOpen: false,
      nightMode: false,
      attachmentCount: null,
      attachmentsFailed: false,
      find: { status: "idle", current: 0, total: 0 },
    };
    const viewerElement = el("div", { class: "pdfViewer" });
    this.container = el(
      "div",
      { class: "viewer-container", attrs: { tabindex: "0", "aria-label": "Documento" } },
      [viewerElement],
    );
    this.panel = new AttachmentsPanel(
      { documentId: info.id, openDocument: (opened) => host.openDocument(opened) },
      () => this.setDrawer(false),
    );
    this.root = el("section", { class: "workspace", dataset: { doc: String(info.id) } }, [
      this.sidebarSlot,
      el("div", { class: "viewer-wrap" }, [this.container]),
      this.panel.root,
    ]);

    this.eventBus = new viewerLib.EventBus();
    this.linkService = new viewerLib.PDFLinkService({
      eventBus: this.eventBus,
      externalLinkTarget: viewerLib.LinkTarget.NONE,
    });
    this.findController = new viewerLib.PDFFindController({
      eventBus: this.eventBus,
      linkService: this.linkService,
      updateMatchesCountOnProgress: true,
    });
    this.viewer = new viewerLib.PDFViewer({
      container: this.container,
      viewer: viewerElement,
      eventBus: this.eventBus,
      linkService: this.linkService,
      findController: this.findController,
      removePageBorders: true,
    });
    this.linkService.setViewer(this.viewer);
    this.wireEvents();
    this.applyLayout();
  }

  // ── Carga ─────────────────────────────────────────────────────────────────

  async load(): Promise<void> {
    try {
      const bytes = await documentBytes(this.info.id);
      if (this.destroyed) return;
      const task = pdfjsLib.getDocument({ data: new Uint8Array(bytes), ...ASSET_OPTIONS });
      this.loadingTask = task;
      task.onPassword = (update: (password: string) => void, reason: number) => {
        const retry = reason === pdfjsLib.PasswordResponses.INCORRECT_PASSWORD;
        void askPassword(this.info.name, retry).then((password) => {
          if (password === null) {
            this.host.close(this);
            return;
          }
          this.password = password;
          update(password);
        });
      };
      const pdf = await task.promise;
      if (this.destroyed) {
        void pdf.loadingTask.destroy();
        return;
      }
      this.pdf = pdf;
      this.viewer.setDocument(pdf);
      this.linkService.setDocument(pdf, null);
      this.state.pages = pdf.numPages;
      this.state.loaded = true;
      this.sidebar = new Sidebar(pdf, {
        goToPage: (page) => this.goToPage(page),
        goToDestination: (dest) => void this.linkService.goToDestination(dest as string),
        openLink: (url) => void this.openLink(url),
      });
      this.sidebarSlot.append(this.sidebar.root);
      this.host.changed(this);
      this.container.focus({ preventScroll: true });
      void this.loadAttachments();
    } catch (error) {
      if (this.destroyed) return;
      this.showLoadError(error);
    }
  }

  private async loadAttachments(): Promise<void> {
    try {
      const listing = await listAttachments(this.info.id, this.password);
      if (this.destroyed) return;
      this.panel.setListing(listing);
      this.state.attachmentCount = listing.attachments.length;
      if (listing.attachments.length > 0) {
        const count = listing.attachments.length;
        toast("info", `${this.info.name} tem ${count === 1 ? "1 anexo" : `${count} anexos`}.`, {
          label: "Ver anexos",
          run: () => this.setDrawer(true),
        });
      }
    } catch (error) {
      if (this.destroyed) return;
      const failure: IpcFailure = toFailure(error);
      this.panel.setFailure(failure);
      this.state.attachmentsFailed = true;
    }
    this.host.changed(this);
  }

  private showLoadError(error: unknown): void {
    const invalid = error instanceof pdfjsLib.InvalidPDFException;
    const message = invalid
      ? "O arquivo está danificado ou não é um PDF válido."
      : error instanceof Error && "code" in error
        ? toFailure(error).message
        : "Não foi possível abrir este documento.";
    this.container.replaceChildren(
      el("div", { class: "load-error" }, [
        el("span", { class: "empty-icon" }, [icon("alert", 30)]),
        el("strong", { text: "Não foi possível abrir" }),
        el("p", { text: message }),
        button({ label: "Fechar aba", variant: "soft", onClick: () => this.host.close(this) }),
      ]),
    );
  }

  // ── Eventos do pdf.js ─────────────────────────────────────────────────────

  private wireEvents(): void {
    const bus = this.eventBus;
    bus.on("pagesinit", () => {
      this.viewer.currentScaleValue = this.state.scaleValue;
    });
    bus.on("pagechanging", ({ pageNumber }: { pageNumber: number }) => {
      this.state.page = pageNumber;
      this.sidebar?.markCurrent(pageNumber);
      this.host.changed(this);
    });
    bus.on("scalechanging", ({ scale, presetValue }: { scale: number; presetValue?: string }) => {
      this.state.scale = scale;
      this.state.scaleValue = presetValue ?? String(scale);
      this.host.changed(this);
    });
    bus.on("rotationchanging", ({ pagesRotation }: { pagesRotation: number }) => {
      this.state.rotation = pagesRotation;
      this.sidebar?.setRotation(pagesRotation);
      this.host.changed(this);
    });
    bus.on("scrollmodechanged", ({ mode }: { mode: number }) => {
      this.state.scrollMode = mode;
      this.host.changed(this);
    });
    bus.on("spreadmodechanged", ({ mode }: { mode: number }) => {
      this.state.spreadMode = mode;
      this.host.changed(this);
    });
    bus.on(
      "updatefindmatchescount",
      ({ matchesCount }: { matchesCount: { current: number; total: number } }) => {
        this.state.find.current = matchesCount.current;
        this.state.find.total = matchesCount.total;
        this.host.changed(this);
      },
    );
    bus.on(
      "updatefindcontrolstate",
      ({
        state,
        matchesCount,
      }: {
        state: number;
        matchesCount: { current: number; total: number };
      }) => {
        this.state.find.status = FIND_STATUS[state] ?? "idle";
        this.state.find.current = matchesCount.current;
        this.state.find.total = matchesCount.total;
        this.host.changed(this);
      },
    );
    // O clipe de papel desenhado na página: o anexo dele está no painel.
    bus.on("fileattachmentannotation", () => this.setDrawer(true));

    // Link para fora do documento: o pdf.js o desenha como `<a href>`, e seguir o href
    // trocaria a janela do app por uma página da web. Ele é entregue ao Rust, que só abre
    // http, https e mailto, no navegador do sistema.
    this.container.addEventListener(
      "click",
      (event) => {
        const anchor = (event.target as Element | null)?.closest?.("a[href]");
        if (!(anchor instanceof HTMLAnchorElement)) return;
        const url = safeUrl(anchor.href);
        if (url === null || url.origin === window.location.origin) return;
        event.preventDefault();
        event.stopPropagation();
        void this.openLink(anchor.href);
      },
      true,
    );
    this.container.addEventListener(
      "wheel",
      (event) => {
        if (!event.ctrlKey) return;
        event.preventDefault();
        const origin = [event.clientX, event.clientY];
        if (event.deltaY < 0) this.viewer.increaseScale({ drawingDelay: 300, origin });
        else this.viewer.decreaseScale({ drawingDelay: 300, origin });
      },
      { passive: false },
    );
  }

  private async openLink(url: string): Promise<void> {
    try {
      await openExternalLink(url);
    } catch (error) {
      toast("warning", toFailure(error).message);
    }
  }

  // ── Comandos da barra ─────────────────────────────────────────────────────

  goToPage(page: number): void {
    if (!this.state.loaded) return;
    const clamped = Math.min(Math.max(1, Math.round(page)), this.state.pages);
    this.viewer.currentPageNumber = clamped;
  }

  nextPage(): void {
    this.viewer.nextPage();
  }

  previousPage(): void {
    this.viewer.previousPage();
  }

  zoomIn(): void {
    this.viewer.increaseScale();
  }

  zoomOut(): void {
    this.viewer.decreaseScale();
  }

  setScale(value: string): void {
    this.viewer.currentScaleValue = value;
  }

  rotate(delta: 90 | -90): void {
    this.viewer.pagesRotation = (this.viewer.pagesRotation + delta + 360) % 360;
  }

  setScrollMode(mode: number): void {
    this.viewer.scrollMode = mode;
  }

  setSpreadMode(mode: number): void {
    this.viewer.spreadMode = mode;
  }

  find(query: string, options: { again: boolean; previous: boolean }): void {
    const trimmed = query.trim();
    if (trimmed.length === 0) {
      this.clearFind();
      return;
    }
    const again = options.again && trimmed === this.lastQuery;
    this.lastQuery = trimmed;
    this.eventBus.dispatch("find", {
      source: this,
      type: again ? "again" : "",
      query: trimmed,
      caseSensitive: this.findOptions.caseSensitive,
      entireWord: this.findOptions.entireWord,
      highlightAll: true,
      findPrevious: options.previous,
      matchDiacritics: false,
    });
  }

  clearFind(): void {
    this.lastQuery = "";
    this.eventBus.dispatch("findbarclose", { source: this });
    this.state.find = { status: "idle", current: 0, total: 0 };
    this.host.changed(this);
  }

  setSidebar(open: boolean): void {
    this.state.sidebarOpen = open;
    this.applyLayout();
  }

  setDrawer(open: boolean): void {
    this.state.drawerOpen = open;
    this.applyLayout();
  }

  setNightMode(on: boolean): void {
    this.state.nightMode = on;
    this.applyLayout();
  }

  private applyLayout(): void {
    this.root.classList.toggle("sidebar-open", this.state.sidebarOpen);
    this.root.classList.toggle("drawer-open", this.state.drawerOpen);
    this.root.classList.toggle("night", this.state.nightMode);
    this.host.changed(this);
  }

  async properties(): Promise<void> {
    if (this.pdf) await showProperties(this.pdf, this.info.name, this.info.path, this.info.size);
  }

  focus(): void {
    this.container.focus({ preventScroll: true });
  }

  /** Libera o pdf.js (worker, páginas, fontes) e os bytes do lado Rust. */
  async destroy(): Promise<void> {
    this.destroyed = true;
    this.sidebar?.destroy();
    this.viewer.cleanup();
    this.viewer.setDocument(null);
    this.linkService.setDocument(null);
    // No pdf.js 6 o documento é liberado pela tarefa que o carregou: worker, páginas e
    // fontes vão juntos.
    await this.loadingTask?.destroy();
    this.pdf = null;
    this.root.remove();
  }
}

function safeUrl(href: string): URL | null {
  try {
    return new URL(href);
  } catch {
    return null;
  }
}
