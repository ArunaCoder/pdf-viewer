// A barra de ferramentas: uma só, ligada à aba ativa. Ela não guarda estado do documento —
// lê o `ViewState` da aba a cada mudança e devolve os gestos à aba.

import type { DocumentView } from "./document-view";
import { el } from "./dom";
import { formatZoom } from "./format";
import { icon } from "./icons";
import { viewerLib } from "./pdfjs";
import { button, type MenuEntry, showMenu } from "./ui";

export interface ToolbarActions {
  open(): void;
  saveCopy(): void;
  showInFolder(): void;
  toggleFullscreen(): void;
}

const ZOOM_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

export class Toolbar {
  readonly root: HTMLElement;
  private view: DocumentView | null = null;
  private readonly pageInput: HTMLInputElement;
  private readonly pageTotal = el("span", { class: "page-total" });
  private readonly zoomButton: HTMLButtonElement;
  private readonly searchInput: HTMLInputElement;
  private readonly searchCount = el("span", { class: "search-count" });
  private readonly searchBox: HTMLElement;
  private readonly sidebarButton: HTMLButtonElement;
  private readonly attachmentsButton: HTMLButtonElement;
  private readonly attachmentsBadge = el("span", { class: "badge" });
  private readonly prevButton: HTMLButtonElement;
  private readonly nextButton: HTMLButtonElement;

  constructor(private readonly actions: ToolbarActions) {
    this.sidebarButton = button({
      icon: "sidebar",
      title: "Painel lateral (F4)",
      onClick: () => this.view?.setSidebar(!this.view.state.sidebarOpen),
    });
    this.prevButton = button({
      icon: "chevron-left",
      title: "Página anterior",
      onClick: () => this.view?.previousPage(),
    });
    this.nextButton = button({
      icon: "chevron-right",
      title: "Próxima página",
      onClick: () => this.view?.nextPage(),
    });
    this.pageInput = el("input", {
      class: "page-input",
      attrs: {
        type: "text",
        inputmode: "numeric",
        "aria-label": "Página atual",
        autocomplete: "off",
      },
      on: {
        keydown: (event) => {
          if (event.key === "Enter") {
            this.view?.goToPage(Number.parseInt(this.pageInput.value, 10));
            this.view?.focus();
          }
        },
        blur: () => this.render(),
        focus: () => this.pageInput.select(),
      },
    });
    this.zoomButton = el(
      "button",
      {
        class: "btn btn-ghost btn-md zoom-value",
        attrs: { type: "button", "aria-haspopup": "menu", "aria-label": "Zoom" },
        on: { click: () => this.zoomMenu() },
      },
      [el("span", { class: "zoom-text" }), icon("chevron-down", 14)],
    );
    this.searchInput = el("input", {
      class: "search-input",
      attrs: {
        type: "search",
        placeholder: "Buscar no documento",
        "aria-label": "Buscar no documento",
        spellcheck: "false",
      },
      on: {
        input: () => this.view?.find(this.searchInput.value, { again: false, previous: false }),
        keydown: (event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            this.view?.find(this.searchInput.value, { again: true, previous: event.shiftKey });
          } else if (event.key === "Escape") {
            this.searchInput.value = "";
            this.view?.clearFind();
            this.view?.focus();
          }
        },
      },
    });
    this.searchBox = el("div", { class: "search-box" }, [
      icon("search", 16),
      this.searchInput,
      this.searchCount,
      button({
        icon: "chevron-up",
        title: "Anterior (Shift+Enter)",
        size: "sm",
        onClick: () => this.findNext(true),
      }),
      button({
        icon: "chevron-down",
        title: "Próximo (Enter)",
        size: "sm",
        onClick: () => this.findNext(false),
      }),
      button({
        icon: "more",
        title: "Opções de busca",
        size: "sm",
        onClick: (event) => this.searchMenu(event.currentTarget as HTMLElement),
      }),
    ]);
    this.attachmentsButton = button({
      icon: "paperclip",
      label: "Anexos",
      title: "Anexos do documento (Ctrl+Shift+A)",
      class: "attachments-button",
      onClick: () => this.view?.setDrawer(!this.view.state.drawerOpen),
    });
    this.attachmentsButton.append(this.attachmentsBadge);
    const more = button({ icon: "more", title: "Mais opções", onClick: () => this.moreMenu(more) });
    more.setAttribute("aria-haspopup", "menu");

