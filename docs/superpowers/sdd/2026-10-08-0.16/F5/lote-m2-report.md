# Relatório do lote M2 da F5 (0.16) · o ambiente com trava, o `levou=`, os textos do 1B e a montagem de certificado

Itens: P51-16, P51-18, P51-20 (pendência 51) e a montagem de certificado do 1B,
que veio do M1. Brief: `.superpowers/sdd/lotes/lote-m2-brief.md`. Branch
`0.16/lotes-m2`, a partir de `0f94be6`. Worktree:
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a9bb67dbd722462e4`.

**Status: DONE.** Os quatro itens fecharam numa sessão. Nada toca o fio, a F6,
a API de MODs, `docs/pendencias.md` nem o índice. No plano 1B, só a linha :99.
Em `docs/ponto-de-encontro.md`, a abertura dos 60 s perdeu só a oração do
quarto.

## O que foi feito, item por item

| item | commit | o que mudou |
|---|---|---|
| P51-16 | `8669d99` | `#[cfg(test)] pub(crate) mod ambiente_de_teste` no `lib.rs` do `seele-server`: `static TRAVA: Mutex<()>` e `AmbienteDoEncontro { antes, _trava: MutexGuard<'static, ()> }`, com `guardar`, `pedir`, `tirar` e um `Drop` que devolve a variável antes de a trava cair. A trava é tomada com `unwrap_or_else(PoisonError::into_inner)`. Os dois testes que trocam `$SEELE_ENCONTRO` passam a usá-lo: `um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada` (sem o `restaurar`, com `drop(ambiente)` logo depois das duas subidas) e `desligar_o_degrau_4_e_uma_variavel_de_ambiente` (o comentário «SAFETY-de-teste… são serializados» virou o que a trava cobre e o que não cobre). Dois testes novos: `o_ambiente_do_encontro_volta_mesmo_quando_o_teste_entra_em_panico` (`catch_unwind`) e `o_ambiente_do_encontro_segura_a_trava_enquanto_vive`. |
| P51-18 | `aa106bc` | `let comecou = tokio::time::Instant::now();` em `onde_mora_hoje` (e `ate = comecou + prazo`), e a linha final ganhou `levou = ?comecou.elapsed()`. O doc foi reescrito com o que o M1 deixou: os endereços do ponto, IPv4 primeiro, e o seguinte só quando nada saiu (com a linha `info`); a primeira volta recusada inteira passa ao endereço seguinte, ou volta como `PontoMudo` sem seguinte; a volta recusada depois da primeira conta como volta, e a ida e volta do `ONDE` se mede da última que saiu; a leitura que falha acaba a consulta com o que chegou, «como `PontoMudo` se nada respondeu antes; se o ponto já respondeu, como `NinguemMora`, ou `Achado` com o que chegou», sem passar ao endereço seguinte. A seção do log diz o `levou=` e a linha do endereço deixado. |
| P51-20 · C-M3 | `70a0f12` | Doc de `ConnectionError::InviteMismatch` (`types.rs`): «A continuidade do servidor não está em questão: o que não confere é a promessa, do link ou da lista, com quem atendeu. Ou a promessa está errada, ou ela está certa e quem atendeu é outro servidor. Num alvo de LAN, a promessa pode estar certa e quem atendeu ser outro servidor no mesmo endereço…». Relido contra o doc que o H deu a `EndReason::ServidorTrocouNaVolta` («quem atendeu… não é o servidor desta sessão»): os dois dizem a mesma coisa. |
| P51-20 · C-M4 | `70a0f12` | Doc de `Marca` (`seele-proto/src/encontro.rs`): «que guardou a impressão que a conexão aceitou». |
| P51-20 · C-M5 | `70a0f12` | Plano 1B, :99: o teste de `convite.rs` prova o aviso em `127.0.0.1`, mas o fixture `destino` passa `o_pino_prova_o_servidor: true` à mão; quem prova o loopback como alvo público são `um_alvo_publico_ou_de_loopback_tem_o_pino_que_prova_o_servidor` e `num_alvo_publico_ou_de_loopback_o_pino_que_confere_passa_e_so_avisa` (`seele-ffi/src/lib.rs`). |
| P51-20 · C-R1-m1 | `70a0f12` | «até o quarto encher e chegar uma marca nova (`Quarto::morar`)» nos cinco lugares: `seele-encontro/src/lib.rs`, `docs/alcance-pela-internet.md` (a tabela, no presente: «quando o quarto enche e chega uma marca nova», e a lista), e `docs/ponto-de-encontro.md` (a abertura e a seção do quarto). Na abertura, só a oração do quarto mudou; «Esse endereço vale 60 segundos desde o último aviso do anfitrião» ficou intacta. |
| P51-20 · T4 | `70a0f12` | Doc de `esperar_o_servidor_fechar`: «volta em microssegundos (medido: 4 µs)». A frase dos 20,0 s ficou, como o brief manda. |
| P51-20 · o pino | `70a0f12` | Doc de `desfazer_pin_orfao`: «**O que ela não distingue:**» o segundo resíduo, com `A` (escreveu o pin, prazo estourado) e `B` (mesma chave, falhou antes sem escrever): a limpeza de `B` vem primeiro e a linha do pin órfão leva a falha do vizinho; o pin que cai é o certo, e a linha da corrida (`registrar_a_falha_do_candidato`) tem a falha de cada um. E «**Os dois resíduos falham para o lado seguro**»: um pin a menos ou uma linha que atribui mal, nunca um pin que nenhuma conexão confirmou ficando na loja. |
| montagem (do M1) | `092dbfc` | `fn escuta_de_teste(rustls::ServerConfig) -> (quinn::Endpoint, SocketAddr)` no `mod tests` de `enlace.rs`: ALPN, `quinn::ServerConfig::with_crypto` e `Endpoint::server` em `127.0.0.1:0`. `ponta_com_certificado_proprio` monta o TLS honesto (`with_single_cert`, que confere a chave) e chama a montagem; `servidor_que_aperta_a_mao_e_cai` pede `ponta_com_certificado_proprio()` e só mantém o que faz depois do TLS; `impostor_que_repete_o_certificado` monta o TLS que assina com outra chave e chama a montagem. Nenhum `Hello` nem prova do cliente foi montado à mão. |

