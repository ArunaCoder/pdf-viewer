// O painel de anexos do documento e a pré-visualização de cada anexo.
//
// Tudo o que se decide sobre um anexo — tipo, se é executável, se confere com o que o PDF
// declara — vem pronto do Rust. Aqui só se escolhe como mostrar.

import { renderDocument } from "./document-preview";
import { append, clear, el } from "./dom";
import {
  attachmentMeta,
  countLabel,
  formatBytes,
  formatIsoDate,
  originLabel,
  typeLabel,
} from "./format";
import { type IconName, icon } from "./icons";
import {
  type Attachment,
  type AttachmentKind,
  type AttachmentListing,
  attachmentBytes,
  attachmentDocumentPreview,
  type DocumentInfo,
  type Integrity,
  type IpcFailure,
  openAttachmentAsDocument,
  openAttachmentExternally,
  saveAllAttachments,
  saveAttachment,
  toFailure,
  verifyAttachment,
} from "./ipc";
import { button, modal, toast } from "./ui";

/** O teto do texto exibido na prévia; o resto do arquivo fica para salvar. */
const MAX_PREVIEW_CHARS = 400_000;

const KIND_ICONS: Record<AttachmentKind, IconName> = {
  pdf: "file-pdf",
  image: "image",
  text: "file-text",
  document: "file-text",
  other: "archive",
};

export interface AttachmentHost {
  documentId: number;
  /** Abre o documento que saiu de um anexo numa aba nova. */
  openDocument(info: DocumentInfo): void;
}

/** O que o painel mostra: carregando, a lista, ou a falha da leitura. */
type PanelState =
  | { kind: "loading" }
  | { kind: "ready"; listing: AttachmentListing }
  | { kind: "failed"; failure: IpcFailure };

export class AttachmentsPanel {
  readonly root: HTMLElement;
  private readonly body = el("div", { class: "drawer-body" });
  private readonly count = el("span", { class: "drawer-count" });
  private readonly saveAll: HTMLButtonElement;
  private state: PanelState = { kind: "loading" };

  constructor(
    private readonly host: AttachmentHost,
    onClose: () => void,
  ) {
    this.saveAll = button({
      label: "Salvar todos",
      icon: "download",
      variant: "soft",
      size: "sm",
      onClick: () => void this.saveEverything(),
    });
    this.root = el("aside", { class: "drawer", attrs: { "aria-label": "Anexos" } }, [
      el("header", { class: "drawer-header" }, [
        el("div", { class: "drawer-title" }, [
          el("span", { class: "drawer-title-icon" }, [icon("paperclip", 17)]),
          el("h2", { text: "Anexos" }),
          this.count,
        ]),
        el("div", { class: "drawer-actions" }, [
          this.saveAll,
          button({ icon: "x", title: "Fechar painel", size: "sm", onClick: onClose }),
        ]),
      ]),
      this.body,
    ]);
    this.render();
  }

  /** Quantos anexos legíveis o documento tem; `null` enquanto não se sabe. */
  get attachmentCount(): number | null {
    return this.state.kind === "ready" ? this.state.listing.attachments.length : null;
  }

  get failed(): boolean {
    return this.state.kind === "failed";
  }

  setListing(listing: AttachmentListing): void {
    this.state = { kind: "ready", listing };
    this.render();
  }

  setFailure(failure: IpcFailure): void {
    this.state = { kind: "failed", failure };
    this.render();
  }

  private render(): void {
    clear(this.body);
    this.saveAll.hidden = !(
      this.state.kind === "ready" && this.state.listing.attachments.length > 1
    );
    switch (this.state.kind) {
      case "loading":
        this.count.textContent = "";
        this.body.append(
          el("div", { class: "drawer-loading" }, [
            el("span", { class: "spinner" }),
            "Lendo anexos…",
          ]),
        );
        return;
      case "failed":
        this.count.textContent = "";
        this.body.append(
          emptyState("alert", "Não foi possível ler os anexos", this.state.failure.message),
        );
        return;
      case "ready":
        this.renderListing(this.state.listing);
        return;
    }
  }

