// A ordem das abas, em funções puras: qual aba fica ativa depois de fechar a ativa, e para
// onde vai o Ctrl+Tab.

/**
 * A posição da aba que fica ativa depois de fechar a da posição `closed`, numa lista que
 * tinha `length` abas: a que estava à direita, ou a da esquerda quando a fechada era a
 * última. `null` quando não sobra aba.
 */
export function indexAfterClose(closed: number, length: number): number | null {
  const remaining = length - 1;
  if (remaining <= 0) return null;
  return Math.min(closed, remaining - 1);
}

/** A posição `delta` abas adiante de `current`, dando a volta nas pontas. */
export function cycleIndex(current: number, length: number, delta: number): number {
  if (length <= 0) return 0;
  return (((current + delta) % length) + length) % length;
}

/**
 * Onde entra a aba nova: logo depois da aba de onde ela saiu (o anexo aberto a partir de um
 * documento fica ao lado dele), ou no fim.
 */
export function insertionIndex(parentIndex: number | null, length: number): number {
  return parentIndex === null ? length : parentIndex + 1;
}
