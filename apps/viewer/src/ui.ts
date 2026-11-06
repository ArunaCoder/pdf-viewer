// Os componentes de interface compartilhados: botão, toast, menu e diálogo modal.

import { el, type Props } from "./dom";
import { type IconName, icon } from "./icons";

// ── Botões ──────────────────────────────────────────────────────────────────

export interface ButtonOptions {
  label?: string;
  icon?: IconName;
  title?: string;
  variant?: "ghost" | "primary" | "soft" | "danger";
  size?: "sm" | "md";
  onClick?: (event: MouseEvent) => void;
  attrs?: Record<string, string>;
  class?: string;
}

/** Um botão do design system; só ícone quando não há rótulo (e então `title` é o nome). */
export function button(options: ButtonOptions): HTMLButtonElement {
  const variant = options.variant ?? "ghost";
  const classes = ["btn", `btn-${variant}`, `btn-${options.size ?? "md"}`];
  if (options.label === undefined) classes.push("btn-icon");
  if (options.class) classes.push(options.class);
  const props: Props = {
    class: classes.join(" "),
    attrs: { type: "button", ...(options.attrs ?? {}) },
  };
  if (options.title !== undefined) {
    props.title = options.title;
    props.attrs = { ...props.attrs, "aria-label": options.title };
  }
  if (options.onClick) props.on = { click: options.onClick };
  return el("button", props, [
    options.icon ? icon(options.icon) : null,
    options.label !== undefined ? el("span", { text: options.label }) : null,
  ]);
}

// ── Toasts ──────────────────────────────────────────────────────────────────

export type ToastKind = "info" | "success" | "error" | "warning";

export interface ToastAction {
  label: string;
  run: () => void;
}

const TOAST_ICONS: Record<ToastKind, IconName> = {
  info: "info",
  success: "check",
  error: "alert",
  warning: "shield-alert",
};

let toastStack: HTMLElement | null = null;

function stack(): HTMLElement {
  if (!toastStack) {
    toastStack = el("div", {
      class: "toast-stack",
      attrs: { role: "status", "aria-live": "polite" },
    });
    document.body.appendChild(toastStack);
  }
  return toastStack;
}

/** Uma notificação passageira no canto da tela. */
export function toast(kind: ToastKind, message: string, action?: ToastAction): void {
  const item = el("div", { class: `toast toast-${kind}` });
  const dismiss = (): void => {
    item.classList.add("leaving");
    window.setTimeout(() => item.remove(), 220);
  };
  item.append(
    el("span", { class: "toast-icon" }, [icon(TOAST_ICONS[kind], 16)]),
    el("span", { class: "toast-text", text: message }),
  );
  if (action) {
    item.append(
      button({
        label: action.label,
        variant: "soft",
        size: "sm",
        onClick: () => {
          action.run();
          dismiss();
        },
      }),
    );
  }
  item.append(button({ icon: "x", title: "Fechar", size: "sm", onClick: dismiss }));
  stack().append(item);
  window.setTimeout(dismiss, kind === "error" ? 9000 : 5500);
}

// ── Menu suspenso ───────────────────────────────────────────────────────────

export type MenuEntry =
  | {
      label: string;
      icon?: IconName;
      shortcut?: string;
      checked?: boolean;
      disabled?: boolean;
      run: () => void;
    }
  | { separator: true }
  | { heading: string };

type MenuAction = Extract<MenuEntry, { run: () => void }>;

let openMenu: { close: () => void } | null = null;