  private renderListing(listing: AttachmentListing): void {
    const { attachments, unreadable } = listing;
    this.count.textContent = attachments.length > 0 ? String(attachments.length) : "";
    if (attachments.length === 0 && unreadable.length === 0) {
      this.body.append(
        emptyState("paperclip", "Nenhum anexo", "Este PDF não carrega arquivos anexados."),
      );
      return;
    }
    const list = el("ul", { class: "attachment-list" });
    for (const attachment of attachments) list.append(this.item(attachment));
    this.body.append(list);
    if (unreadable.length > 0) {
      this.body.append(
        el("section", { class: "unreadable" }, [
          el("h3", {
            text: countLabel(
              unreadable.length,
              "arquivo citado fora do PDF",
              "arquivos citados fora do PDF",
            ),
          }),
          el("p", {
            class: "muted",
            text: "O documento referencia estes arquivos, mas eles não estão embutidos nele.",
          }),
          el(
            "ul",
            {},
            unreadable.map((item) =>
              el("li", {}, [
                icon(item.reason === "not_embedded" ? "external" : "alert", 14),
                el("span", { text: item.name ?? "Arquivo sem nome" }),
                el("span", { class: "muted", text: ` · ${originLabel(item.origin)}` }),
              ]),
            ),
          ),
        ]),
      );
    }
  }

  private item(attachment: Attachment): HTMLElement {
    const primary =
      attachment.kind === "pdf"
        ? button({
            icon: "arrow-up-right",
            title: "Abrir em nova aba",
            size: "sm",
            onClick: () => void this.openAsTab(attachment),
          })
        : button({
            icon: "eye",
            title: "Visualizar",
            size: "sm",
            onClick: () => this.preview(attachment),
          });
    return el(
      "li",
      {
        class: `attachment kind-${attachment.kind}`,
        attrs: { tabindex: "0" },
        title: attachment.name,
        on: {
          dblclick: () => void this.activate(attachment),
          keydown: (event) => {
            if (event.key === "Enter") void this.activate(attachment);
          },
        },
      },
      [
        el("span", { class: "attachment-icon" }, [
          icon(KIND_ICONS[attachment.kind], 20),
          el("span", { class: "attachment-ext", text: typeLabel(attachment) }),
        ]),
        el("div", { class: "attachment-text" }, [
          el("span", { class: "attachment-name", text: attachment.name }),
          el("span", { class: "attachment-meta", text: attachmentMeta(attachment) }),
          attachment.description
            ? el("span", { class: "attachment-desc", text: attachment.description })
            : null,
        ]),
        el("div", { class: "attachment-actions" }, [
          primary,
          button({
            icon: "download",
            title: "Salvar como…",
            size: "sm",
            onClick: () => void this.save(attachment),
          }),
          button({
            icon: "info",
            title: "Detalhes",
            size: "sm",
            onClick: () => this.preview(attachment),
          }),
        ]),
      ],
    );
  }

  /** O gesto principal (duplo clique, Enter): PDF abre em aba, o resto abre a prévia. */
  private async activate(attachment: Attachment): Promise<void> {
    if (attachment.kind === "pdf") await this.openAsTab(attachment);
    else this.preview(attachment);
  }

  async openAsTab(attachment: Attachment): Promise<void> {
    try {
      const info = await openAttachmentAsDocument(this.host.documentId, attachment.id);
      this.host.openDocument(info);
    } catch (error) {
      toast("error", toFailure(error).message);
    }
  }

  async save(attachment: Attachment): Promise<void> {
    try {
      const saved = await saveAttachment(this.host.documentId, attachment.id);
      if (saved) toast("success", `Anexo salvo em ${saved.path}`);
    } catch (error) {
      toast("error", toFailure(error).message);
    }
  }

  async openExternally(attachment: Attachment): Promise<void> {
    try {
      await openAttachmentExternally(this.host.documentId, attachment.id);
    } catch (error) {
      toast("error", toFailure(error).message);
    }
  }

  private async saveEverything(): Promise<void> {
    try {
      const batch = await saveAllAttachments(this.host.documentId);
      if (!batch) return;
      if (batch.failed.length === 0) {
        toast(
          "success",
          `${countLabel(batch.saved, "anexo salvo", "anexos salvos")} em ${batch.folder}`,
        );
      } else {
        const names = batch.failed.map((failure) => failure.name).join(", ");
        toast(
          "warning",
          `${countLabel(batch.saved, "anexo salvo", "anexos salvos")}; não foi possível salvar: ${names}`,
        );
      }
    } catch (error) {
      toast("error", toFailure(error).message);
    }
  }

