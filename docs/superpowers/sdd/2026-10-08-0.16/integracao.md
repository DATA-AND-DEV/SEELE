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
| as migrações do banco | F1 (14, `api` em `mods`, `4112f96b`) e F3 (15, `temas_dos_mods`, `bfd5f5d`) | a próxima é a 16. Conferir com `git log --all -S "version: 1"` em `persistence/schema.rs` antes de uma migração nova |
| `NOME_DA_SALA` | F1 (`seele_proto::transport`, `7e123ee5`) e F7 (`pub const` própria em `seele-server/src/tls.rs:27`) | quando os dois estiverem em `versao/0.16`, a F7 troca a sua por `pub use seele_proto::transport::NOME_DA_SALA;`. Conferir que não sobram duas cópias |
| `0.16/protocolo` (F1) | a P8 parada no meio (09/10): um filtro de segurança interrompeu a edição do aperto de mão do cliente na sessão da F1, e o dono decide na sessão dela. O WIP isolado é `87c2eb58`, com os dois casos de outra linha do `outra_linha.rs` em `#[ignore]` | ~~integrar só até `24e7e249`~~: desde 09/10 o WIP mora no `0.16/protocolo-p8` (`87c2eb58`), e o `0.16/protocolo` voltou a ser integrável inteiro. **O `-p8` não entra** enquanto a P8 não fechar. Ninguém retoma o trecho por outra sessão. A tarefa 16 da F7 espera |
| `apps/seele-app/src/executor.rs` | F4 (a extração do laço para um crate comum, aberta em 09/10), F3 (tarefas 3, passo 5, 4 e 8, que esperam a extração) e F5 (os Menores adiados do lote D no executor, se o Z os consertar) | ninguém além da F4 toca no arquivo até a extração ser integrada. Depois, a F3 e um eventual conserto do D entram em fila |
| `apps/seele-app/ui/` (a casca do celular) | F8 (`0.16/celular`, os arquivos novos `celular-*` e os pontos de extensão), F5 (ondas 5 e 6) e F4 (`ponte-web.js`, `mods-no-navegador.js`, `voz-web.js`, `identidade-web.js`, `tela-boot.js` da 3.4) | a F8 só mexe nos arquivos existentes depois da liberação da `ui/`. Na integração: os guardas de texto leem os `celular-*`, e o desktop não muda de aparência |
| `docs/superpowers/sdd/2026-10-08-0.16/F2/progress.md` | F2 (`a2181ad6`) | ~~trocar «deste branch» por «da `versao/0.16` depois da integração verde»~~: feito em `f5956869` |

## Guardas a acrescentar na integração

- **O esquema da 0.16 é maior que o da 0.15.** A primeira abertura (F6, tarefa 12)
  só reconhece uma pasta já migrada porque a 0.16 grava um esquema acima de 13
  (`versoes::ESQUEMA_DA_0_15`). A migração 14 da F1 (`4112f96b`) garante isso hoje.
  O guarda afirma que a maior versão de `MIGRATIONS` do `seele-server` é maior que
  `ESQUEMA_DA_0_15`. Ele só pode entrar depois de F1 e F6 estarem juntas.

## Antes de publicar

