// A casca do webview: abas, tela inicial, barra de ferramentas, atalhos e a chegada de
// documentos — pelo seletor, pelos recentes e pelo que o Rust entrega (linha de comando,
// arrastar e soltar, segunda instância).

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { DocumentView } from "./document-view";
import { clear, el } from "./dom";
import { icon } from "./icons";
import {
  clearRecent,
  closeDocument,
  type DocumentInfo,
  type OpenOutcome,
  onDocumentsOpened,
  openRecent,
  pickDocuments,
  type RecentEntry,
  recentDocuments,
  saveDocumentCopy,
  showInFolder,
  takePendingDocuments,
  toFailure,
} from "./ipc";
import { comboOf } from "./shortcuts";
import { cycleIndex, indexAfterClose, insertionIndex } from "./tab-order";
import { applyTheme, currentTheme } from "./theme";
import { Toolbar } from "./toolbar";
import { button, toast } from "./ui";

const APP_NAME = "Violeta PDF";

export class App {
  private readonly views: DocumentView[] = [];
  private active: DocumentView | null = null;
  private readonly tabs = el("div", {
    class: "tabs",
    attrs: { role: "tablist", "aria-label": "Documentos" },
  });
  private readonly stage = el("main", { class: "stage" });
  private readonly welcome = el("div", { class: "welcome" });
  private readonly dropOverlay = el(
    "div",
    { class: "drop-overlay", attrs: { "aria-hidden": "true" } },
    [
      el("div", { class: "drop-card" }, [
        icon("file-pdf", 40),
        el("strong", { text: "Solte para abrir" }),
      ]),
    ],
  );
  private readonly toolbar: Toolbar;
  private readonly themeButton: HTMLButtonElement;

  constructor(private readonly mount: HTMLElement) {
    this.toolbar = new Toolbar({
      open: () => void this.pick(),
      saveCopy: () => void this.saveCopy(),
      showInFolder: () => void this.showInFolder(),
      toggleFullscreen: () => void toggleFullscreen(),
    });
    this.themeButton = button({
      icon: "sun",
      title: "Alternar tema",
      onClick: () => this.toggleTheme(),
    });
    this.renderThemeButton();
  }

  async start(): Promise<void> {
    this.mount.append(
      el("div", { class: "shell" }, [
        el("header", { class: "topbar" }, [
          el("div", { class: "brand" }, [
            el("span", { class: "brand-mark" }, [icon("paperclip", 16)]),
            el("span", { class: "brand-name", text: "Violeta" }),
          ]),
          this.tabs,
          button({
            icon: "plus",
            title: "Abrir PDF (Ctrl+O)",
            class: "tab-new",
            onClick: () => void this.pick(),
          }),
          el("div", { class: "topbar-spacer" }),
          this.themeButton,
          button({
            icon: "maximize",
            title: "Tela cheia (F11)",
            onClick: () => void toggleFullscreen(),
          }),
        ]),
        this.toolbar.root,
        this.stage,
      ]),
      this.dropOverlay,
    );
    this.stage.append(this.welcome);
    window.addEventListener("keydown", (event) => this.onKey(event));
    await this.listenForDrops();
    await onDocumentsOpened((outcome) => this.receive(outcome));
    this.receive(await takePendingDocuments());
    await this.refresh();
  }

  // ── Documentos ────────────────────────────────────────────────────────────

  private receive(outcome: OpenOutcome): void {
    for (const failure of outcome.failed) {
      toast("error", `${failure.name}: ${toFailure(failure.error).message}`);
    }
    outcome.opened.forEach((info, position) => {
      this.add(info, position === outcome.opened.length - 1);
    });
    if (outcome.opened.length > 0) void this.refreshRecent();
  }

  private add(info: DocumentInfo, activate: boolean): void {
    const view = new DocumentView(info, {
      openDocument: (opened) => this.add(opened, true),
      changed: (changed) => this.changed(changed),
      close: (closing) => void this.close(closing),
    });
    const parent =
      info.parentId === null ? -1 : this.views.findIndex((v) => v.info.id === info.parentId);
    this.views.splice(insertionIndex(parent >= 0 ? parent : null, this.views.length), 0, view);
    this.stage.append(view.root);
    if (activate || !this.active) this.activate(view);
    else void this.refresh();
    void view.load();
  }

  private activate(view: DocumentView | null): void {
    this.active = view;
    for (const v of this.views) v.root.classList.toggle("active", v === view);
    this.toolbar.bind(view);
    view?.focus();
    void this.refresh();
  }

  private async close(view: DocumentView): Promise<void> {
    const index = this.views.indexOf(view);
    if (index < 0) return;
    this.views.splice(index, 1);
    if (this.active === view) {
      const next = indexAfterClose(index, this.views.length + 1);
      this.activate(next === null ? null : (this.views[next] ?? null));
    } else {
      void this.refresh();
    }
    try {
      await view.destroy();
    } finally {
      // O documento já sumiu da interface; a falha de liberar os bytes só pesa na memória.
      await closeDocument(view.info.id).catch(() => undefined);
    }
  }

