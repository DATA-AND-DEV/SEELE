# A integração da 0.16 — o que a orquestradora confere ao juntar as frentes

As frentes trabalham em `0.16/<frente>` e entram em `versao/0.16` por merge sem
squash, depois de revisadas, com a bateria inteira rodada **numa fila só**: duas
baterias ao mesmo tempo disputam a porta 8384. Enquanto a bateria da integração
roda, as frentes não rodam conformidade.

Esta página guarda o que cada frente avisou que se encontra com outra, para que
nada dependa da memória de uma conversa.

## Encontros conhecidos entre frentes

| onde | quem | o que fazer |
|---|---|---|
| `fuzz/Cargo.lock` | F1 (`1277d85`) e F5 (lote de ferramentas, L1-m3) | o mesmo blob dos dois lados: o merge sai limpo. Conferir que é o mesmo |
| `crates/seele-proto/tests/sementes_do_fuzz.rs` | F1 (construtores de `Hello`/`MediaHeader` e `ALVOS_COM_VERSAO`) e F5 (M1, só o trecho do `datagrama_do_encontro`) | trechos diferentes. Regenerar as sementes com `SEELE_REGERAR_SEMENTES` depois do merge, se o guarda pedir |
| `EndReason` (`seele-ffi/src/types.rs`) e `frases.js` (`MOTIVOS`) | F1 (`206f269c`, quatro razões novas com frases provisórias) e F5 (lote H, a recusa legível do lado do cliente) | o `match` é exaustivo e o guarda `every_reason_a_session_can_end_with_has_a_sentence_in_the_page` cobra uma frase por razão. Juntar as duas listas; as frases da sala ficam provisórias até a casca da sala |
| `crates/seele-core/src/client.rs` | F1 (`impl Client`, `handshake`) e F5 (H: `ConnectError::ChaveNaoProvada`, `classify_connection_error`) | trechos diferentes, conferidos pela F5 contra `a8457f7` |
| `crates/seele-ffi/src/lib.rs` | F1 (reexportação e `material_e2ee: None`) e F5 (H: `drive`, `fim_da_recusa`, `classify_connect_failure`) | trechos diferentes |
| `apps/seele-app/ui/frases.js` | F6 (`0c01558`, `0ba303f`, commits isolados) e F5 (H) | juntar |
| `apps/seele-app/src/main.rs` | F1 (P7b: `api: instalado.api` em `habilitar_mod` e `aplicar_conjunto_de_mods`), F6 (comandos finos, `setup`, `Abertura.pelo_lancador`) e F5 (C, D, E, F, K4 e I, em série nas ondas) | conflitos de texto esperados no `generate_handler!`, no `struct Session` e no fim do módulo de testes |
| `NOME_DA_SALA` | F1 (`seele_proto::transport`, `7e123ee5`) e F7 (`pub const` própria em `seele-server/src/tls.rs:27`) | quando os dois estiverem em `versao/0.16`, a F7 troca a sua por `pub use seele_proto::transport::NOME_DA_SALA;`. Conferir que não sobram duas cópias |

## Guardas a acrescentar na integração

- **O esquema da 0.16 é maior que o da 0.15.** A primeira abertura (F6, tarefa 12)
  só reconhece uma pasta já migrada porque a 0.16 grava um esquema acima de 13
  (`versoes::ESQUEMA_DA_0_15`). A migração 14 da F1 (`4112f96b`) garante isso hoje.
  O guarda afirma que a maior versão de `MIGRATIONS` do `seele-server` é maior que
  `ESQUEMA_DA_0_15`. Ele só pode entrar depois de F1 e F6 estarem juntas.

## Antes de publicar

- **Rodar a bateria inteira no `linha/0.15`** (a 0.15.2), inclusive os 4
  chamadores do `seele-conformance` que a F6 só compilou ao mudar a assinatura de
  `Hospedagem::iniciar` (o conserto da janela do Comandante, `ba99007`).
- **As medidas de campo do protocolo nas duas direções**, com o dono: o app
  0.15.1 publicado contra o `seeled` novo, e o app novo contra o `seeled` 0.15.1
  (plano F1b, P1 e P8).
- **A parte 2 da tarefa 17 da F6**, com tela, no Mac e no Windows, com o dono.
- **O ponto em produção fala o ENC/2**: `sondar --exige-enc2 encontro.seele.app.br`, de
  outra máquina, depois de o dono reimplantar a VPS (`docs/ponto-de-encontro.md`).
  A VPS vai antes do app (spec-mestre, §4).
- **As notas não têm condição pendente**: o `publicar.sh` recusa notas com o marcador
  `<!-- CONDIÇÃO:` (F6, `36274663` e `7e444fe8`).

## As integrações feitas

| quando | o que entrou | commit | bateria |
|---|---|---|---|
| 08/10/2026 | F5 onda 1 (C, H, K2, M1, ferramentas, P54), F1 até `f9fdac74` (protocolo 9, S2, vocabulário, API guardada), F6 até `4e66dbf3` (o launcher, tarefas 1 a 16), F4 até `f937ff1` (etapa 0), e o guarda do esquema (`2a37ffc5`) | `2a37ffc5` | **verde**: fmt, check-runtime, check-api, check-versao, check-vetores, clippy, `cargo test --workspace -- --test-threads=1` (2912 passaram, 0 falharam, 5 ignorados, com `SEELE_EXIGE_CODEC=1`), check-deps, deny licenses e advisories |
| 08/10/2026 | F5 onda 2 (M2, P53, K5, D), F1 até `ac1fae91` (`ModAviso`, `TemaMudou`, P9, P10), F4 até `737a627` (o `seele-web`, o motor em `wasm`, a exceção de `unsafe`, o `cargo xtask web`), F3 até `493aa594` (o rascunho da API 6, o esqueleto do ADR 0056, a M3 de base) e F7 até `85879154` (a janela do Comandante na 0.16, o certificado da sala, a campainha, a agenda, o toque, os ADRs 0059 a 0061) | `4bc764d3`, consertado em `5122f39d` | **verde depois de um conserto**: a primeira rodada reprovou o clippy (um `iter().any()` no `fronteira_do_celular.rs`) e um teste (`todo_crate_do_produto_pode_falar_no_log` cobrava os crates do celular no filtro de log do desktop); o resto verde, com 3005 testes passando. Depois do `5122f39d`: o clippy do workspace inteiro, o `empacotamento` (22/22) e o `xtask` (168/0) rodados de novo, e o `cargo xtask web` à parte (os sete passos) |
| 08/10/2026 | F5 onda 3 (E, G-servidor, J; o conserto do guarda do `frontend` em `9daa0c0`, revisado na integração), F1 até `33bca7a9` (`NOME_DA_SALA`, `verify_strict` e a chave de ordem pequena recusada), F6 até `36274663` (`SEELE_IDENTIDADE`, notas com condição recusadas), F4 até `ec78faa` (o `xtask web` na bateria, os links no `.wasm`), F3 até `c1fda14` (a M3 de base) e F7 até `ed1057f1` (o UPnP por papel, o `ConfereCodigo`, o ADR 0061 aceito) | `0e30ea36` | **verde**: a bateria inteira, agora com o `cargo xtask web` como 11ª etapa (3059 passaram, 0 falharam, 7 ignorados). Sem conflito de texto |

Conflitos de texto, resolvidos à mão: na primeira, o `EndReason` de `crates/seele-ffi/src/types.rs`, onde o lote H e a F1 acrescentaram razões no mesmo lugar. Ficaram as duas listas. Na segunda, o índice dos ADRs (`docs/adr/README.md`), com as linhas do 0056, do 0057 emendado e do 0058.
