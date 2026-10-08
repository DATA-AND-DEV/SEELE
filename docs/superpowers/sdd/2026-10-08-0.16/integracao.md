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