  private changed(view: DocumentView): void {
    if (view === this.active) this.toolbar.render();
    this.renderTabs();
  }

  // ── Comandos ──────────────────────────────────────────────────────────────

  private async pick(): Promise<void> {
    try {
      this.receive(await pickDocuments());
    } catch (error) {
      toast("error", toFailure(error).message);
    }
  }

  private async openRecentAt(index: number): Promise<void> {
    try {
      this.add(await openRecent(index), true);
      await this.refreshRecent();
    } catch (error) {
      toast("error", toFailure(error).message);
      await this.refreshRecent();
    }
  }

  private async saveCopy(): Promise<void> {
    if (!this.active) return;
    try {
      const saved = await saveDocumentCopy(this.active.info.id);
      if (saved) toast("success", `Cópia salva em ${saved.path}`);
    } catch (error) {
      toast("error", toFailure(error).message);
    }
  }

  private async showInFolder(): Promise<void> {
    if (!this.active) return;
    try {
      await showInFolder(this.active.info.id);
    } catch (error) {
      toast("error", toFailure(error).message);
    }
  }

  private toggleTheme(): void {
    applyTheme(currentTheme() === "dark" ? "light" : "dark");
    this.renderThemeButton();
  }

  private renderThemeButton(): void {
    const dark = currentTheme() === "dark";
    this.themeButton.replaceChildren(icon(dark ? "sun" : "moon"));
    this.themeButton.title = dark ? "Tema claro" : "Tema escuro";
  }

  // ── Renderização ──────────────────────────────────────────────────────────

  private async refresh(): Promise<void> {
    const empty = this.views.length === 0;
    this.welcome.hidden = !empty;
    this.toolbar.root.hidden = empty;
    this.renderTabs();
    const title = this.active ? `${this.active.info.name} · ${APP_NAME}` : APP_NAME;
    document.title = title;
    await getCurrentWindow()
      .setTitle(title)
      .catch(() => undefined);
    if (empty) await this.refreshRecent();
  }

  private renderTabs(): void {
    clear(this.tabs);
    for (const view of this.views) {
      const active = view === this.active;
      const count = view.state.attachmentCount ?? 0;
      const tab = el(
        "div",
        {
          class: `tab${active ? " active" : ""}`,
          title: view.info.path ?? `${view.info.name} (anexo)`,
          attrs: { role: "tab", "aria-selected": String(active), tabindex: active ? "0" : "-1" },
          on: {
            click: () => this.activate(view),
            auxclick: (event) => {
              if (event.button === 1) void this.close(view);
            },
          },
        },
        [
          el("span", { class: "tab-icon" }, [
            icon(view.info.parentId === null ? "file-pdf" : "paperclip", 15),
          ]),
          el("span", { class: "tab-name", text: view.info.name }),
          count > 0 ? el("span", { class: "tab-dot", title: `${count} anexo(s)` }) : null,
          button({
            icon: "x",
            title: "Fechar (Ctrl+W)",
            size: "sm",
            class: "tab-close",
            onClick: (event) => {
              event.stopPropagation();
              void this.close(view);
            },
          }),
        ],
      );
      this.tabs.append(tab);
    }
    this.tabs.querySelector(".tab.active")?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }

  private async refreshRecent(): Promise<void> {
    if (this.views.length > 0) return;
    let recent: RecentEntry[] = [];
    try {
      recent = await recentDocuments();
    } catch (error) {
      toast("error", toFailure(error).message);
    }
    this.renderWelcome(recent);
  }

