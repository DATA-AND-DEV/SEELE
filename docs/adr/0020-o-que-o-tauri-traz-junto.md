# 0020 — O que o Tauri traz junto, e por que aceitamos

Status: aceito

Contexto: `specs/06-clientes-gui.md` decide Tauri em vez de Electron, e a decisão é boa — a webview é do sistema, o binário fica em dezenas de MB e o núcleo Rust continua no processo nativo. O que a spec não dimensionou é a árvore de dependências que vem junto, e ela só ficou visível quando o `cargo deny` rodou pela primeira vez com o crate do desktop dentro do workspace:

- **16 avisos `unmaintained`.** Nenhuma vulnerabilidade. Dez são os bindings GTK3 do gtk-rs, alcançados só no Linux e só através do `webkit2gtk`, que é a webview do sistema lá. Cinco são tabelas Unicode do `unic-*`, usadas no tratamento de URL do Tauri. Um é o `proc-macro-error`, que existe apenas em tempo de compilação.
- **5 crates sob MPL-2.0**: `cssparser`, `cssparser-macros`, `selectors`, `dtoa-short` e `option-ext`. Os três primeiros são o parser de CSS do Servo. O `deny.toml` negava MPL por omissão.

Isso é uma ampliação real da superfície auditada de um produto cujo argumento é ser auto-hospedado por quem não confia em terceiros. Merece ser uma decisão registrada e não um `ignore` genérico colocado para o build passar.

Decisão: aceitar, com as exceções **nomeadas uma a uma** no `deny.toml`, cada uma com o motivo.

- Cada `RUSTSEC-` está listado pelo identificador. Nenhum `ignore` por padrão, nenhum por categoria. **Nenhuma vulnerabilidade é ignorada** — a lista inteira é `unmaintained`, e no dia em que qualquer um desses virar vulnerabilidade ele deixa de casar com a entrada e quebra o build. Essa é a propriedade que importa, e é por isso que a lista é de identificadores e não de nomes de crate.
- MPL-2.0 entra na lista de licenças permitidas com o raciocínio escrito ao lado: é copyleft por arquivo, alcança os arquivos que cobre e não o trabalho que os liga. Usamos os cinco sem modificar. Se algum dia modificarmos um, aqueles arquivos precisam ser publicados — é a obrigação inteira, e é uma que conseguimos cumprir.

Alternativas:

1. **Abandonar o Tauri.** Contraria a spec e não melhora o quadro: Electron traz uma árvore npm inteira, que é a auditoria que o `cargo deny` não faz e que o ADR 0019 justamente evitou no frontend.
2. **`ignore` genérico de `unmaintained`.** Faria o build passar hoje e passaria a esconder o próximo — inclusive os que apareceriam em dependências nossas, não do Tauri. O ganho de digitação não paga.
3. **Esperar o Tauri migrar para GTK4.** É trabalho de terceiros, sem data, e M5 não pode depender dele.

Consequências:

- O `deny.toml` fica maior e mais informativo. A lista de identificadores é uma lista de revisão: cada linha diz por onde a dependência entra, e isso é o que permite decidir rápido quando uma delas mudar de status.
- **A régua a observar não é "quantos itens tem a lista", é "algum deixou de ser `unmaintained`".** A verificação continua rodando em cada build, e é ela que avisa.
- Quando o Tauri migrar para GTK4, dez destas somem sozinhas. Vale reconferir a lista naquele momento em vez de deixá-la envelhecer.
- Vale registrar no `specs/06-clientes-gui.md` que a escolha do Tauri tem esse custo, para que a próxima pessoa não descubra do mesmo jeito que eu descobri.

Custo de reverter: **baixo** para as exceções — apagar linhas do `deny.toml`. **Alto** para a escolha do Tauri, assim que o app tiver usuários.
