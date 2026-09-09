# Task 11 — relatório

## Status

DONE.

## Commit

`b3c88c9` — "docs(teste): o roteiro mede o furo entre pares e o custo de um salto",
único arquivo tocado: `docs/teste-duas-maquinas.md` (+89 linhas).

## Testes

`cargo test --workspace`: **PASS** — 69 suítes, 1672 testes, 0 falhas (rodado
duas vezes; a segunda com log completo salvo para contagem).

## O que o roteiro entrega

Seção nova ao fim de `docs/teste-duas-maquinas.md`: montagem de três máquinas
(ou duas + celular em 4G), tabela de onde ler no `tracing`, repetição de dez
tentativas para a fração de furo, a ressalva de `ComoChegou` sobre faixa de
endereço vs. furo de verdade, e o fechamento marcando a aritmética da malha
como estimativa até os dois números existirem.

## Nomes conferidos no fonte (não copiados do brief sem checar)

- `crate::par::ComoChegou` com `Local`/`Furo` — confirmado, `par.rs:533-538`.
- `ParLigado { conexao, como, ida_e_volta }`, `ida_e_volta` vindo de
  `conexao.rtt()` — confirmado, `par.rs:596-600` e `:983`.
- Evento `"um par ligou"` com campos `par`, `como`, `ida_e_volta` — confirmado,
  `par.rs:669` (a mensagem do brief batia).
- **Corrigido**: o brief citava o evento como `"o par não veio"` e o campo
  `erro` para "o motivo enumerado". O evento real é
  `"o par não veio; a tela vem do servidor"` (`par.rs:988`), e tem **dois**
  campos: `motivo` (o `MotivoDeFalhaDePar` enumerado — o que a tabela pede) e
  `erro` (a string detalhada do `ErroDePar`). Documentei os dois, com o campo
  certo para "o motivo enumerado".
- A ressalva de `ComoChegou` (faixa de endereço, hairpin de NAT, IPv6 global
  sem NAT) reaproveitou a linguagem exata do achado 3 da Task 4
  (`task-4-report.md:169`), que é quem primeiro mediu isso.
- A frase final ("dois números que hoje são estimativa") existe de verdade em
  `docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md` §2 — citei
  o arquivo, não inventei a alegação.

## Preocupação principal — achado que não estava no brief

**O empréstimo de subida não tem hoje nenhuma interface.** Conferi:
`emprestar_subida` existe em `crates/seele-core/src/client.rs` e
`ClientMessage::EmprestarSubida` no protocolo, mas nenhuma das ~90 funções
`#[tauri::command]` de `apps/seele-app/src/main.rs` chama isso, e nenhum
arquivo de `apps/seele-app/ui/` menciona "emprestar" ou "subida". O único
chamador no repositório é o teste de integração
`crates/seele-conformance/tests/tela_por_um_par.rs`, que roda em processo. Isto
bate com uma nota já registrada em `progress.md` (Task 8): *"o caminho de LAN
do §3.1 fica inerte até o opt-in existir na interface."*

Escrevi essa lacuna como o primeiro passo do roteiro ("confira isto antes de
marcar horário com três pessoas") em vez de instruir alguém a clicar num botão
que não existe — é exatamente o defeito que o brief pediu para evitar
("um roteiro que manda procurar uma linha que não existe é pior que não ter
roteiro"). Consequência prática: **o roteiro, como está, não pode ser
executado por uma pessoa sem acesso ao código** até essa ligação existir em
algum lugar clicável — nem que seja provisória. Isso é anterior a qualquer
coisa que a Task 11 pudesse consertar (é documento, sem interface).

## Outra observação, fora do escopo desta tarefa

`docs/teste-duas-maquinas.md` já referenciava `./target/release/connection`
nas seções 1–6, mas esse binário (a antiga TUI, `seele-tui`) foi removido pelo
ADR 0039 — hoje o workspace só produz `seeled`, `seele-app` (Tauri) e
`seele-instalador`. Não toquei nessas seções (fora do escopo do brief, que
pede só a seção nova ao fim), mas registro que o documento já estava
desatualizado antes desta tarefa.

---

## Fix round 1/5

### Status

DONE.

### Commit

`725be01` — "fix(teste): quatro consertos de revisão no roteiro do caminho
entre pares", em `docs/teste-duas-maquinas.md` e
`crates/seele-server/src/pares.rs` (17 inserções, 8 remoções).

### Testes

`cargo test --workspace`: **PASS** — 69 suítes, 1672 testes, 0 falhas.

### Os quatro consertos

1. **Important** — "o primeiro que declarou" (`Pares::escolher`) trocado por
   "a escolha é automática e não segue critério visível nenhum — nem
   latência, nem ordem de chegada", tanto no roteiro quanto na doc de
   `crates/seele-server/src/pares.rs`, onde a frase errada nasceu (diferida
   da Task 7). Confirmado no fonte: `self.quem` é `HashMap<PersonId,
   QuemDeclarou>`, e `escolher` usa `.values().find(...)` — ordem de
   iteração não é ordem de inserção.
2. **Minor** — "repita umas dez vezes" ganhou parágrafo dizendo que dez é
   piso, não meta, e que mais tentativas e mais redes distintas produzem um
   número melhor.
3. **Minor** — passo 2 da montagem ("ligue o empréstimo de subida") marcado
   com **(bloqueado)** explícito, em vez de só remeter ao aviso anterior.
4. **Minor** — a linha de `ida_e_volta` na tabela agora avisa que o campo já
   sai formatado com unidade (`Duration` via `?ida_e_volta`, ex. `12.345ms`),
   não como número cru.
