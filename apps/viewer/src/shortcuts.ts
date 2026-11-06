// A normalização de uma tecla em combinação (`ctrl+shift+a`), para os atalhos serem uma
// tabela e não uma cadeia de `if`.

export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/** A combinação da tecla: modificadores em ordem fixa, tecla em minúsculas. */
export function comboOf(event: KeyLike): string {
  const parts: string[] = [];
  if (event.ctrlKey || event.metaKey) parts.push("ctrl");
  if (event.altKey) parts.push("alt");
  if (event.shiftKey) parts.push("shift");
  parts.push(event.key === " " ? "space" : event.key.toLowerCase());
  return parts.join("+");
}
