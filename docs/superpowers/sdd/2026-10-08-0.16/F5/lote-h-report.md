# Lote H da F5 — relatório

A recusa legível, metade cliente: a volta recusada diz o motivo de verdade (P51-03), o impostor que não prova a chave
deixa de parecer «versão incompatível» (P51-02), e o doc de um guarda do `frontend.rs` (K1-m1).

Base `fbeff8e`, branch `0.16/lotes-h`, worktree `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a9198e5ed1ed9f4eb`.
Três commits de código e doc e este relatório, todos locais, nenhum empurrado. Todos terminam com
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. **G1:** nenhum assunto contém uma das catorze cadeias
(conferido com `grep -F -f` das linhas entre `CADEIAS` da receita do plano 1B contra os quatro assuntos, este incluído).

| Commit | Item |
|---|---|
| `4807207b` | P51-03 — a volta recusada diz o motivo |
| `663363d4` | P51-02 — o impostor que não prova a chave |
| `b570f86a` | K1-m1 — o doc do guarda de `anotarRecusa` |

Ordem seguida: o P51-03 inteiro, depois o P51-02, depois o K1-m1.

## O que foi feito, por item

### 1. P51-03 — `4807207b`

- **O tipo.** `Motivo::Recusado(String)` passou a `Recusado(ConnectError)` (`enlace.rs`). `Motivo` continua derivando
  `Debug, Clone, PartialEq, Eq`. O ponto de produção passou de `Motivo::Recusado(format!("{erro:?}"))` a
  `Motivo::Recusado(erro)`. O doc de `Recusado` diz por que leva o tipo, e o de `Moderado` deixou de dizer que
  `Recusado` vira `CredentialRejected`.
- **A linha no registro.** No braço `Err(erro) if !vale_insistir(&erro)`, antes do `encerrar`:
  `tracing::warn!(?erro, "a volta da bateria foi recusada, e a sessão acaba")`.
- **`fim_da_recusa(&ConnectError) -> EndReason`** (`seele-ffi/src/lib.rs`, logo antes de `classify_connect_failure`),
  exaustiva e sem `_`: `Refused { reason }` → `(*reason).into()`; `InviteMismatch` → `ServidorTrocouNaVolta` (nova);
  `PinChanged` → `ChaveMudouNaVolta` (nova); `ModsNaoAceitos` → `ModsMudaram`; `LocalEndpoint`, `Unreachable`,
  `TlsRefused`, `HandshakeTimeout`, `SemResposta` e `ProtocolViolation` → `LinkLost`, cada uma nomeada. As variantes
  novas de `EndReason` são unitárias (o enum é `Copy, Hash`).
- **A fiação, com uma diferença do brief.** O `match motivo` que ficava dentro do `Aviso::Encerrado(motivo)` de
  `drive` saiu para `fn fim_da_sessao(&Motivo) -> EndReason`, que `drive` chama (`reason: fim_da_sessao(&motivo)`), e o
  braço `Recusado(erro) => fim_da_recusa(erro)` mora nela. Os outros três braços são os de antes, com o comentário do
  `Moderado`. O motivo: a reversão que o brief pede («voltar o braço a `CredentialRejected` deixa o teste da FFI
  vermelho») só é possível se o teste passar pelo braço; dentro de `drive`, que precisa de uma sessão viva, nenhum
  teste o alcançava, e o teste de `fim_da_recusa` sozinho ficaria verde com o braço revertido.
- **As frases.** Em `MOTIVOS`, `ServidorTrocouNaVolta` e `ChaveMudouNaVolta`, com o texto exato do brief e um
  comentário de bloco.
- **Os testes que liam o texto passaram a ler o tipo.** `a_volta_da_bateria_onde_o_pino_nao_prova_o_servidor_e_recusada_antes_do_hello`
  usa `matches!(aviso, Aviso::Encerrado(Motivo::Recusado(ConnectError::InviteMismatch { .. })))`.
  `a_volta_da_bateria_recusa_no_tls_o_impostor_que_a_impressao_desmente` (`bateria_interna.rs`) casa
  `Recusado(ConnectError::InviteMismatch { expected, offered })` e confere `expected == real` e `offered == ofertada`,
  cada um com a sua mensagem.
