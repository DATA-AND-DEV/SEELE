# API 8, parte 4 (poder `programa`) — o núcleo: decisões, provas e pendências

O núcleo de `programa.baixar` e `programa.rodar` em `seele-core::programa`
(ramo `desenho/api-8-programa`, a partir de `92d678ba`). Spec:
`docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md`, §5 e
«Como provar que funciona». **Não depende da parte 3**: os objetos do QuickJS,
a ligação com o registro do aceite, os comandos do Tauri e a casca ficam para
depois dela. Cada decisão diz o custo, se estiver errada.

## Decisões

- **P1. Mora em `seele-core::programa`.** A metade de máquina do túnel já está
  em `seele-core::tunel`, e o plano da parte 3 põe o registro do aceite em
  `seele-core::aceites_de_maquina`. O processo roda na máquina de quem tem o
  app, e não no `seeled`. *Custo:* se a cola da parte 3 rodar dentro do
  `seele-server` (o executor QuickJS), ela chama isto pelo `seele-ffi`, e não
  direto.
- **P2. Dependências: `reqwest` (só `rustls-no-provider`), `rustls` (`ring`),
  `sha1` e `sha2` no `seele-core`. Medido: zero crate novo no `Cargo.lock`**,
  só seis linhas de aresta. O `reqwest` e o `rustls` vêm do `seele-app` e do
  `seele-server`; `sha1` e `sha2` já estavam na árvore deste crate pelo
  `seele-enlace`. Dev: `rcgen` e `tokio-rustls`, também já no lock.
  *Custo:* o `seele-core` passa a linkar um cliente HTTP.
- **P3. Uma `Politica` explícita em cada `Programa`**: `dominios`,
  `comando: Option<ComandoAceito>`, `teto_de_disco`, `raiz`. Um `Programa` por
  servidor de jogo aceito; soltá-lo encerra tudo o que ele começou. Os tipos
  espelham `OfertaDeMaquina` da parte 3 (`ArtefatoOferecido`,
  `ComandoOferecido`, `AlgoritmoDeHash`) sem importá-los. *Custo:* dois enums
  `AlgoritmoDeHash` até a cola unir os dois.
- **P4. Domínio aceito é host exato ou subdomínio por rótulo**, sem diferença
  de caixa e sem o ponto final: `mojang.com` aceita `piston-data.mojang.com` e
  recusa `evilmojang.com`. Qualquer porta. *Custo:* aceitar `mojang.com` abre
  todo subdomínio dele.
- **P5. Redirecionamento: seguido até 5 saltos (`MAX_REDIRECIONAMENTOS`), cada
  salto conferido como a URL original (`https` e domínio aceito).** Um salto
  para fora é recusado (`RedirecionouParaFora { de, para }`) **sem pedir o
  destino**. O `reqwest` roda com `Policy::none()`, e quem segue é o núcleo.
  As origens reais redirecionam para CDN (GitHub → `objects.githubusercontent.com`),
  então a receita tem de declarar o domínio do CDN. *Custo:* uma receita que
  esqueça o CDN falha no primeiro download, com o domínio nomeado na recusa.
- **P6. Vale o hash mais forte declarado** (`o_mais_forte`, ordem
  `Sha1 < Sha256 < Sha512`); os outros são ignorados, e não conferidos junto.
  Pedido sem hash é recusado (`SemHash`). *Custo:* um `sha1` errado ao lado de
  um `sha512` certo passa calado.
- **P7. O `Content-Length` diferente do declarado recusa antes de ler**
  (`TamanhoAlegadoDiferente`); sem ele, o corpo é cortado no primeiro pedaço
  que passa do declarado, **antes de escrevê-lo**. Corpo curto:
  `MenorQueODeclarado`. *Custo:* uma origem que comprima por conta própria
  seria recusada; o `gzip` do `reqwest` fica desligado de propósito.
- **P8. Escreve em `.<nome>.<n>.seele-parcial` ao lado do destino, e renomeia
  só se tudo bateu.** Qualquer outro fim — hash, tamanho, rede, cancelamento,
  `Programa` solto — apaga o parcial (`Drop`). *Custo:* um processo morto à
  força (SIGKILL no app) deixa o parcial; o teto de disco não o conta.