  preview(attachment: Attachment): void {
    new AttachmentPreview(this, this.host.documentId, attachment).open();
  }
}

function emptyState(iconName: IconName, title: string, text: string): HTMLElement {
  return el("div", { class: "empty-note" }, [
    el("span", { class: "empty-icon" }, [icon(iconName, 26)]),
    el("strong", { text: title }),
    el("p", { text }),
  ]);
}

// ── Pré-visualização ───────────────────────────────────────────────────────

class AttachmentPreview {
  private objectUrl: string | null = null;

  constructor(
    private readonly panel: AttachmentsPanel,
    private readonly documentId: number,
    private readonly attachment: Attachment,
  ) {}

  open(): void {
    const attachment = this.attachment;
    const { body, footer, close, closed } = modal({
      title: attachment.name,
      subtitle: attachmentMeta(attachment),
      icon: KIND_ICONS[attachment.kind],
      size: "lg",
    });
    const stage = el("div", { class: "preview-stage" });
    const integrity = el("div", { class: "integrity pending" }, [
      el("span", { class: "spinner" }),
      el("span", { text: "Conferindo integridade…" }),
    ]);
    body.classList.add("preview");
    body.append(stage, el("aside", { class: "preview-details" }, [this.details(), integrity]));

    const external = button({
      label: "Abrir no app padrão",
      icon: "external",
      onClick: () => void this.panel.openExternally(attachment),
    });
    if (attachment.executable) {
      external.disabled = true;
      external.title =
        "Executáveis não são abertos pelo visualizador. Salve e verifique o arquivo antes de usá-lo.";
    }
    footer.append(
      external,
      button({
        label: "Salvar como…",
        icon: "download",
        onClick: () => void this.panel.save(attachment),
      }),
      attachment.kind === "pdf"
        ? button({
            label: "Abrir em nova aba",
            icon: "arrow-up-right",
            variant: "primary",
            onClick: () => {
              close();
              void this.panel.openAsTab(attachment);
            },
          })
        : button({ label: "Fechar", variant: "primary", onClick: close }),
    );
    void closed.then(() => this.release());
    void this.fillStage(stage);
    void this.checkIntegrity(integrity);
  }

  private details(): HTMLElement {
    const a = this.attachment;
    const rows: Array<[string, string | null]> = [
      ["Tipo", a.mimeType ?? typeLabel(a)],
      ["Tamanho declarado", a.declaredSize === null ? null : formatBytes(a.declaredSize)],
      ["Anexado a", originLabel(a.origin)],
      ["Criado em", formatIsoDate(a.created)],
      ["Modificado em", formatIsoDate(a.modified)],
      ["Relação com o PDF", a.relationship],
      ["Descrição", a.description],
    ];
    return el(
      "dl",
      { class: "properties compact" },
      rows
        .filter((row): row is [string, string] => row[1] !== null)
        .flatMap(([label, value]) => [el("dt", { text: label }), el("dd", { text: value })]),
    );
  }

  private async fillStage(stage: HTMLElement): Promise<void> {
    const a = this.attachment;
    if (a.kind === "pdf") {
      stage.append(
        placeholder(
          "file-pdf",
          "Documento PDF",
          "Abra em uma nova aba para ler com todos os recursos do visualizador.",
        ),
      );
      return;
    }
    if (a.kind === "other") {
      stage.append(
        placeholder(
          a.executable ? "shield-alert" : "archive",
          a.executable ? "Arquivo executável" : "Sem pré-visualização",
          a.executable
            ? "Programas e scripts anexados a um PDF são um vetor comum de ataque. O visualizador não os abre; salve e verifique com o antivírus antes de usar."
            : "Este tipo de arquivo não tem pré-visualização. Salve-o ou abra no aplicativo padrão.",
        ),
      );
      return;
    }
    stage.append(
      el("div", { class: "drawer-loading" }, [el("span", { class: "spinner" }), "Carregando…"]),
    );
    try {
      if (a.kind === "document") {
        const preview = await attachmentDocumentPreview(this.documentId, a.id);
        clear(stage);
        stage.classList.add("document");
        stage.append(renderDocument(preview));
        return;
      }
      const bytes = await attachmentBytes(this.documentId, a.id);
      clear(stage);
      if (a.kind === "image") this.showImage(stage, bytes);
      else showText(stage, bytes);
    } catch (error) {
      clear(stage);
      stage.append(placeholder("alert", "Não foi possível carregar", toFailure(error).message));
    }
  }