- **Testes novos.**
  - `seele-ffi`, módulo `a_volta_recusada_diz_o_motivo`:
    - `fim_da_recusa_diz_o_motivo_de_verdade_e_nao_credencial_recusada` passa por `fim_da_sessao(&Motivo::Recusado(…))`
      e junta os casos errados numa lista, para o vermelho mostrar todos de uma vez;
    - `fim_da_recusa_do_que_nao_e_recusa_e_enlace_perdido` cobre as seis de rede.
  - `enlace.rs`: `a_volta_recusada_diz_no_registro_por_que_a_sessao_acabou`, de thread única, com
    `crate::rastro_de_teste::Rastro` em `WARN`. Exige uma linha `WARN` com «a volta da bateria foi recusada» e
    `InviteMismatch`.

### 2. P51-02 — `663363d4`

- **`ConnectError::ChaveNaoProvada`** (`client.rs`), com doc: o certificado público repetido, a assinatura que o TLS
  cobra depois do verificador, o alerta `decrypt_error` (51), a recusa antes do `Hello`, e o `None` que fica
  `TlsRefused`.
- **`classify_connection_error`.** Um braço novo, antes do de sempre:
  `Some(FirstContact | Matches) if matches!(error, TransportError(e) if e.code == TransportErrorCode::crypto(51))` →
  `warn!` + `ChaveNaoProvada`. O braço de sempre (`Some(FirstContact | Matches) | None => match error {…}`) ficou como
  era. Por isso `None` com o 51 e os outros `TransportError` continuam `TlsRefused`. O comentário diz por que o
  campo é `TransportError(e).code` e não o `ConnectionClosed` de `par::motivo_da_recusa`.
- **`alguem_respondeu`** dá `true` para `ChaveNaoProvada`, e **`vale_insistir`** dá `false`, com comentário. Os dois
  continuam exaustivos. `desfazer_o_pin_deste_aperto` não mudou.
- **`fim_da_recusa(ChaveNaoProvada)`** → `ServidorTrocouNaVolta`, no mesmo braço do `InviteMismatch`. O doc de
  `EndReason::ServidorTrocouNaVolta` passou a citar os dois.
- **`ConnectionError::ChaveNaoProvada`** (`types.rs`), unitária, com doc. Ela é traduzida em
  `classify_connect_failure`, que continua exaustiva e antes a mandava (como `TlsRefused`) para
  `Refused { Incompatible }`.
- **A frase.** Em `FRASES`, `ChaveNaoProvada` leva o texto exato do brief. Ela fica logo depois de `SemResposta`, com
  um comentário.
- **Os docs.**
  - O de `TlsRefused` diz o que sobra nele: outro alerta, ou nenhuma decisão do verificador.
  - O parágrafo de `desfazer_o_pin_deste_aperto` lista agora `ChaveNaoProvada` entre as que saem do próprio TLS. O
    impostor de cima passou a ser `ChaveNaoProvada`, e `TlsRefused` virou «o TLS também caiu, por outro alerta».
- **Testes.**
  - `o_pin_que_um_impostor_fez_fixar_e_desfeito_e_o_log_nao_diz_que_o_tls_terminou` exige
    `Some(ConnectError::ChaveNaoProvada)` e procura `ChaveNaoProvada` na linha do pin desfeito.
  - `client.rs`, módulo `a_recusa_do_tls_chega_com_nome`:
    - `sem_recusa_do_verificador_a_classificacao_e_a_de_sempre` continua verde, sem mudança;
    - três casos novos: `o_tls_que_cai_na_assinatura_depois_de_o_verificador_aceitar_e_chave_nao_provada` (`Matches`
      e `FirstContact` com o 51), `outro_alerta_do_tls_continua_tls_refused` (120, `no_application_protocol`) e
      `sem_decisao_do_verificador_o_alerta_da_assinatura_continua_tls_refused` (`None` com o 51).
  - `enlace.rs`: `quem_nao_prova_a_chave_respondeu_e_insistir_nao_muda_a_resposta`, sobre os dois predicados.
  - `seele-ffi`:
    - `quem_nao_prova_a_chave_nao_chega_como_versao_incompativel`;
    - o caso `ChaveNaoProvada` → `ServidorTrocouNaVolta` no teste de `fim_da_recusa`.
  - `frontend.rs`: `quem_nao_prova_a_chave_tem_frase_propria_e_nao_a_de_versao_incompativel`.
    - Ele tira o nome do serde (`serde_json::to_string(&ConnectionError::ChaveNaoProvada)`) e lê a entrada em `FRASES`
      por `sentences_of`, sem comentários. Exige «NÃO PROVOU».
    - Fica restrito a esta variante, e não cobra todas as de `ConnectionError` contra `FRASES`: cinco delas
      (`CaptureDeviceGone`, `PlaybackDeviceGone`, `UnknownMessage`, `IconNotAPicture` e `IconTooBig`) têm a frase em
      `tela-server.js`, e um guarda geral contra `FRASES` reprovaria pelo que já é assim. O doc do teste diz isso.