- **P9. O teto de disco reserva o tamanho declarado** de cada download em
  andamento, sob um cadeado tomado **antes** de medir o disco. A conta é
  `ocupado + reservado + (declarado − arquivo que ele substitui) > teto`, e a
  recusa leva `usado`, `pedido` e `teto`. A medida soma arquivos sem seguir
  link e sem os parciais. *Custo:* o que o próprio jogo escreve depois não é
  vigiado — o teto é conferido em cada `baixar`, e não continuamente.
- **P10. Caminhos: a regra do `dados` (`seele_proto::mods::inner_path`) mais
  três.** As duas barras separam nos dois sistemas (`a\..\b` é `..` também no
  Mac); `/x`, `\x`, `C:` e UNC são absolutos em qualquer sistema; cada prefixo
  existente que é link é resolvido e comparado com a raiz canônica, e um link
  que não resolve é recusado. A pasta é conferida de novo depois de criada.
  *Custo:* resta a janela entre conferir e abrir, contra um link criado pelo
  próprio jogo (spec, «avisar e isolar depois»).
- **P11. O comando é comparado como texto exato com o aceito, e os argumentos
  como vetor exato.** O comando é um caminho dentro da raiz, nunca procurado
  no `PATH`; no Windows, `java/bin/java` acha `java.exe`. A aceitação é
  conferida antes do caminho, para a recusa dizer qual é o aceito.
  *Custo:* `java\bin\java` é recusado onde `java/bin/java` é o aceito.
- **P12. O processo herda o ambiente da pessoa, e o `stdin`/`stdout`/`stderr`
  são canos.** No Windows, `CREATE_NO_WINDOW`. *Custo:* uma variável do
  ambiente da pessoa chega ao jogo (o mesmo que rodar com a conta dela, que a
  spec já aceita).
- **P13. A fila de eventos tem teto (4 096) e a linha também (16 KiB).** A
  linha que não cabe é perdida e contada, sem segurar o processo; a longa é
  cortada e contada; os dois números chegam no `FimDoProcesso`. Cada leitura
  deixa duas vagas, e o `Terminou` sempre cabe. *Custo:* um MOD que não lê
  perde saída — dito, e não calado.
- **P14. Soltar um `Processo` ou um `Download` não os mata**: os dois são do
  `Programa`. `Processo::matar`, `Download::cancelar`,
  `Programa::encerrar_tudo` (mata e **espera**) e o `Drop` do `Programa`
  (manda matar) são os caminhos. Todo filho tem `kill_on_drop`, então o
  runtime caindo também mata. *Custo:* um objeto JS coletado pelo GC não para
  o jogo; quem para é o `Programa` (o aceite).
- **P15. Toda recusa é contada por nome** (`Programa::recusas()`, um
  `BTreeMap<&'static str, u64>`) e vai ao log com `tracing::warn!`. *Custo:*
  a contagem vive com o `Programa`, e some com ele.
- **P16. Testes: origem injetável (`trait Origem`) nos testes de unidade, e um
  servidor HTTPS de verdade em `127.0.0.1` (`rcgen` + `tokio-rustls`, com
  `OrigemHttps::confiando_so_em`) nos de integração**, no molde do servidor
  TCP local de `mods::mundo`. Processos: o binário
  `seele-programa-de-teste` deste crate, e não `sh`, fora de qualquer
  `#![cfg(windows)]`. A morte é medida por um batimento num arquivo, sem FFI.
  *Custo:* um `[[bin]]` a mais no `seele-core`, que nenhum empacotamento
  constrói (conferido: `empacotar/` e a CI não usam `--bins`).

## Medidas

- **O corte pelo `reqwest` real** (`programa_https.rs`, três rodadas no Mac,
  30/10): declarado 1 048 576; corte em **1 056 768** (um pedaço de 8 KiB
  depois); o servidor escreveu **1 769 472 a 1 835 008** bytes no total, o
  resto nos buffers do TLS e do TCP. O teste exige menos que declarado + 8 MiB.
- **O corte pela origem de mentira:** pedaços de 1 000 bytes sem fim,
  declarado 10 000 → a origem entregou **11** pedaços, nem um a mais.

## As provas dos guardas (revertidos um a um)

Cada guarda foi revertido sozinho, o teste dele rodado e o arquivo restaurado
do commit (`git checkout`). Todos falharam revertidos, menos o G20 na primeira
vez, que levou a um teste mais forte.