/** Abre um menu ancorado em `anchor`; fecha no clique fora, no Esc ou na escolha. */
export function showMenu(anchor: HTMLElement, entries: MenuEntry[]): void {
  openMenu?.close();
  const menu = el("div", { class: "menu", attrs: { role: "menu" } });
  const close = (): void => {
    menu.remove();
    anchor.setAttribute("aria-expanded", "false");
    document.removeEventListener("pointerdown", onPointer, true);
    document.removeEventListener("keydown", onKey, true);
    openMenu = null;
  };
  const onPointer = (event: PointerEvent): void => {
    if (!menu.contains(event.target as Node) && event.target !== anchor) close();
  };
  const onKey = (event: KeyboardEvent): void => {
    if (event.key !== "Escape") return;
    event.stopPropagation();
    close();
    anchor.focus();
  };
  menu.append(...entries.map((entry) => menuEntry(entry, close)));
  document.body.append(menu);
  placeBelow(menu, anchor);
  anchor.setAttribute("aria-expanded", "true");
  document.addEventListener("pointerdown", onPointer, true);
  document.addEventListener("keydown", onKey, true);
  openMenu = { close };
  menu.querySelector<HTMLButtonElement>(".menu-item:not([disabled])")?.focus();
}

function menuEntry(entry: MenuEntry, close: () => void): HTMLElement {
  if ("separator" in entry) return el("div", { class: "menu-separator" });
  if ("heading" in entry) return el("div", { class: "menu-heading", text: entry.heading });
  return menuItem(entry, close);
}

function menuItem(entry: MenuAction, close: () => void): HTMLButtonElement {
  const checkable = entry.checked !== undefined;
  const attrs: Record<string, string> = {
    type: "button",
    role: checkable ? "menuitemradio" : "menuitem",
  };
  if (checkable) attrs["aria-checked"] = String(entry.checked);
  if (entry.disabled) attrs.disabled = "";
  return el(
    "button",
    {
      class: entry.checked ? "menu-item checked" : "menu-item",
      attrs,
      on: {
        click: () => {
          close();
          entry.run();
        },
      },
    },
    [
      el("span", { class: "menu-check" }, [entry.checked ? icon("check", 15) : null]),
      entry.icon ? icon(entry.icon, 16) : el("span", { class: "menu-gap" }),
      el("span", { class: "menu-label", text: entry.label }),
      entry.shortcut ? el("kbd", { class: "menu-shortcut", text: entry.shortcut }) : null,
    ],
  );
}

/** Posiciona `menu` abaixo de `anchor`, alinhado à direita dele e dentro da janela. */
function placeBelow(menu: HTMLElement, anchor: HTMLElement): void {
  const rect = anchor.getBoundingClientRect();
  const width = menu.offsetWidth;
  const left = Math.min(Math.max(8, rect.right - width), window.innerWidth - width - 8);
  menu.style.left = `${left}px`;
  menu.style.top = `${rect.bottom + 6}px`;
}

// ── Diálogo modal ───────────────────────────────────────────────────────────

export interface Modal {
  dialog: HTMLDialogElement;
  body: HTMLElement;
  footer: HTMLElement;
  close: () => void;
  closed: Promise<void>;
}

/**
 * Um `<dialog>` modal nativo — o navegador cuida do foco preso e do Esc. O diálogo some do
 * DOM ao fechar, e `closed` resolve nesse momento.
 */
export function modal(options: {
  title: string;
  icon?: IconName;
  subtitle?: string;
  size?: "sm" | "md" | "lg";
}): Modal {
  const body = el("div", { class: "modal-body" });
  const footer = el("div", { class: "modal-footer" });
  const dialog = el("dialog", {
    class: `modal modal-${options.size ?? "md"}`,
  });
  const close = (): void => dialog.close();
  dialog.append(
    el("header", { class: "modal-header" }, [
      options.icon ? el("span", { class: "modal-icon" }, [icon(options.icon, 20)]) : null,
      el("div", { class: "modal-titles" }, [
        el("h2", { class: "modal-title", text: options.title }),
        options.subtitle !== undefined
          ? el("p", { class: "modal-subtitle", text: options.subtitle })
          : null,
      ]),
      button({ icon: "x", title: "Fechar", onClick: close }),
    ]),
    body,
    footer,
  );
  dialog.addEventListener("click", (event) => {
    // Clique no fundo (o próprio <dialog>, fora da caixa) fecha.
    if (event.target === dialog) close();
  });
  const closed = new Promise<void>((resolve) => {
    dialog.addEventListener("close", () => {
      dialog.remove();
      resolve();
    });
  });
  document.body.append(dialog);
  dialog.showModal();
  return { dialog, body, footer, close, closed };
}