### 3. K1-m1 — `b570f86a`

- No doc de `o_dono_da_regiao_leva_cada_recusa_de_midia_ao_registro`:
  - «Fora daqui, só a bancada de navegador … segura o caso» virou «Fora daqui, até `4599e14`, só … segurava»;
  - «ficam verdes» virou «ficavam verdes», e a data «(medido em 01/10/2026)» ficou como registro;
  - entrou a frase: desde `4599e14`, o bloco «o elo» da `contribuicoes-e-camadas.cjs`, que o `check-runtime` roda,
    também reprova (`confere("o elo · anotarRecusa", …)`), com o dono sem `anotarRecusa` e com o `meu()` invertido.
- **Medido de novo, e não copiado do commit `4599e14`.** As duas mutações foram feitas à mão em `ui/base.js`, e a
  bancada rodou com cada uma (`node apps/seele-app/bancada/contribuicoes-e-camadas.cjs`):
  - sem `anotarRecusa` (a chave renomeada), ela sai com `contribuicoes-e-camadas: reprovou` e reprova duas vezes em
    `o elo · anotarRecusa`: «o dono de `base.js` não tem `anotarRecusa`…» e «a recusa de mídia não saiu como um
    `registrar_da_janela` só…»;
  - com o `meu()` invertido (`if (meu()) return;`), reprova `o elo · anotarRecusa` e os três casos
    `o elo · anotarRecusa fora da vez`.
- `base.js` foi restaurado da cópia, e o `shasum` antes e depois é o mesmo (`a7c18777…`). O `git status` saiu limpo
  em `base.js`. É só doc, e não teve reversão.

## TDD — RED e GREEN

### RED 1: o tipo de P51-03 mudado, com `fim_da_recusa` devolvendo `CredentialRejected` para tudo

Comando: `CARGO_BUILD_JOBS=8 perl -e 'alarm 600; exec @ARGV' cargo test -p seele-ffi fim_da_recusa`

```
a volta recusada chega à tela com um motivo que não é o dela, e a frase manda a pessoa conferir o que não está errado: [
    "InviteMismatch deu CredentialRejected, e não ServidorTrocouNaVolta",
    "Refused { ServerFull } deu CredentialRejected, e não ServerFull",
    "PinChanged deu CredentialRejected, e não ChaveMudouNaVolta",
    "ModsNaoAceitos deu CredentialRejected, e não ModsMudaram",
]
fim_da_recusa_do_que_nao_e_recusa_e_enlace_perdido:
assertion `left == right` failed: a volta que acabou em LocalEndpoint chega à tela como recusa de credencial, e manda conferir convite e senha por uma falha de rede
  left: CredentialRejected
 right: LinkLost
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 121 filtered out
```

Era o esperado: os quatro casos do brief vermelhos, pelo comportamento de hoje (`CredentialRejected` para tudo), e
não por compilação.

### RED 2: o teste de rastro, sem o `warn!`

Comando: `… cargo test -p seele-core --lib a_volta_`