## Evidência de TDD: RED e GREEN

- **P51-16.** O módulo foi escrito primeiro com os dois testes, o struct sem o
  campo da trava (`guardar_sob` não a tomava) e o `Drop` vazio.
  `cargo test -p seele-server --lib ambiente_de_teste`:
  - `o_ambiente_do_encontro_volta_mesmo_quando_o_teste_entra_em_panico`: «um
    teste que entrou em pânico com `$SEELE_ENCONTRO` trocada deixou a troca
    para trás: todo teste seguinte deste binário fala com o ponto de encontro
    errado», `left: Some("valor-que-o-panico-nao-pode-deixar-para-tras")
    right: None`;
  - `o_ambiente_do_encontro_segura_a_trava_enquanto_vive`: «a trava estava
    livre com um `AmbienteDoEncontro` vivo: dois testes trocam
    `$SEELE_ENCONTRO` ao mesmo tempo, e um lê o valor que o outro pôs».

  GREEN com o campo e o `Drop`: `seele-server --lib`, 562 passaram.
- **P51-18.** A asserção nova em
  `um_nome_que_nao_resolve_no_prazo_diz_no_rastro_que_foi_o_prazo`, antes do
  conserto: «a linha do resultado da consulta ao quarto não diz quanto ela
  levou (`levou=`)…», com o rastro `INFO: quarto: onde o anfitrião mora hoje
  ponto=nao-existe-mesmo.invalid:8384 resposta=PontoNaoResolve`. GREEN:
  `seele-core --lib encontro`, 29 passaram.
- **P51-20** é texto. A prova é o bloco de greps (abaixo), o `cargo doc` e os
  testes do que os textos descrevem.
- **A montagem** é uma troca de forma: o vermelho dela é a reversão abaixo. Os
  63 testes de `cargo test -p seele-core enlace` são os mesmos antes e depois
  (a lista `--list` foi comparada, idêntica), e todos verdes.

## Cada reversão, e o vermelho que ela deu

Cada uma foi desfeita no arquivo, rodada, e o conserto voltou; o arquivo foi
conferido byte a byte (`cmp`) contra a cópia verde.

- **O `Drop` vazio** (`fn drop(&mut self) {}`), com o binário inteiro de
  `seele-server --lib`: `o_ambiente_do_encontro_volta_mesmo_quando_o_teste_entra_em_panico`
  vermelho, `left: Some("valor-que-o-panico-nao-pode-deixar-para-tras") right:
  None`; 561 passaram, 1 falhou.