- ~~**Rodar a bateria inteira no `linha/0.15`** (a 0.15.2)~~: **feita em 08/10, verde**,
  em `fe1cd866` (com os dois consertos de segurança, `ba99007` e `40318ea`, a identidade
  `9ff9b73` e a recusa de notas com condição `7e444fe8`): fmt, check-runtime, check-api,
  check-versao, check-vetores, clippy, `cargo test --workspace -- --test-threads=1`
  (2725 passaram, 0 falharam, 5 ignorados, com `SEELE_EXIGE_CODEC=1`), check-deps e deny.
  **Se o `linha/0.15` mudar depois disto, ela roda de novo.**
  Rodou de novo em 09/10, **verde**, em `148c9e60`, com o terceiro conserto de segurança
  (a sala cheia, `250cde9c`): 2729 passaram, 0 falharam, 5 ignorados. Depois dela, só
  as notas mudaram (`60788523`, a D-linha-2: a 0.15.2 sai agora como atualização normal).
  Depois disso entraram dois consertos de leitura (D-linha-3): o texto da sessão que acompanha um
  tema claro (F3, `b46032e`) e o nome na escolha entre transmissões, que dizia «VER —» (F8,
  `6035d847`). **A bateria rodou de novo em 09/10, em `bb1d5e78`:** 2728 passaram, 1 falhou,
  5 ignorados. O que falhou foi o `um_manifesto_anterior_que_nao_existe_para_o_release`: o mesmo
  defeito do `publicar.sh` que a sétima integração achou na 0.16 (`«$ma_extra»` no bash 3.2 do
  Mac, num locale UTF-8). As baterias anteriores da linha rodaram em locale `C` e não o viam. **A
  avaliação da sétima integração estava errada**: o `publicar.sh` roda a própria bateria antes de
  publicar, e no terminal de quem publica esse teste travaria a 0.15.2. Consertado em `40066a7c`,
  com o guarda de texto, e conferido pelo `xtask` inteiro em `C.UTF-8` (153 de 153), fmt e clippy;
  pôr o defeito de volta reprova os dois testes. Entre `bb1d5e78` e `40066a7c` só mudaram essa
  frase e o guarda. **A `linha/0.15` pronta para publicar é `40066a7c`.**
- **As medidas de campo do protocolo nas duas direções**, com o dono: o app
  0.15.1 publicado contra o `seeled` novo, e o app novo contra o `seeled` 0.15.1
  (plano F1b, P1 e P8).
- **A parte 2 da tarefa 17 da F6**, com tela, no Mac e no Windows, com o dono.
- **O ponto em produção fala o ENC/2**: `sondar --exige-enc2 encontro.seele.app.br`, de
  outra máquina, depois de o dono reimplantar a VPS (`docs/ponto-de-encontro.md`).
  A VPS vai antes do app (spec-mestre, §4).
- **O indexador, antes dos pacotes da API 6** (conferido pelo ESTILO em `SEELE-MODS-INDEXER/ferramentas/manifesto.py`); do dono, no `SEELE-MODS-INDEXER`:
  1. a chave `tema` em `CHAVES` e em `TIPOS`, como booleana (`:49-50`; hoje uma chave desconhecida sai `malformed`, `:144-146`), e levada ao catálogo para o site dizer «muda a aparência do servidor»;
  2. `VERSAO_DA_API` de 5 para 6 (`:20`; hoje a 4.0.0 sai `api-too-new`, `:186-187`);
  3. `APIS_ACEITAS` como a **união** das linhas que o launcher abre, `(6, 5, 4, 3)`, e não o espelho do app (`:22-43`; com `(6,)`, todo conserto legado futuro dos três MODs sai `api-too-old`, `:191-193`), e a amarração `catalogo.py:106-110` (`api_oferecida` contra `MOD_API_VERSION`) comparando a união.
- **Os 28 roteiros da casca passam** (`tools/carga-da-casca.py --roteiro tools/roteiros/*.js`). Achado da F8, medido em 09/10: na `versao/0.16` (`f7bc902b`), 17 dos 28 reprovavam no mesmo ponto, a sessão que não aparece depois de HOSPEDAR AQUI. A causa, achada pela F8 com prova por reversão: a `acbc1c29` (08/10) faz uma máquina sem nome perguntar «COMO OS OUTROS VÃO TE CHAMAR?» antes do `connect`, e o duble respondia `""` ao `apelido_local`. O `linhas_para_hospedar` sem resposta (F6, `a2160d75`) só estourava num `console.warn`. Na `linha/0.15` (`148c9e60`), os 28 passam. Nada rodava os roteiros, e por isso a regressão passou calada. O conserto é o primeiro commit da F8 (`5d2e6681`, integrado em `ba1ce8e4`), e os roteiros viram a 12ª etapa da bateria da integração: desde a sétima, os 28 passam.
- **As notas não têm condição pendente**: o `publicar.sh` recusa notas com o marcador
  `<!-- CONDIÇÃO:` (F6, `36274663` e `7e444fe8`).

## As integrações feitas