  private renderWelcome(recent: RecentEntry[]): void {
    clear(this.welcome);
    const features: Array<[Parameters<typeof icon>[0], string]> = [
      ["paperclip", "Anexos embutidos"],
      ["search", "Busca no texto"],
      ["layers", "Miniaturas e sumário"],
      ["lock", "PDFs protegidos"],
    ];
    this.welcome.append(
      el("div", { class: "welcome-hero" }, [
        el("div", { class: "hero-mark" }, [icon("file-pdf", 44)]),
        el("h1", { class: "hero-title" }, [el("span", { text: "Violeta" })]),
        el("p", { class: "hero-tagline", text: "Leia o PDF — e tudo o que vem dentro dele." }),
        el("div", { class: "hero-actions" }, [
          button({
            label: "Abrir PDF",
            icon: "folder-open",
            variant: "primary",
            onClick: () => void this.pick(),
          }),
          el("span", { class: "hero-hint" }, [
            "ou arraste arquivos para a janela · ",
            el("kbd", { text: "Ctrl" }),
            "+",
            el("kbd", { text: "O" }),
          ]),
        ]),
        el(
          "ul",
          { class: "feature-chips" },
          features.map(([name, label]) =>
            el("li", {}, [icon(name, 15), el("span", { text: label })]),
          ),
        ),
      ]),
    );
    if (recent.length === 0) return;
    this.welcome.append(
      el("section", { class: "recent" }, [
        el("header", { class: "recent-header" }, [
          el("h2", {}, [icon("clock", 16), el("span", { text: "Recentes" })]),
          button({
            label: "Limpar",
            size: "sm",
            onClick: () =>
              void clearRecent()
                .then(() => this.refreshRecent())
                .catch((error: unknown) => toast("error", toFailure(error).message)),
          }),
        ]),
        el(
          "ul",
          { class: "recent-list" },
          recent.map((entry, index) =>
            el("li", {}, [
              el(
                "button",
                {
                  class: "recent-item",
                  attrs: { type: "button" },
                  title: `${entry.folder}\\${entry.name}`,
                  on: { click: () => void this.openRecentAt(index) },
                },
                [
                  el("span", { class: "recent-icon" }, [icon("file-pdf", 20)]),
                  el("span", { class: "recent-text" }, [
                    el("span", { class: "recent-name", text: entry.name }),
                    el("span", { class: "recent-folder", text: entry.folder }),
                  ]),
                  icon("chevron-right", 16),
                ],
              ),
            ]),
          ),
        ),
      ]),
    );
  }

  // ── Entrada ───────────────────────────────────────────────────────────────

  /** O overlay de soltar; o arquivo em si é aberto pelo Rust, que recebe o evento. */
  private async listenForDrops(): Promise<void> {
    await getCurrentWebview().onDragDropEvent((event) => {
      const type = event.payload.type;
      this.dropOverlay.classList.toggle("visible", type === "enter" || type === "over");
    });
  }

  /** Os atalhos que valem com ou sem documento aberto. */
  private readonly globalKeys: Record<string, (event: KeyboardEvent) => void> = {
    "ctrl+o": () => void this.pick(),
    "ctrl+tab": () => this.cycle(1),
    "ctrl+shift+tab": () => this.cycle(-1),
    f11: () => void toggleFullscreen(),
    // Recarregar trocaria a sessão inteira por uma vazia, e imprimir imprimiria a
    // interface do app, não o documento.
    f5: () => undefined,
    "ctrl+p": () => undefined,
    "ctrl+shift+r": () => this.active?.rotate(-90),
    "ctrl+r": () => this.active?.rotate(90),
  };

  /** Os atalhos do documento ativo. */
  private readonly documentKeys: Record<
    string,
    (view: DocumentView, event: KeyboardEvent) => void
  > = {
    "ctrl+w": (view) => void this.close(view),
    "ctrl+f": () => this.toolbar.focusSearch(),
    f3: () => this.toolbar.findNext(false),
    "shift+f3": () => this.toolbar.findNext(true),
    "ctrl+=": (view) => view.zoomIn(),
    "ctrl++": (view) => view.zoomIn(),
    "ctrl+shift++": (view) => view.zoomIn(),
    "ctrl+-": (view) => view.zoomOut(),
    "ctrl+0": (view) => view.setScale("page-width"),
    "ctrl+s": () => void this.saveCopy(),
    "ctrl+d": (view) => void view.properties(),
    "ctrl+shift+a": (view) => view.setDrawer(!view.state.drawerOpen),
    f4: (view) => view.setSidebar(!view.state.sidebarOpen),
  };

  /** Os atalhos de navegação, que não valem enquanto se digita num campo. */
  private readonly navigationKeys: Record<string, (view: DocumentView) => void> = {
    home: (view) => view.goToPage(1),
    end: (view) => view.goToPage(view.state.pages),
    n: (view) => view.nextPage(),
    j: (view) => view.nextPage(),
    p: (view) => view.previousPage(),
    k: (view) => view.previousPage(),
  };

  private onKey(event: KeyboardEvent): void {
    const combo = comboOf(event);
    const view = this.active;
    const global = this.globalKeys[combo];
    const forDocument = view ? this.documentKeys[combo] : undefined;
    const forNavigation = view && !isTyping(event.target) ? this.navigationKeys[combo] : undefined;
    if (!global && !forDocument && !forNavigation) return;
    event.preventDefault();
    event.stopPropagation();
    if (global) global(event);
    else if (view && forDocument) forDocument(view, event);
    else if (view && forNavigation) forNavigation(view);
  }

  private cycle(delta: number): void {
    if (this.views.length === 0) return;
    const index = this.active ? this.views.indexOf(this.active) : 0;
    this.activate(this.views[cycleIndex(index, this.views.length, delta)] ?? null);
  }
}

function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}

async function toggleFullscreen(): Promise<void> {
  const window = getCurrentWindow();
  await window.setFullscreen(!(await window.isFullscreen()));
}
