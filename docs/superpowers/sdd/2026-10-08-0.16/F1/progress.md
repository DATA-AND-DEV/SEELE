# F1 — andamento

Plano: `docs/superpowers/plans/2026-10-08-0.16-F1-protocolo.md`. Branch `0.16/protocolo`, de `versao/0.16` em `d4eb6f1` (a orquestradora citou `c0337b2`; entre os dois só há `d4eb6f1`, um commit de documento do spec-mestre).

| tarefa | estado | commit | nota |
|---|---|---|---|
| 0 o plano | feito | `1f76d16`, `bbc6953` | a 13b entrou a pedido da orquestradora |
| 1 o adendo de negociação | feito | `fafe105` | |
| 2 o ADR 0058 | feito | `2569bc6` | o guarda `adr_index` do xtask passa |
| 3 o enquadramento no `seele-proto` | feito | `ce90b19` | reversão: `to_le_bytes` reprova 4 de 8; `wasm32` compila |
| pedidos da F6 (a), (b), (e) | feito | `aa8574e` | o ffi reexporta pelo `seele_core` (ADR 0002) |
| 4 a faixa, a linha e o 9 | feito | `a435b93` | reversão: `VERSAO_MINIMA = 8` reprova 3; fuzz `version_negotiation` compilado e rodado |
| (lock do fuzz) | feito | `1277d85` | o lock não tinha o `serde_json` do `seele-proto` |
| 5 o carimbo e a recusa legível | feito | `ce70c50` | reversão: carimbar sempre o próprio número reprova |
| 6 a mensagem desconhecida | feito | `4b4be35` | duas reversões: sem o pulo, reprovam 2; `CONHECIDAS = 40`, reprova 1 |
| 7 o número da voz | feito | `f3aaf9c` | reversão: comparar com o protocolo reprova 3; fuzz `media_header` rodado num corpus de rascunho |
| 8 o assento do E2EE | feito | `d36e7cf2` | reversão: sem a conferência do perfil, reprova 1. O teto de tamanho do `Hello` de teste foi de 64 para 128: medido em 64 depois da faixa e do `None` |
| 9 os bytes-ouro (`fio_1_0.rs`) | feito | `d0b0f11b` | duas reversões: trocar a ordem de `AlertSeverity` reprova `servidor:Alert`; com o estado `congelado`, regerar um quadro mudado reprova |
| 10 os enums aninhados | feito | `3b2bb4eb` | reversão: `Kicked` depois de `Banned` reprova, **e os bytes-ouro não pegam essa troca** — é o buraco que este guarda cobre |
| 11 a PUBLICADA real | feito | `b423843c` | com `VERSAO_MINIMA = 8`, a v0.15.1 real reprova; com o 3 de antes, o mesmo erro passa. Corrigida no plano a reversão: voltar os dois números para 8 põe a build na linha da 0.15, e aí o guarda passa com razão |
| 12 o helper do S2 | feito | `a5af6ec6` | duas reversões: o `MORO2` sem NUL reprova; o nonce de até 256 bytes reprova |
| 13 o S2 no fio | feito | `67619142` | RED antes: a prova amarrada era recusada e a do nonce cru entrava. Reversões: o servidor conferindo contra zeros reprova `prova_amarrada`; o cliente assinando o nonce cru reprova os dois de `recusa` |
| 13b o lado do cliente | feito | `33d23400` | reversão: o cliente tirando a impressão de outra fonte (zeros) reprova |

## O lote do vocabulário (liberado em 08/10)

| parte | estado | commit | nota |
|---|---|---|---|
| plano do lote | feito | `99caa927` | a razão de outra linha **não** entra no fio (decisão `7abfb500`) |
| V1 o `Codigo` | feito | `b10797ec` | quatro vetores calculados por uma referência independente em Python; `vetores-de-codigo.json` cruzado com ela. Reversões: fator do Luhn reprova 4; corte de bits, 3 |
| V2 `Tocando` | feito | `567fa409` | ordinal 41. Reversão: sem o exemplar, a completude acusa `Tocando` |
| V3 o FORA DO AR | feito | `2a70021b` | `Membros { membros, fim }` (42), `MembroNovo` (43) e `MembroBanido` (44); o renomear é o `PersonRenamed`. Reversão: sem o teto de membros, reprova |
| V4 a API de MOD no fio | feito | `49de9d45` | `Hello.apis_de_mod` e `ModAnunciado.api`; o servidor manda 0 («não preenchida»). Reversão: sem o teto de APIs, reprova |
| V5 razões 18–21 | feito | `206f269c` | `ChamadaEncerrada`, `NinguemAtendeu`, `ChamadaRecusada` (terminais no `enlace.rs`) e `ModsDeOutraApi { apis: u64 }`, em máscara para a razão continuar `Copy`. Tocou a FFI (`EndReason`) e o `frases.js` (frases provisórias), que o guarda da casca exige. Três reversões |
| F4: hashes do navegador | feito | `64792dee` | `HashesDoNavegador` (45), as constantes das janelas, o `ALPN_DO_NAVEGADOR` e o doc do `Server::impressao`. Bytes conferidos por uma conta independente. Reversão: sem a ordem, reprova |

## Verificação final (08/10)

- `cargo fmt --all -- --check` limpo; `cargo clippy --workspace --all-targets` sem aviso.
- `cargo test -p seele-proto`: 295 + 4 + 5 + 1 + 1, todos verdes. `wasm32` compila.
- `cargo test -p xtask`: 141 verdes.
- Conformidade rodada por arquivo: `prova_amarrada`, `recusa`, `aceite_dos_mods`, `acceptance_m2`, `acceptance_seguranca`, `convite`, `ejetar`, `tela_por_um_par`, `desassentar`.
- `seele-server`: `--lib session::`, `mods::`, `voice_room::` e os sete testes de integração com par cru. `seele-core`: `--lib enlace::`, `client::`, `state::`. `seele-ffi --lib`.
- **Não rodei** a bateria inteira nem `cargo test --workspace`, como pede a regra da porta 8384.

## Medidas desta sessão

- `cargo check -p seele-proto --target wasm32-unknown-unknown` passa em `d4eb6f1` (3,7 s). Medido em 08/10.
- Publicada, pela API de releases em 08/10: v0.15.1, do commit `54f542f`. Lá, `PROTOCOL_VERSION = 8` (`version.rs:124`) e `COMPATIBILITY_WINDOW = 1` (`:164`).

## Pendências para a orquestradora

- O portão do Plano 2: a orquestradora o trocou pelas Tarefas 13 e 13b (08/10).
- O lugar do assento do E2EE (`Hello` e `PersonProfile`) é escolha técnica minha (adendo §10), aberta até o 9 congelar.
- O ADR novo leva o número 0058. Nenhuma branch tinha 0058 em 08/10.
