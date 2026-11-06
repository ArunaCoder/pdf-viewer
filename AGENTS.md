# AGENTS.md — Diretrizes de Desenvolvimento (Violeta PDF)

Regras obrigatórias de qualidade e segurança. Nenhuma entrega é aceita sem cumpri-las.

## 1. Terminal

- Dialeto **Unix** na ferramenta `Bash` (Git Bash). PowerShell só para cmdlet Windows sem equivalente.
- Ler/buscar com **Read, Glob, Grep**; editar com **Edit/Write**, nunca edição in-place por shell.
- **Git é somente-leitura para o agente**: nada de `commit`, `push`, `pull` nem comando destrutivo. A entrega termina no working tree.
- Veredito de gate não se lê por pipe: `<comando> > reports/<nome>.txt 2>&1; echo "exit=$?"`.

## 2. Idioma

- **Português**: chat, documentação, comentários, doc-comments e todo texto visível ao usuário do app.
- **Inglês**: identificadores, chaves JSON, códigos de erro e mensagens de diagnóstico (log, `Display` de erro). Erro que chega à interface é traduzido no webview pelo `code` (`errorMessage` em `apps/viewer/src/ipc.ts`), nunca exibido cru.
- Exceção: mensagem de asserção de teste fica em português, como o comentário ao lado dela.

## 3. Verificação obrigatória

O portão da entrega é **um comando**: `npm run gate` (Biome, `tsc`, clippy pedantic + nursery com `-D warnings`, `rustfmt`, `cargo doc`, `cargo test`, Vitest, build do webview, `cargo deny` e `npm audit`).

- **Zero novas supressões** (`#[allow]`, `#[expect]`, `biome-ignore`, `@ts-expect-error`). As configs (`Cargo.toml` `[workspace.lints]`, `clippy.toml`, `biome.json`, `tsconfig.json`) são a fonte da verdade e não se afrouxam para calar erro. A única exceção é a supressão *inevitável* — causa externa nomeada (bug do lint, limitação do toolchain) —, com `#[expect]` e a causa escrita na linha.
- Falso positivo de lint pede reescrita equivalente antes de qualquer outra coisa (ex.: o `grid-template-areas` trocado por posicionamento explícito em `app.css`).
- **Mudança de comportamento entra com teste**; correção de bug, com teste de regressão.

## 4. Rust

- `unsafe_code = "forbid"`. Sem `unwrap`/`expect`/`panic!`/`unreachable!`/indexação que pode panicar (`indexing_slicing`) nem `as` entre inteiros: fluxo falível devolve `Result`, conversão é `try_from`.
- Erros tipados com `thiserror`; a fronteira IPC devolve `IpcError { code, message }`. O union TS dos códigos é **gerado** pelo teste `error::tests::generated_ts_matches_the_enum` (`REGENERATE=1` para reescrever).
- Tipo serializado que o webview lê tem a forma do fio travada por teste contra JSON literal (`ipc/dto.rs`).
- Dependência nova: versão do registry, fixada em `major.minor.patch`, com o motivo comentado no manifesto. Nada que exija compilador C — esta estação compila o alvo MSVC via xwin, que não fornece um.
- Logging com `log`/`tauri-plugin-log`; `println!` é proibido fora do `build.rs`.

## 5. Fronteiras de segurança (não negociáveis)

O PDF é **entrada hostil**. O crate `pdf-attachments` é o dono das réguas, e o resto as aplica:

- Leitura com teto de descompressão (`MAX_OBJECT_STREAM_BYTES`, `ExtractLimits`), travessia de árvore com profundidade máxima e detecção de ciclo.
- Nome de anexo só vira caminho por `file_name::safe_file_name`; executável nunca é entregue ao sistema (`kind::is_executable`); todo arquivo extraído leva a marca de download (`Zone.Identifier`).
- **Nenhum caminho de arquivo vem do webview**: abrir e salvar usam o seletor nativo chamado pelo Rust; documento e anexo trafegam por id. Diálogos, escrita e `opener` não têm permissão na capability.
- Links externos só `http`, `https` e `mailto`, abertos pelo Rust; a navegação da janela é recusada (`navigation.rs`). CSP sem script externo nem `eval`.
- Comando novo entra no `generate_handler!`, no `COMMANDS` do `build.rs` e na capability — o teste `every_command_is_in_the_manifest_and_the_capability` confere os três.

## 6. TypeScript

- `strict` + `noUncheckedIndexedAccess` + `exactOptionalPropertyTypes`. DOM só pelo helper `el` (`dom.ts`): nada de `innerHTML` — todo texto do PDF entra como nó de texto.
- O webview não reimplementa veredito do núcleo (tipo de anexo, executável, integridade): só o mostra.
- Cores, raios, sombras e movimento saem dos tokens de `styles/tokens.css`; componente não inventa valor.

## 7. Decisão

Decida e aja na forma da implementação, no conserto de defeito e na aplicação destas regras. Pergunte ao usuário quando houver perda de trabalho, mudança de produto visível, direção que o trabalho seguinte herda ou algo irreversível — em texto, com alternativas `1)`, `2)`, `3)` e a recomendação marcada.