- **A trava solta**, o desenho que o brief proíbe (`let _t = trava.lock()…`
  dentro de `guardar_sob`, e o campo virando `()`): `o_ambiente_do_encontro_segura_a_trava_enquanto_vive`
  vermelho, «a trava estava livre com um `AmbienteDoEncontro` vivo…».
  - E a corrida, com essa reversão: os dois testes que trocam a variável,
    juntos (`--test-threads=8 desligar_o_degrau fora_do_ar`), reprovaram em
    **13 de 20** rodadas. Nas 13, `um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada`
    reprovou em `hospedagem.rs:577`: «o ponto de encontro fora do ar mudou o
    degrau da escada», `left: Some(SoRedeLocal) right: Some(FuroDeNat)`. A
    subida «sem ponto de encontro» leu o valor do outro teste e chegou a um
    ponto de encontro de verdade. Numa delas,
    `desligar_o_degrau_4_e_uma_variavel_de_ambiente` também reprovou
    (`alcance/encontro.rs:1416`).
  - Com a trava: os mesmos dois juntos, **20 de 20** verdes.
- **Sem o `levou=`** na linha final: o mesmo vermelho do RED do P51-18, com o
  binário de `encontro` inteiro (28 passaram, 1 falhou).
- **A montagem e o impostor:** o impostor assinando com a chave verdadeira
  (`verdadeiro.signing_key` no lugar de `outra`), para provar que os testes do
  H continuam medindo através da montagem nova:
  - `o_pin_que_um_impostor_fez_fixar_e_desfeito_e_o_log_nao_diz_que_o_tls_terminou`:
    «o impostor que mostrou o certificado sem a chave não saiu como
    `ChaveNaoProvada`…», `left: Some(Unreachable) right:
    Some(ChaveNaoProvada)`;
  - `o_candidato_perdedor_que_nao_prova_a_chave_vai_ao_log_como_aviso`: «o
    candidato perdedor que mostrou o certificado sem provar a chave foi ao log
    como falha comum, e não como aviso…», com a linha `INFO: este endereço do
    convite não deu … erro=Unreachable …`.

## A corrida

`cargo test -p seele-server --lib -- --test-threads=8`, com a trava:
- 15 rodadas seguidas do binário de teste (`--test-threads=8`), 562 verdes em
  todas;
- 10 rodadas seguidas pelo `cargo`, no HEAD final, 562 verdes em todas.

## O bloco de greps da pendência 51

No HEAD final:
- P51-16, `let restaurar = |valor: Option<String>|`: nada.
- P51-18, a linha `tracing::info!(%ponto, ?resposta, "quarto: onde o anfitrião mora hoje")`
  e `leitura falha\*\*: volta na hora`: nada.
- P51-20:
  - «O que falhou é a promessa», «guardou a impressão de um link», «o loopback
    conta como público» e «volta em dezenas de»: nada.
  - `git grep "quarto encher\|quarto enche" -- docs crates` **ainda acha**
    os cinco lugares, porque a frase do conserto («até o quarto encher e
    chegar uma marca nova») contém «quarto encher». Também acha
    `docs/pendencias.md` e os planos de lote. As frases antigas sumiram:
    `git grep -n "quarto encher (a varredura)\|quando o quarto enche,\|até o quarto encher,\|quarto encher (aí" -- docs crates`
    só acha `docs/superpowers/sdd/2026-10-08-0.16/F5/lotes.md:938`, que cita o
    texto velho.
  - `O que ela não distingue` acha **duas** linhas de `enlace.rs`, de
    propósito: o primeiro resíduo, que fica no doc de
    `desfazer_o_pin_deste_aperto`, e o segundo, com o mesmo cabeçalho, no doc
    de `desfazer_pin_orfao`.

## Arquivos mudados

- `crates/seele-server/src/lib.rs`: o módulo `ambiente_de_teste` e os dois
  testes dele.
- `crates/seele-server/src/hospedagem.rs` e
  `crates/seele-server/src/alcance/encontro.rs`: os dois testes que trocam a
  variável.
- `crates/seele-core/src/encontro.rs`: o `levou=`, o doc de `onde_mora_hoje`
  e a asserção nova.
- `crates/seele-core/src/enlace.rs`: o doc de `desfazer_pin_orfao` e a
  montagem do `mod tests`.
- Só doc: `crates/seele-ffi/src/types.rs`, `crates/seele-proto/src/encontro.rs`,
  `crates/seele-encontro/src/lib.rs`,
  `crates/seele-conformance/tests/volta_pela_trilha.rs`,
  `docs/alcance-pela-internet.md`, `docs/ponto-de-encontro.md` e o plano 1B
  (:99).