| # | Guarda revertido | Teste que falhou |
|---|---|---|
| G1 | corte no pedaço que passa (`depois > tamanho`) | `corpo_maior_e_cortado_no_pedaco_que_passa` (prazo de 10 s) |
| G1b | o mesmo, pelo HTTPS real | `corpo_sem_fim_e_cortado_pelo_https_de_verdade` (prazo de 30 s) |
| G2 | domínio antes do pedido | `url_fora_do_aceite_e_recusada_antes_de_qualquer_pedido` |
| G3 | subdomínio por rótulo (o `.` antes) | `host_exato_ou_subdominio_por_rotulo` |
| G4 | só `https` | `url_fora_do_aceite_e_recusada_antes_de_qualquer_pedido` |
| G5 | cada salto conferido | `redirecionamento_para_fora_e_recusado_sem_pedir_o_destino` |
| G6 | hash conferido | `hash_que_nao_bate_apaga_e_conta` |
| G7 | parcial apagado no `Drop` | 4 testes, entre eles `encerrar_no_meio_apaga_o_parcial` |
| G8 | `Content-Length` diferente | `tamanho_alegado_diferente_recusa_antes_de_ler` |
| G9 | corpo menor | `corpo_menor_e_recusado_e_apagado` |
| G10 | teto de disco | `disco_cheio_recusa_com_os_numeros_antes_do_pedido` |
| G11 | reserva entre downloads | `downloads_simultaneos_reservam_o_disco` |
| G12 | `..` recusado | `destino_fora_da_raiz_e_recusado_antes`, `pai_e_recusado_…`, e em `rodar`: `pasta_fora_da_raiz_e_recusada` |
| G13 | absoluto recusado | `destino_fora_da_raiz_e_recusado_antes`, `absoluto_e_recusado_…`, e em `rodar`: `pasta_fora_da_raiz_e_recusada` |
| G14 | link que sai da raiz | `link_que_sai_da_raiz_e_recusado` |
| G15 | comando aceito | `comando_ou_argumentos_fora_do_aceite_sao_recusados_e_contados` |
| G16 | argumentos aceitos | o mesmo |
| G17 | o `Drop` do `Programa` manda matar | `soltar_o_programa_mata_o_processo` |
| G18 | `kill_on_drop(true)` | `o_runtime_caindo_mata_o_processo` |
| G19 | a vaga do `Terminou` na fila | `saida_demais_e_perdida_e_contada_sem_parar_o_processo` |
| G20 | `encerrar_tudo` espera (e não só manda) | `encerrar_tudo_mata_espera_e_fecha_a_porta` — **passou** revertido na primeira versão (o batimento olhado 300 ms depois dava tempo ao kill); o teste agora exige o `Terminou` já na fila quando `encerrar_tudo` volta, e falha revertido |
| G21 | o mais forte vale | `vale_o_mais_forte_que_a_origem_der`, `confere_o_mais_forte_e_ignora_o_mais_fraco` |

## Aberto

- **Netos do processo.** Só o filho é morto. Matar a árvore pede grupo de
  processos no Unix e *job object* no Windows; os dois são FFI, e o workspace
  tem `unsafe_code = "forbid"`. **Não medido** se o servidor do jogo ou um
  instalador de loader cria netos; medir antes de decidir se isto bloqueia.
- **Os testes de processo e de link não rodaram no Windows** daqui; foram
  escritos para rodar (o binário de teste, `EXE_SUFFIX`, a barra invertida).
  O de link pula no Windows sem privilégio de criar link, e diz que pulou.
- **A cola**, depois da parte 3: `maquina.programa.baixar/rodar` no QuickJS,
  a `Politica` montada do `AceiteDeMaquina` (`oferta.origens`,
  `oferta.rodar`, `oferta.disco_maximo`, a pasta `<config>/jogos/…` da
  decisão 10 do plano da parte 3), a contagem das recusas na tela do MOD, e
  o `encerrar_tudo` no `ExitRequested` do app, no DESLIGAR e no revogar.
- **As URLs dos `artefatos` do aceite** também podem ser exigidas (e não só
  os domínios); a cola decide.
- **O teto de disco não vigia o que o jogo escreve** depois de rodar (P9).