```
a volta recusada acabou com a sessão sem dizer no `seele.log` qual recusa foi: quem relata «caiu e não voltou» não tem o que mostrar. Linhas: ["WARN: havia um pino que conferia com a chave de quem atendeu, e a impressão esperada decidiu: o pino deste destino não prova o servidor (um candidato que a pessoa não escolheu, ou um alvo de escopo local). Recusado dentro do TLS, antes do `Hello`; o pino fica como estava chave=127.0.0.1:60750 esperada=abab… ofertada=59b2…"]
test result: FAILED. 6 passed; 1 failed
```

Era o esperado: a volta acabou recusada (a primeira asserção passou), e a única linha `WARN` era a do verificador.
Nenhuma dizia que a sessão acabou.

### RED 3: as variantes novas de `EndReason` sem frase

Comando: `… cargo test -p seele-app --test frontend every_reason_a_session_can_end`

```
these ways a session can end reach the screen with no sentence written for them, so it says nothing or says the name of a Rust variant: ["ServidorTrocouNaVolta", "ChaveMudouNaVolta"]
```

### GREEN do P51-03

Depois do mapeamento, do `warn!` e das frases:

- `fim_da_recusa`: 2 passed;
- `a_volta_`: 7 passed;
- `frontend`: 259 passed;
- `-p seele-core enlace`: 61 passed;
- `bateria_interna`: 6 passed, com a volta do impostor lendo o tipo.

### RED 4: P51-02, com a variante existindo e `classify_connection_error` ainda sem ler o alerta

Comando:
`… cargo test -p seele-core --lib -- a_recusa_do_tls_chega_com_nome o_pin_que_um_impostor quem_nao_prova_a_chave`

```
o_tls_que_cai_na_assinatura_depois_de_o_verificador_aceitar_e_chave_nao_provada:
assertion `left == right` failed: com Matches { fingerprint: "aaaa1111" }, quem não provou a chave saiu com outro nome, e a tela o chamaria de versão incompatível
  left: TlsRefused
 right: ChaveNaoProvada
o_pin_que_um_impostor_fez_fixar_e_desfeito_e_o_log_nao_diz_que_o_tls_terminou:
assertion `left == right` failed: o impostor que mostrou o certificado sem a chave não saiu como `ChaveNaoProvada`: a tela o chamaria de versão incompatível, ou o teste mediu outra coisa
  left: Some(TlsRefused)
 right: Some(ChaveNaoProvada)
test result: FAILED. 5 passed; 2 failed
```

Era o esperado:

- os dois que pedem o nome novo reprovam com o `TlsRefused` de hoje;
- o caso do 120, o do `None` e os dois predicados já passam, como deviam;
- o impostor de verdade (`impostor_que_repete_o_certificado`) chega como `TlsRefused`.

O GREEN abaixo mede o número do alerta: com o `rustls` 0.23.45 do `Cargo.lock`, o impostor dá `crypto(51)`.

### RED 5: a frase de `ChaveNaoProvada` faltando

Comando: `… cargo test -p seele-app --test frontend quem_nao_prova_a_chave`

```
`ConnectionError::ChaveNaoProvada` chega à tela sem frase em `FRASES`, e a tela escreve o nome de uma variante do Rust a quem acabou de bater num impostor
```

### GREEN do P51-02 e do lote inteiro

Rodados depois do `cargo fmt --all`, na árvore de `663363d4`. O K1-m1 só mexe em doc, e o `frontend` rodou de novo
depois dele.

| Comando (`CARGO_BUILD_JOBS=8 perl -e 'alarm 600; exec @ARGV' cargo test …`) | Resultado |
|---|---|
| `-p seele-core enlace` | 62 passed |
| `-p seele-core client` | 20 passed |
| `-p seele-ffi fim_da_recusa` | 2 passed |
| `-p seele-ffi --lib` | 123 passed, 1 ignored (o ignorado é anterior ao lote) |
| `-p seele-conformance --test bateria_interna` | 6 passed |
| `-p seele-conformance --test acceptance_m5 --test estados` | 15 passed, 10 passed |
| `-p seele-app --test frontend` | 260 passed (também depois do K1-m1) |