## Testes e verificações que rodaram

No HEAD final, `perl -e 'alarm 600; exec @ARGV' cargo test …`:
- `-p seele-server --lib`: 562 passaram;
- `-p seele-core --lib encontro`: 29 passaram;
- `-p seele-core enlace`: 63 passaram;
- `-p seele-conformance --test volta_pela_trilha`: 6 passaram;
- `-p xtask --test textos_publicos`: 2 passaram. Ele lê
  `docs/alcance-pela-internet.md` e `docs/ponto-de-encontro.md`.

E também:
- `cargo fmt --all -- --check`: limpo antes de cada commit;
- `cargo clippy -p seele-server -p seele-core -p seele-ffi -p seele-proto -p seele-encontro -p seele-conformance --all-targets --all-features -- -D warnings`: limpo;
- `cargo doc -p seele-proto --no-deps`: os 4 avisos que saem são de
  `control.rs` e `vocabulario.rs`, de antes deste lote; nenhum de
  `encontro.rs`;
- `cargo doc --no-deps` de `seele-core`, `seele-ffi` e `seele-encontro`:
  nenhum aviso nos trechos mexidos.

## O que não rodou

- `quarto_pelo_link` (a porta 8384), como o brief manda. O `AmbienteDoEncontro`
  dele não foi tocado.
- A bateria inteira e o `cargo test --workspace`: são da integração.
- `apps/seele-app` não foi compilado: nada dele mudou.
- Nenhum código com `cfg(windows)` ou `cfg(target_os = "linux")` foi mexido.

## Preocupações

1. **Dois greps da pendência não podem «deixar de achar».** O do C-R1-m1
   (`quarto encher\|quarto enche`) casa com a própria frase do conserto, e o
   do pino (`O que ela não distingue`) aponta o primeiro resíduo, que fica.
   Quem registrar o fecho (o lote Z) precisa de outro comando para o C-R1-m1;
   o das frases antigas está acima.
2. **Uma sexta ocorrência do C-M4, fora da lista do brief:**
   `crates/seele-encontro/tests/apresentacao.rs:106`, num comentário de
   teste, diz que a lista «guardou a impressão de um link». Não foi mexida.
3. **Além da letra do brief, no P51-16:**
   - `tirar()`, ao lado de `guardar` e `pedir`: o teste de
     `alcance/encontro.rs` tira a variável no meio, para medir o padrão, e
     sem ele chamaria `remove_var` por fora do guarda;
   - `guardar_sob(&'static Mutex<()>)`, privado ao módulo. É por ele que os
     dois testes do módulo provam o guarda com uma trava só deles, enquanto o
     de pânico segura a `TRAVA` de fora do começo ao fim. Sem isso, outro
     teste que tirasse a variável entre o pânico e a conferência deixava o
     vermelho da reversão depender da ordem;
   - o teste `o_ambiente_do_encontro_segura_a_trava_enquanto_vive`, que prova
     a trava como campo.
4. **O que a trava não cobre (risco registrado, como o brief manda):** os
   outros 9 `Hospedagem::iniciar` de `hospedagem.rs` leem a variável sem a
   trava. A corrida sem a trava mostrou o que isso custa: um teste que leu o
   valor de outro chegou a um ponto de encontro de verdade (`FuroDeNat`). Um
   deles que suba durante a janela do `BURACO_NEGRO` pode pagar o prazo.
5. **O commit do P51-18 foi refeito** depois da primeira passada: o doc novo
   ainda casava com o grep `leitura falha\*\*: volta na hora`. A história foi
   refeita sem `-i` (de volta ao commit, `--amend`, e `cherry-pick` dos dois
   seguintes), e a única diferença para a primeira versão é esse item do doc.
   Os hashes deste relatório são os finais.
6. **A montagem não cobre tudo o que se repete:** `generate_simple_self_signed`
   continua em dois lugares (a ponta honesta e o «verdadeiro» do impostor), e o
   laço que aceita todo aperto de mão continua em
   `ponta_que_so_mostra_a_chave` e no impostor. O brief pedia só o trecho até
   o `Endpoint`.

## Para a v6

Nada. Nenhum lugar deste lote cria evento, campo ou frase de API.

## Achado fora do lote

Não foi mexido: fica com a orquestradora, para virar pendência.