  private showImage(stage: HTMLElement, bytes: ArrayBuffer): void {
    const type = this.attachment.mimeType?.startsWith("image/") ? this.attachment.mimeType : "";
    this.objectUrl = URL.createObjectURL(new Blob([bytes], { type }));
    const image = el("img", {
      class: "preview-image",
      attrs: { alt: this.attachment.name, src: this.objectUrl },
    });
    image.addEventListener("error", () => {
      image.replaceWith(
        placeholder(
          "image",
          "Imagem ilegível",
          "O conteúdo não é uma imagem que o visualizador consiga desenhar.",
        ),
      );
    });
    stage.classList.add("checkerboard");
    stage.append(image);
  }

  private async checkIntegrity(target: HTMLElement): Promise<void> {
    try {
      const verdict = await verifyAttachment(this.documentId, this.attachment.id);
      renderIntegrity(target, verdict);
    } catch (error) {
      target.className = "integrity bad";
      clear(target);
      target.append(icon("alert", 16), el("span", { text: toFailure(error).message }));
    }
  }

  private release(): void {
    if (this.objectUrl) URL.revokeObjectURL(this.objectUrl);
    this.objectUrl = null;
  }
}

function placeholder(iconName: IconName, title: string, text: string): HTMLElement {
  return el("div", { class: "preview-placeholder" }, [
    el("span", { class: "preview-placeholder-icon" }, [icon(iconName, 34)]),
    el("strong", { text: title }),
    el("p", { text }),
  ]);
}

/** O texto do anexo: UTF-8 quando é, Windows-1252 (o legado dos sistemas brasileiros) quando não. */
export function decodeText(bytes: ArrayBuffer): string {
  const view = new Uint8Array(bytes);
  if (view[0] === 0xff && view[1] === 0xfe) return new TextDecoder("utf-16le").decode(view);
  if (view[0] === 0xfe && view[1] === 0xff) return new TextDecoder("utf-16be").decode(view);
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(view);
  } catch {
    return new TextDecoder("windows-1252").decode(view);
  }
}

function showText(stage: HTMLElement, bytes: ArrayBuffer): void {
  const text = decodeText(bytes);
  const truncated = text.length > MAX_PREVIEW_CHARS;
  append(stage, [
    el("pre", { class: "preview-text", text: truncated ? text.slice(0, MAX_PREVIEW_CHARS) : text }),
    truncated
      ? el("p", {
          class: "preview-note",
          text: "Pré-visualização truncada. Salve o arquivo para ver o conteúdo completo.",
        })
      : null,
  ]);
}

function renderIntegrity(target: HTMLElement, verdict: Integrity): void {
  clear(target);
  switch (verdict.status) {
    case "verified":
      target.className = "integrity good";
      target.append(
        icon("shield-check", 16),
        el("span", { text: "Íntegro — o MD5 declarado no PDF confere." }),
      );
      return;
    case "not_declared":
      target.className = "integrity neutral";
      target.append(
        icon("shield", 16),
        el("span", { text: "O PDF não declara checksum; a integridade não pode ser conferida." }),
      );
      return;
    case "size_mismatch":
      target.className = "integrity bad";
      target.append(
        icon("shield-alert", 16),
        el("span", {
          text: `Tamanho divergente: o PDF declara ${formatBytes(verdict.declared)}, o arquivo tem ${formatBytes(verdict.actual)}.`,
        }),
      );
      return;
    case "checksum_mismatch":
      target.className = "integrity bad";
      target.append(
        icon("shield-alert", 16),
        el("span", {
          text: "O MD5 não confere com o declarado: o anexo pode ter sido corrompido ou alterado.",
        }),
      );
      return;
  }
}