- `cargo xtask check-runtime`: as sete bancadas de node passaram.
- `cargo xtask check-api`: verde, com «5 versão(ões)» e nenhum `api/*.json` mexido.
- `cargo fmt --all -- --check`: limpo.
- O clippy de `seele-core`, `seele-ffi`, `seele-conformance` e `seele-app` (`--all-targets --all-features -- -D warnings`)
  rodou antes de cada commit. Saiu limpo: o único aviso foi o `ld: duplicate -rpath`, que é ambiental.

## As reversões

Cada guarda novo foi provado desfazendo o conserto, vendo o vermelho com a frase dele e restaurando. A restauração foi
conferida byte a byte pelo `shasum`, igual antes e depois.

| Reversão | Vermelho | Restaurado |
|---|---|---|
| Em `fim_da_sessao`, `Recusado(erro) => fim_da_recusa(erro)` de volta a `Recusado(_) => EndReason::CredentialRejected` | Os dois testes `fim_da_recusa_*`, com a mesma saída do RED 1 (os quatro casos, mais o `LocalEndpoint`) | `lib.rs` `30957a45…` |
| Sem o `warn!` da volta recusada | `a_volta_recusada_diz_no_registro_por_que_a_sessao_acabou`, com a frase do RED 2: só a linha do verificador | `enlace.rs` `a063053b…` |
| Sem o braço do alerta 51 em `classify_connection_error` | O impostor de verdade: «o impostor que mostrou o certificado sem a chave não saiu como `ChaveNaoProvada`…», `left: Some(TlsRefused)` | `client.rs` `a955cf77…` |
| Sem a entrada `ChaveNaoProvada` em `FRASES` | `quem_nao_prova_a_chave_tem_frase_propria…`: «`ConnectionError::ChaveNaoProvada` chega à tela sem frase em `FRASES`…» | `frases.js` `e9440a5e…` |
| Na FFI, `classify_connect_failure(ChaveNaoProvada)` de volta a `Refused { Incompatible }`, e `fim_da_recusa(ChaveNaoProvada)` em `LinkLost` (as duas juntas) | `quem_nao_prova_a_chave_nao_chega_como_versao_incompativel` (`left: Refused { reason: Incompatible }`), e o de `fim_da_recusa` com «ChaveNaoProvada deu LinkLost, e não ServidorTrocouNaVolta» | `lib.rs` `071960b4…` |

Os guardas das frases novas de `MOTIVOS` (P51-03) e de `FRASES` (P51-02) foram vistos vermelhos antes de a frase
existir (RED 3 e RED 5). A reversão da frase de `FRASES` repetiu esse vermelho.

## Os arquivos mudados

- `crates/seele-core/src/client.rs`: a variante, o braço do alerta 51, o doc de `TlsRefused` e três testes.
- `crates/seele-core/src/enlace.rs`:
  - o tipo de `Motivo::Recusado`, o `warn!` e os dois predicados;
  - o doc de `desfazer_o_pin_deste_aperto`;
  - os dois testes que liam texto e o impostor;
  - dois testes novos.
- `crates/seele-ffi/src/lib.rs`: `fim_da_sessao`, `fim_da_recusa`, o braço novo de `classify_connect_failure` e o
  módulo de teste `a_volta_recusada_diz_o_motivo`.
- `crates/seele-ffi/src/types.rs`: `EndReason::{ServidorTrocouNaVolta, ChaveMudouNaVolta}` e
  `ConnectionError::ChaveNaoProvada`.
- `apps/seele-app/ui/frases.js`: duas entradas em `MOTIVOS` e uma em `FRASES`.
- `apps/seele-app/tests/frontend.rs`: o guarda novo de `ChaveNaoProvada` e o doc do K1-m1.
- `crates/seele-conformance/tests/bateria_interna.rs`: o teste do impostor lê o tipo.

Nada fora da lista do lote foi commitado. `ui/base.js` foi mudado e restaurado só para a medida do K1-m1.
`crates/seele-core/src/par.rs` foi só lido.

## O que não rodou aqui, e por quê

- **A bateria inteira e `cargo test --workspace`.** É regra do lote: rodam na integração, numa fila só.
- **`quarto_pelo_link`, o binário da porta 8384.** Não é deste lote.
- **As bancadas de navegador** (`diagnostico-de-mods.cjs` e as do job `bancadas`). O lote não mexe no que elas medem.
  A frase do K1-m1 sobre a `diagnostico-de-mods.cjs` é a de antes, e não foi medida de novo.