- **O que é:** os testes do `seele-server` que chamam `Hospedagem::iniciar`
  sem pôr `$SEELE_ENCONTRO`, e o `ejetar.rs` (`seele-conformance`, :321 e
  :326), falam com o ponto de encontro de produção. Sem a variável,
  `Convocacao::do_ambiente` usa o `PONTO_PADRAO`, `encontro.seele.app.br`.
  O `ejetar.rs` foi apontado pela revisão; ali só conferi que ele chama
  `iniciar` sem pôr a variável.
- **O que eu medi:** na reversão da trava solta, os dois testes que trocam a
  variável rodaram juntos e reprovaram em 13 de 20 rodadas. Nas 13,
  `um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada` deu `left:
  Some(SoRedeLocal) right: Some(FuroDeNat)`. A subida que devia rodar com
  `nao` leu outro valor e chegou ao degrau 4, o `FuroDeNat`. Só um ponto de
  encontro que responde dá esse degrau. Os valores que o outro teste põe não
  respondem: `""` desliga, e `meu.ponto:9000` não resolve. O que sobra é o
  padrão, lido depois de o teste de `alcance` tirar a variável. É uma
  inferência, sem captura de pacotes: um teste de unidade alcançou o ponto de
  encontro de produção a partir desta máquina.
- **O que a trava não resolve:** ela serializa só quem troca a variável. Quem
  só a lê sem tê-la posta, e são os outros 9 `iniciar` de `hospedagem.rs` e o
  `ejetar.rs`, continua indo ao ponto de produção.

## Rodada 1 de conserto

A revisão deu «precisa de conserto» por um Importante. Ela confirmou contra o
código os textos do 1B, o `levou=`, o doc de `onde_mora_hoje` (com o T6f) e a
montagem do impostor.

| achado | commit | o que mudou |
|---|---|---|
| Importante 1 | `9a46b77` | `o_ambiente_do_encontro_segura_a_trava_enquanto_vive` começa com `let _ninguem_mais_troca = TRAVA.lock().unwrap_or_else(PoisonError::into_inner);`, como o de pânico, e o `try_lock` continua em `SO_DESTE_TESTE`. O comentário diz por quê: o guarda lê a variável ao nascer e a escreve ao cair. |
| Menor 1 | `9de50a7` | O doc de `consultar_por` diz «na hora, quando nenhuma pergunta da **primeira** volta saiu ou a leitura falhou». |
| Menor 2 | `9de50a7` | No C-M3, as duas frases viraram uma: «Ou a promessa está errada, ou ela está certa e quem atendeu é outro servidor: num alvo de LAN, outro servidor no mesmo endereço, porque o endereço da rede de casa se repete de uma casa para outra.» Não se perdeu nada. A frase «O que falhou é a promessa» continua fora. |

**O defeito do Importante 1, medido antes do conserto.** Rodei o binário de
teste só com os dois testes do módulo (`--test-threads=2 ambiente_de_teste`),
em laço:
- deu 1 falha em 2000 rodadas, e depois 3 em 6000;
- em todas, foi o teste de pânico que reprovou, com a frase falsa: «…deixou a
  troca para trás…», `left:
  Some("valor-que-o-panico-nao-pode-deixar-para-tras") right: None`;
- foi o terceiro escritor, e não o `Drop`: o guarda do teste da trava leu o
  valor do pânico e o escreveu de volta depois de o outro teste devolver a
  variável.

Depois do conserto, o mesmo laço deu **0 falhas em 6000**.

**A reversão da trava solta continua vermelha.** Com o campo virando `()` e o
`let _t = trava.lock()…` solto em `guardar_sob`, `cargo test -p seele-server --lib`
deu 561 verdes e 1 vermelho: `o_ambiente_do_encontro_segura_a_trava_enquanto_vive`,
com «a trava estava livre com um `AmbienteDoEncontro` vivo…». O arquivo
voltou idêntico, conferido com `cmp`.

**A corrida:** `cargo test -p seele-server --lib -- --test-threads=8`, 20
rodadas seguidas pelo `cargo`. Passaram as 20, com 562 verdes em cada.

**O que rodou, no HEAD desta rodada:**
- `cargo test -p seele-server --lib`: 562;
- `cargo test -p seele-core --lib encontro`: 29;
- `cargo fmt --all -- --check`: limpo;
- `cargo clippy -p seele-server -p seele-core -p seele-ffi --all-targets --all-features -- -D warnings`:
  limpo;
- `cargo doc --no-deps` de `seele-core` e `seele-ffi`: nenhum aviso em
  `encontro.rs` nem no doc de `InviteMismatch`.
