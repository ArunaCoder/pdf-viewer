// Construção de DOM sem `innerHTML`: todo texto entra como nó de texto, então nome de
// anexo, título de sumário ou metadado do PDF — tudo texto do autor do documento — nunca
// é interpretado como HTML.

type Child = Node | string | null | undefined | false;

type Listeners = {
  [K in keyof HTMLElementEventMap]?: (event: HTMLElementEventMap[K]) => void;
};

export interface Props {
  class?: string;
  text?: string;
  title?: string;
  attrs?: Record<string, string>;
  dataset?: Record<string, string>;
  on?: Listeners;
}

/** Um elemento `tag` com as propriedades e os filhos dados. */
export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Props = {},
  children: readonly Child[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (props.class !== undefined) node.className = props.class;
  if (props.text !== undefined) node.textContent = props.text;
  if (props.title !== undefined) node.title = props.title;
  for (const [name, value] of Object.entries(props.attrs ?? {})) {
    node.setAttribute(name, value);
  }
  for (const [name, value] of Object.entries(props.dataset ?? {})) {
    node.dataset[name] = value;
  }
  for (const [type, listener] of Object.entries(props.on ?? {})) {
    node.addEventListener(type, listener as EventListener);
  }
  append(node, children);
  return node;
}

/** Acrescenta os filhos que existem — `null`, `undefined` e `false` são pulados. */
export function append(parent: Node, children: readonly Child[]): void {
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    parent.appendChild(typeof child === "string" ? document.createTextNode(child) : child);
  }
}

/** Esvazia o nó. */
export function clear(node: Node): void {
  while (node.firstChild) node.removeChild(node.firstChild);
}

/** Troca os filhos do nó pelos dados. */
export function replaceChildren(node: Node, children: readonly Child[]): void {
  clear(node);
  append(node, children);
}

/** O elemento `#id`, ou erro se a página não o tiver — falha de montagem, não de uso. */
export function byId<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing #${id}`);
  return node as T;
}