- **Código de plataforma.** Nenhum foi escrito: tudo o que o lote muda compila e roda neste Mac.
- **`drive` com uma sessão de verdade.** Nada abre uma sessão real que a volta da bateria recuse e confere o `Ended`
  na casca. A fiação de `drive` até `fim_da_sessao` é uma linha, e nenhum teste a prende. O teste da FFI prende
  `fim_da_sessao` e `fim_da_recusa`, e o `bateria_interna` prende o núcleo até o `Aviso::Encerrado(Recusado(…))`.

## Preocupações

1. **«A sessão acabou sem mandar nada a ele»** (a frase de `ServidorTrocouNaVolta`, texto exato do brief) usa
   «nada».
   - O guarda irmão, `a_recusa_pela_impressao_diz_que_o_convite_a_senha_e_o_apelido_nao_chegaram_a_ele`, registra no
     doc que «nada saiu» seria falso, porque o `ClientHello` do TLS sai antes de haver o que conferir. Por isso ele
     proíbe «nada» e «nenhum» no ramo `InviteMismatch` de `fraseDeErro`.
   - Esse guarda não lê `MOTIVOS`, e o teste passa. A tensão de texto existe: na volta, o que não saiu é o `Hello`
     (a identidade, o apelido, a credencial), e o `ClientHello` saiu.
   - Usei a frase do brief, como pedido. Se quiserem alinhar com a regra da irmã: «A sessão acabou antes de mandar o
     apelido e a credencial a ele.»
2. **`ServidorTrocouNaVolta` fala de «convite» e de «link» também quando a volta é pela lista.**
   - `MOTIVOS` não recebe `deOnde`, ao contrário de `fraseDeErro`, que separa a impressão do link da que a lista
     guardou.
   - A mesma frase cobre o `ChaveNaoProvada` da volta. Ali quem atendeu mostrou o certificado certo, e «não é o
     servidor do convite» é verdade, mas o remédio «Confirme o link» serve menos.
   - É o texto do brief. Fica registrado para a «recusa legível» do Plano 3.
3. **`fim_da_sessao` é uma função nova, fora de `drive`.** O brief diz que o braço «dentro do `Aviso::Encerrado` de
   `drive` passa a chamar `fim_da_recusa`». Ele chama, um nível abaixo, para que a reversão pedida pelo brief tivesse
   um teste que a visse. O comportamento de `drive` é o mesmo, e o mapeamento dos outros três braços não mudou.
4. **O número do alerta é do `rustls`.** Medido no `rustls` 0.23.45: `BadSignature` → `DecryptError` (51). O
   `rustls` também manda o 51 para `UnsupportedSignatureAlgorithm*` e para um `Finished` que não confere. Se uma
   versão nova mudar o mapeamento, o teste do impostor reprova alto, que é o que o brief pede.
5. **A corrida registra `ChaveNaoProvada` em `info!`.** É o `registrar_a_falha_do_candidato`, que guarda o `warn!`
   para `InviteMismatch` e `PinChanged`. O `seele.log` tem a linha `WARN` do próprio `classify_connection_error`
   (nova neste lote), então o impostor perdedor não fica calado. Mas a linha da corrida, com `onde` e `entrou`, sai
   em `INFO`. Fora do brief, e não mexi.
6. **Não há guarda de todas as variantes de `ConnectionError` contra as frases.** O guarda novo é desta variante.
   Cinco variantes têm a frase em `tela-server.js`, e um guarda geral teria de ler os dois arquivos.

## Para a v6

- `EndReason` ganhou `ServidorTrocouNaVolta` e `ChaveMudouNaVolta`, que chegam ao retrato (`ended`) que os MODs leem
  hoje. São valores aditivos, e nenhum `api/*.json` enumera `EndReason` (`cargo xtask check-api` verde, `api/`
  intocado). Se a API 6 projetar o `ended` do retrato, é a F3 que decide se os dois valores entram na promessa
  escrita.
- Nada mais do lote toca a superfície de MOD.