| quando | o que entrou | commit | bateria |
|---|---|---|---|
| 08/10/2026 | F5 onda 1 (C, H, K2, M1, ferramentas, P54), F1 até `f9fdac74` (protocolo 9, S2, vocabulário, API guardada), F6 até `4e66dbf3` (o launcher, tarefas 1 a 16), F4 até `f937ff1` (etapa 0), e o guarda do esquema (`2a37ffc5`) | `2a37ffc5` | **verde**: fmt, check-runtime, check-api, check-versao, check-vetores, clippy, `cargo test --workspace -- --test-threads=1` (2912 passaram, 0 falharam, 5 ignorados, com `SEELE_EXIGE_CODEC=1`), check-deps, deny licenses e advisories |
| 08/10/2026 | F5 onda 2 (M2, P53, K5, D), F1 até `ac1fae91` (`ModAviso`, `TemaMudou`, P9, P10), F4 até `737a627` (o `seele-web`, o motor em `wasm`, a exceção de `unsafe`, o `cargo xtask web`), F3 até `493aa594` (o rascunho da API 6, o esqueleto do ADR 0056, a M3 de base) e F7 até `85879154` (a janela do Comandante na 0.16, o certificado da sala, a campainha, a agenda, o toque, os ADRs 0059 a 0061) | `4bc764d3`, consertado em `5122f39d` | **verde depois de um conserto**: a primeira rodada reprovou o clippy (um `iter().any()` no `fronteira_do_celular.rs`) e um teste (`todo_crate_do_produto_pode_falar_no_log` cobrava os crates do celular no filtro de log do desktop); o resto verde, com 3005 testes passando. Depois do `5122f39d`: o clippy do workspace inteiro, o `empacotamento` (22/22) e o `xtask` (168/0) rodados de novo, e o `cargo xtask web` à parte (os sete passos) |
| 08/10/2026 | F5 onda 3 (E, G-servidor, J; o conserto do guarda do `frontend` em `9daa0c0`, revisado na integração), F1 até `33bca7a9` (`NOME_DA_SALA`, `verify_strict` e a chave de ordem pequena recusada), F6 até `36274663` (`SEELE_IDENTIDADE`, notas com condição recusadas), F4 até `ec78faa` (o `xtask web` na bateria, os links no `.wasm`), F3 até `c1fda14` (a M3 de base) e F7 até `ed1057f1` (o UPnP por papel, o `ConfereCodigo`, o ADR 0061 aceito) | `0e30ea36` | **verde**: a bateria inteira, agora com o `cargo xtask web` como 11ª etapa (3059 passaram, 0 falharam, 7 ignorados). Sem conflito de texto |
| 08/10/2026 | F1 até `d794a27c` (o servidor do protocolo completo: P1 a P5 e o `MembroNovo`; a P7c fica de fora até o corte da API 6), F2 até `a2181ad6` (o ENC/2 inteiro, com o cookie e o teto por prefixo, e o portão da VPS no `publicar.sh`), F7 até `41fa1b5d` (a campainha viva, o aperto em três fases, a `Esperada`, ligar pelo código), F6 e F3 (só documentos), e a linha da VPS corrigida no andamento da F2 | `f5956869` | **verde**: a bateria inteira com o `xtask web` (3196 passaram, 0 falharam, 7 ignorados). A `moderacao`, que a F1 viu reprovar uma vez em 21, passou. Sem conflito de texto. A onda 4 da F5 não entrou: estava no meio |
| 09/10/2026 | F5 onda 4 (F — a saída com despedida, a pendência 50: quem sai some dos presentes em menos de 1 ms no laço local, e não nos ~20 s do `IDLE_TIMEOUT`; e K3), F1 P9 (`14f2b2db`, o `v=` como versão do servidor) e o merge da quarta no `0.16/sala` | `ee45e13a` | **verde**: a bateria inteira com o `xtask web` (3207 passaram, 0 falharam, 7 ignorados). Sem conflito de texto; a F5 consertou antes, no próprio branch, o `saida_com_despedida.rs` que a troca do `impressao_esperada` pela `esperada` (F7) tinha quebrado (`7f885a5`) |
| 09/10/2026 | F1 até `85f441ba` (a sala cheia que não se escuta, o `PedidoRecusado`, a frase provisória e o guarda dos avisos), F3 até `ea327114` (o tema no servidor, a migração 15), F7 até `0cad3594` (a hospedagem da sala, o `ler_entrada`, discar pelo código, a reconexão da ligação), F6 (`6f04154c`) e os documentos do lote I da F5 | `3f8babe0` | **verde depois de um conserto**: a primeira rodada deu 3281 passaram e 1 falhou — o `seele-web/tests/pelo_fio_1_0.rs` contava as linhas dos bytes-ouro (49) contra as variantes (48), porque a F1 acrescentou um exemplar a mais do `Alert`; consertado contando variantes. Um conflito no `frontend.rs`, onde a F1 e a F3 acrescentaram guardas no fim: ficaram os dois |
| 09/10/2026 | F5 onda 5, só o lote I-a (`77644c4b`: o áudio só na sala de voz, a D-52; a recusa que não tira ninguém da sala; o pânico no `seele.log`), F4 até `c71fef7a` (a volta do executor extraída para o crate `seele-motor`, comum ao desktop e ao celular, com a tabela de casos nos dois alvos), F3 até `f75a1b34`, F1 e F7 (só andamento) | `05a5aca2`, consertado em `27e8e923`; o duble da F8 em `ba1ce8e4` | **verde depois de um conserto**: 3332 passaram, 1 falhou, 7 ignorados. O que falhou foi o `um_manifesto_anterior_que_nao_existe_para_o_release`: no `/bin/bash` 3.2 do Mac, num locale UTF-8, `«$ma_extra»` vira a variável `ma_extra\xc2`, e o `set -u` mata o `publicar.sh` antes da frase. A sexta integração rodou em locale `C`, onde o mesmo texto funciona. Consertado com `${ma_extra}` e um guarda que lê o texto de todo script de shell, sem depender do locale, provado por reversão. A `linha/0.15` tem a mesma linha; a avaliação de que ela «ficava como estava» estava errada, e ela foi consertada depois (`40066a7c`, ver «Antes de publicar»). A 12ª etapa, nova, reprovou os 17 roteiros conhecidos; com o duble da F8, os 28 passam. Depois do conserto: `xtask` inteiro (202), `frontend` (272), clippy do `xtask`, fmt e os 28 roteiros |
| 09/10/2026 | F5 onda 5 inteira (I-b: o microfone que entrega zero exato é dito, o volume mora na conexão, nada passa de um servidor ao seguinte, a `AINDA_SEM_FRASE` vazia), F1 até `d56977cd` (recusar os MODs não gasta o convite; a P12 do servidor), F7 até `a0d89668` (a identidade em `SEELE_IDENTIDADE`; o quarto corre junto com a corrida, a 14b de núcleo; a contagem de furos sem prazo por leitura, pendência 29), F3 até `6ecd12bb` (as capacidades, o retrato por lista branca, a recusa pelo nome, o aviso da metade de servidor, os eventos da tela), F8 até `0385b08f` (a raiz, a moldura, SEGURAR PARA FALAR, as abas SERVIDOR, CONVERSA e PESSOAS) e o duble dos volumes (`da0b74d3`) | `4194dcea`, consertado em `12ed17d1` e `f7fb8ab9` | **verde depois de dois consertos**. A primeira rodada (`78aa71ef`) parou com o disco a 700 MB livres: os `target/` de 21 worktrees que sobravam foram apagados com o sim do dono, e o resto foi para a Lixeira. Na segunda: 3415 passaram, 1 falhou, 7 ignorados; as etapas 12 (os 28 roteiros) e 13 (a bancada do celular, nova) verdes. O que falhou foi o G2 da F8: o `cascaDoRetrato` da F3 lia a marca do celular em `base.js`, e passou a perguntar pelo `CELULAR`, que só a casca do celular cria. A bancada reprovou uma vez o volume da folha (a releitura dos volumes do I-b contra um duble sem memória), e com o duble consertado passou 3 em 3. Um conflito de texto na FFI (`fold`: o `ModAviso` da F3 e o `dobrar_no_room` da F5), resolvido com os dois |

Conflitos de texto, resolvidos à mão: na primeira, o `EndReason` de `crates/seele-ffi/src/types.rs`, onde o lote H e a F1 acrescentaram razões no mesmo lugar. Ficaram as duas listas. Na segunda, o índice dos ADRs (`docs/adr/README.md`), com as linhas do 0056, do 0057 emendado e do 0058.
