// O tema claro/escuro. A escolha é conveniência de quem usa, guardada no navegador do app;
// sem armazenamento disponível, o app segue no escuro, que é o tema de origem.

export type Theme = "dark" | "light";

const KEY = "violeta.theme";

export function storedTheme(): Theme {
  try {
    return localStorage.getItem(KEY) === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // Sem armazenamento, o tema vale só para esta sessão.
  }
}

export function currentTheme(): Theme {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}
