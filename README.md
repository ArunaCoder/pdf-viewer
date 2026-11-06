# Violeta PDF

Visualizador de PDF para Windows (Rust + Tauri 2) que lê **e entrega** os arquivos que vêm dentro do PDF: faturas XML de PDF/A-3, planilhas, imagens, outros PDFs.

## O que ele faz

- **Leitura completa**: rolagem contínua, horizontal, em grade ou página a página; páginas lado a lado; zoom (Ctrl+roda, presets, ajustar à largura/página); rotação; seleção e cópia de texto; links internos.
- **Navegação**: miniaturas desenhadas sob demanda, sumário (outline) expansível, campo de página, `Home`/`End`, `n`/`p`.
- **Busca** no texto com destaque de todas as ocorrências, contador, maiúsculas e palavras inteiras.
- **Abas**, recentes, arrastar e soltar, "Abrir com" / duplo clique (associação `.pdf`) e instância única — o segundo arquivo abre numa aba nova.
- **PDF protegido** por senha (a mesma senha libera a leitura dos anexos).
- **Anexos**, pelos três caminhos do formato — árvore `EmbeddedFiles`, arquivos associados `/AF` (PDF/A-3, PDF 2.0) e anotações de clipe nas páginas:
  - lista com tipo, tamanho, datas, descrição, página de origem e relação PDF/A-3;
  - **pré-visualização** de imagem e texto; anexo PDF abre **em nova aba** (e os anexos dele também);
  - **pré-visualização de leitura** de `.docx` e `.odt` — títulos, listas, tabelas, negrito/itálico e imagens —, com o formato decidido pelo conteúdo, macros só avisadas e nada externo buscado;
  - **salvar** um ou **todos** de uma vez, sem nunca sobrescrever arquivo existente;
  - **abrir no app padrão**, recusado para executáveis;
  - **conferência de integridade** contra o tamanho e o MD5 declarados no PDF;
  - arquivos citados mas não embutidos aparecem à parte.
- Propriedades do documento, leitura noturna, tema claro/escuro, tela cheia.

## Atalhos

`Ctrl+O` abrir · `Ctrl+W` fechar aba · `Ctrl+Tab` próxima aba · `Ctrl+F` / `F3` buscar · `Ctrl +`/`Ctrl −`/`Ctrl+0` zoom · `Ctrl+R` / `Ctrl+Shift+R` girar · `Ctrl+Shift+A` anexos · `F4` painel lateral · `Ctrl+S` salvar cópia · `Ctrl+D` propriedades · `F11` tela cheia.

## Estrutura

```text
crates/pdf-attachments/   núcleo puro (sem Tauri, sem disco): lê, classifica e extrai anexos
  src/                    índice, travessia, datas PDF, nome seguro, tipo de arquivo
  tests/                  PDFs montados em memória para cada caso
  examples/sample_pdf.rs  gera um PDF de amostra com anexos de todos os tipos
crates/office-preview/    núcleo puro: lê .docx e .odt como entrada hostil e devolve blocos com tipo
apps/viewer/              o app
  src/                    webview TypeScript (pdf.js), design system em styles/
  src-tauri/              casca Rust: comandos IPC, documentos, recentes, escrita segura
```

Por que o pdf.js renderiza e o Rust lê os anexos: os motores de renderização maduros em Rust embrulham C/C++ (MuPDF, PDFium), que a estação de build não compila. O pdf.js é o renderizador mais completo disponível no webview, e os anexos — onde mora o risco de segurança — ficam com o Rust, testado e com as réguas no núcleo.

## Desenvolvimento

```bash
npm install && npm --prefix apps/viewer install
npm run app                 # tauri dev
npm run gate                # o portão da entrega
npm run app:build           # instalador NSIS

# um PDF de amostra para testar à mão
cargo run -p pdf-attachments --example sample_pdf -- amostra.pdf
```

Na estação sem Visual Studio, o alvo MSVC compila via xwin + `rust-lld`, configurado no `.cargo/config.toml` local (ignorado pelo git).

As regras de desenvolvimento estão em [AGENTS.md](AGENTS.md).