    this.root = el(
      "div",
      { class: "toolbar", attrs: { role: "toolbar", "aria-label": "Documento" } },
      [
        el("div", { class: "toolbar-group" }, [
          this.sidebarButton,
          el("span", { class: "divider" }),
          this.prevButton,
          el("div", { class: "page-field" }, [this.pageInput, this.pageTotal]),
          this.nextButton,
        ]),
        el("div", { class: "toolbar-group" }, [
          button({
            icon: "minus",
            title: "Diminuir zoom (Ctrl −)",
            onClick: () => this.view?.zoomOut(),
          }),
          this.zoomButton,
          button({
            icon: "plus",
            title: "Aumentar zoom (Ctrl +)",
            onClick: () => this.view?.zoomIn(),
          }),
          el("span", { class: "divider" }),
          button({
            icon: "rotate-ccw",
            title: "Girar à esquerda (Ctrl+Shift+R)",
            onClick: () => this.view?.rotate(-90),
          }),
          button({
            icon: "rotate-cw",
            title: "Girar à direita (Ctrl+R)",
            onClick: () => this.view?.rotate(90),
          }),
        ]),
        el("div", { class: "toolbar-spacer" }),
        el("div", { class: "toolbar-group" }, [this.searchBox, this.attachmentsButton, more]),
      ],
    );
    this.render();
  }

  bind(view: DocumentView | null): void {
    this.view = view;
    this.searchInput.value = "";
    this.render();
  }

  focusSearch(): void {
    this.searchInput.focus();
    this.searchInput.select();
  }

  findNext(previous: boolean): void {
    if (this.searchInput.value.trim().length === 0) {
      this.focusSearch();
      return;
    }
    this.view?.find(this.searchInput.value, { again: true, previous });
  }

  /** Relê o estado da aba ativa. */
  render(): void {
    const view = this.view;
    this.root.classList.toggle("disabled", !view?.state.loaded);
    if (!view) return;
    const { state } = view;
    if (document.activeElement !== this.pageInput)
      this.pageInput.value = state.loaded ? String(state.page) : "";
    this.pageTotal.textContent = state.loaded ? `de ${state.pages}` : "";
    this.prevButton.disabled = !state.loaded || state.page <= 1;
    this.nextButton.disabled = !state.loaded || state.page >= state.pages;
    const zoomText = this.zoomButton.querySelector(".zoom-text");
    if (zoomText) zoomText.textContent = formatZoom(state.scale);
    this.sidebarButton.classList.toggle("active", state.sidebarOpen);
    this.renderSearch(view);
    this.renderAttachments(view);
  }

  private renderSearch(view: DocumentView): void {
    const { status, current, total } = view.state.find;
    this.searchBox.classList.toggle("not-found", status === "not_found");
    this.searchCount.textContent =
      status === "idle"
        ? ""
        : status === "not_found"
          ? "Nada"
          : total > 0
            ? `${current} de ${total}`
            : "…";
  }

  private renderAttachments(view: DocumentView): void {
    const count = view.state.attachmentCount;
    this.attachmentsButton.classList.toggle("active", view.state.drawerOpen);
    this.attachmentsButton.classList.toggle("has-attachments", (count ?? 0) > 0);
    this.attachmentsButton.classList.toggle("failed", view.state.attachmentsFailed);
    this.attachmentsBadge.textContent = view.state.attachmentsFailed
      ? "!"
      : count && count > 0
        ? String(count)
        : "";
    this.attachmentsBadge.hidden = this.attachmentsBadge.textContent === "";
  }

  private zoomMenu(): void {
    const view = this.view;
    if (!view) return;
    const current = view.state.scaleValue;
    const preset = (
      label: string,
      value: string,
      iconName?: "fit-width" | "fit-page",
    ): MenuEntry => ({
      label,
      ...(iconName ? { icon: iconName } : {}),
      checked: current === value,
      run: () => view.setScale(value),
    });
    showMenu(this.zoomButton, [
      preset("Ajustar à largura", "page-width", "fit-width"),
      preset("Página inteira", "page-fit", "fit-page"),
      preset("Automático", "auto"),
      preset("Tamanho real", "page-actual"),
      { separator: true },
      ...ZOOM_PRESETS.map((scale) => preset(formatZoom(scale), String(scale))),
    ]);
  }

  private searchMenu(anchor: HTMLElement): void {
    const view = this.view;
    if (!view) return;
    const rerun = (): void => view.find(this.searchInput.value, { again: false, previous: false });
    showMenu(anchor, [
      {
        label: "Diferenciar maiúsculas",
        checked: view.findOptions.caseSensitive,
        run: () => {
          view.findOptions.caseSensitive = !view.findOptions.caseSensitive;
          rerun();
        },
      },
      {
        label: "Palavras inteiras",
        checked: view.findOptions.entireWord,
        run: () => {
          view.findOptions.entireWord = !view.findOptions.entireWord;
          rerun();
        },
      },
    ]);
  }

  private moreMenu(anchor: HTMLElement): void {
    const view = this.view;
    const { ScrollMode, SpreadMode } = viewerLib;
    const scroll = (label: string, mode: number): MenuEntry => ({
      label,
      checked: view?.state.scrollMode === mode,
      run: () => view?.setScrollMode(mode),
    });
    const spread = (label: string, mode: number): MenuEntry => ({
      label,
      checked: view?.state.spreadMode === mode,
      run: () => view?.setSpreadMode(mode),
    });
    showMenu(anchor, [
      {
        label: "Abrir PDF…",
        icon: "folder-open",
        shortcut: "Ctrl+O",
        run: () => this.actions.open(),
      },
      {
        label: "Salvar cópia…",
        icon: "download",
        shortcut: "Ctrl+S",
        run: () => this.actions.saveCopy(),
      },
      {
        label: "Mostrar na pasta",
        icon: "folder-open",
        disabled: view?.info.path === null,
        run: () => this.actions.showInFolder(),
      },
      {
        label: "Propriedades",
        icon: "info",
        shortcut: "Ctrl+D",
        run: () => void view?.properties(),
      },
      { separator: true },
      { heading: "Rolagem" },
      scroll("Vertical", ScrollMode.VERTICAL),
      scroll("Horizontal", ScrollMode.HORIZONTAL),
      scroll("Em grade", ScrollMode.WRAPPED),
      scroll("Uma página por vez", ScrollMode.PAGE),
      { separator: true },
      { heading: "Páginas lado a lado" },
      spread("Nenhuma", SpreadMode.NONE),
      spread("Ímpares à esquerda", SpreadMode.ODD),
      spread("Pares à esquerda", SpreadMode.EVEN),
      { separator: true },
      {
        label: "Leitura noturna",
        icon: "moon",
        checked: view?.state.nightMode ?? false,
        run: () => view?.setNightMode(!view.state.nightMode),
      },
      {
        label: "Tela cheia",
        icon: "maximize",
        shortcut: "F11",
        run: () => this.actions.toggleFullscreen(),
      },
    ]);
  }
}
