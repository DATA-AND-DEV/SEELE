# Relatório do lote Z1 (F5, 0.16)

O item 7 do lote Z: a **pendência 56**. Os testes que leem o log perdiam linhas quando rodavam em paralelo, e o segundo despachante do app chega aos três crates.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a3b816a17b664446c`
- Branch: `0.16/lotes-z1`, aberto em `f60a1305`.
- Target semeado por `cp -c` do `abertos-da-1.0`, e tudo rodado com `CARGO_TARGET_DIR` vazio.
- Status: **DONE_WITH_CONCERNS**. As preocupações estão no fim, e nenhuma bloqueia.
- **Rodada 1** (FIX_BASE `5ea62d22`): **DONE_WITH_CONCERNS**. Ela resolve as preocupações 1 e 3 e corrige a 2. A seção «Rodada 1», no fim, diz o que mudou.

## Os commits

| Crate | Commit | O que entra |
|---|---|---|
| `seele-core` | `a4fcd99e` | `rastro_de_teste::segundo_despachante()`. `Rastro::a_partir_de` passa a chamá-lo, e a prova de ordem forçada vem pelo `Rastro`. |
| `seele-server` | `cefb5c53` | O módulo novo `#[cfg(test)] pub(crate) mod rastro_de_teste` (`src/rastro_de_teste.rs`), com o auxiliar e a prova. As quatro capturas o chamam, e o truque local da recepção vira o auxiliar. `tests/desconhecida.rs` ganha a própria cópia e a própria prova. |
| `seele-ffi` | `12bb1769` | Uma cópia local em `mod trilha_no_log`, chamada por `o_log_da_trilha_diz_qual_candidato_deu_o_que`, e a prova feita com o `Espiao`. |

Este relatório entra num quarto commit, o último do branch.

O auxiliar é o do app, linha por linha: um `OnceLock<tracing::Dispatch>` com `NoSubscriber::new()`. Não toquei no auxiliar do app. Não mexi em `apps/`, no `executor.rs` nem no `seele-motor`.

## O que cada auxiliar cobre

### `seele-core`: `Rastro::a_partir_de`, um lugar só

São 25 pontos de captura. Todo `set_default` do crate recebe um `Rastro` feito por `a_partir_de` na linha de cima, e conferi os pares um a um. Nenhum teste de log do core passa por fora dele, e por isso nenhum chama o auxiliar direto.

- **`enlace.rs`, 10 testes:**
  - `o_pin_que_um_aperto_desfaz_vai_ao_log_com_a_chave_e_o_motivo`
  - `o_pin_orfao_desfeito_depois_da_corrida_vai_ao_log_com_a_chave_e_o_motivo`
  - `a_limpeza_que_nao_apaga_nada_nao_escreve_nada`
  - `o_pin_que_um_impostor_fez_fixar_e_desfeito_e_o_log_nao_diz_que_o_tls_terminou`
  - `o_pin_orfao_de_um_candidato_que_estourou_o_prazo_vai_ao_log_com_a_falha_dele`
  - `a_volta_recusada_diz_no_registro_por_que_a_sessao_acabou`
  - `a_recusa_de_um_candidato_perdedor_vai_ao_log_mesmo_quando_outro_entra`
  - `o_candidato_perdedor_que_nao_prova_a_chave_vai_ao_log_como_aviso`
  - `sem_vencedor_a_recusa_de_um_candidato_vai_ao_log_do_mesmo_jeito`
  - `um_anexo_que_nao_foi_salvo_deixa_o_porque_no_log`
- **`encontro.rs`, 8 testes.** A pendência contava 7.
  - `os_achados_vao_com_o_ipv4_na_frente_e_a_lista_vazia_diz_no_rastro`
  - `um_ponto_que_nao_se_procura_diz_no_rastro_por_que_nao`
  - `o_bilhete_desta_volta_so_troca_o_aviso_pela_escuta_que_o_servidor_confirma`
  - `a_consulta_que_estoura_o_prazo_diz_no_rastro_o_que_perguntou_e_o_que_ouviu`
  - `um_nome_que_nao_resolve_no_prazo_diz_no_rastro_que_foi_o_prazo`
  - `uma_pergunta_que_nao_sai_volta_na_hora_e_diz_no_rastro_por_que_nao_saiu`
  - `um_endereco_sem_rota_nao_esconde_o_seguinte_do_mesmo_ponto`
  - `o_rastro_da_consulta_nao_grava_a_marca_de_quem_se_procura`
- **`anexo_no_disco.rs`, 4 testes:**
  - `sem_link_fisico_o_recuo_tambem_nao_substitui_nada`
  - `um_parcial_que_nao_sai_da_pasta_fica_dito_no_log`
  - `uma_reserva_que_nao_sai_da_pasta_fica_dita_no_log`
  - `um_parcial_que_ja_estava_na_pasta_fica_como_estava_e_e_dito_no_log`
- **`tofu.rs`:** o auxiliar `o_que_decide_diz`, e por ele 3 testes:
  - `a_recusa_por_cima_de_um_pino_que_conferia_diz_isso_no_log`
  - `a_recusa_no_primeiro_contato_vai_ao_log_sem_dizer_que_havia_pino`
  - `a_chave_trocada_vai_ao_log_com_as_duas_impressoes`
- **`par.rs`:** `repassar_avisa_quando_o_canal_fecha_sem_a_chave_chegar`.
- **`client.rs`:** `um_aperto_de_mao_sem_decisao_do_verificador_falha_e_diz_por_que`.
- **A prova:** `rastro_de_teste::uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada`.

### `seele-server`: `crate::rastro_de_teste::segundo_despachante()`

Cada captura chama o auxiliar antes do `with_default` ou do `set_default`.

- **`recepcao.rs`:** `cada_transicao_vai_ao_log_com_o_prefixo_do_codigo_e_nunca_o_codigo_inteiro`.
  - A linha `let _segundo = tracing::Dispatch::new(NoSubscriber::default())` virou a chamada ao auxiliar.
  - O comentário guarda o sentido e as medidas da F7 (3/3 vermelhas sem ele, 20/20 verdes com ele), e agora diz que é o auxiliar do crate.
- **`admissao.rs`:** `erro_de_infraestrutura_no_banco_fica_distinguivel_de_sala_inexistente`.
- **`mods/pedidos.rs`:** o auxiliar `o_registro_da_recusa`, e por ele 4 testes:
  - `a_recusa_do_pedido_escreve_o_mod_id_que_o_grep_do_guia_acha`
  - `a_recusa_do_catalogo_diz_catalogo_e_nao_um_mod_id_em_branco`
  - `um_id_so_do_que_o_filtro_tira_diz_que_nao_se_lia_e_nao_um_mod_id_em_branco`
  - `um_id_com_quebra_de_linha_nao_forja_uma_linha_no_registro`
- **`alcance/encontro.rs`:** `Rastro::de_info`, que devolve o `DefaultGuard`. Por ele passam 7 testes:
  - `o_registro_que_deixa_de_sair_diz_no_log_uma_vez_e_diz_quando_volta`
  - `uma_fila_cheia_num_tique_nao_vira_linha_no_log`
  - `atender_diz_no_log_quando_o_registro_da_subida_nao_sai`
  - `o_relogio_torto_vira_estado_e_uma_linha_no_log`
  - `um_ponto_que_cala_o_enc2_vira_sem_resposta_e_e_dito`
  - `um_cookie_que_o_ponto_nao_aceita_e_dito_e_nao_vira_laco`
  - `a_mesma_resposta_repetida_diz_uma_linha_so_e_os_silencios_contam_por_tique`
- **A prova:** `rastro_de_teste::uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada`.

### `seele-server/tests/desconhecida.rs`: a cópia do binário de integração

O crate não tem um `tests/` comum, e o `#[cfg(test)]` da biblioteca não chega a um teste de integração. Por isso o arquivo leva a própria cópia, `fn segundo_despachante()`.

- **Quem a usa:** `a_mensagem_desconhecida_e_pulada_contada_e_registrada_uma_vez`, que a chama na primeira linha.
- **A prova:** `uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada`, com o mesmo `Rastro`/`fmt` do teste de cima.
- **A doc do teste de cima mudou.** Ela dizia «Um teste só, e não dois, de propósito». Agora diz que o caso é um só de propósito e que o segundo despachante vem antes da captura. Ela guarda a medida de 08/10 (seis de seis reprovações em paralelo) e diz que o único outro teste do arquivo é a prova. A razão do `current_thread` ficou.

### `seele-ffi`: a cópia local em `mod trilha_no_log`

- **Quem a usa:** `o_log_da_trilha_diz_qual_candidato_deu_o_que`.
- **A prova:** `trilha_no_log::uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada`, com o `Espiao` do módulo.

## TDD: o vermelho antes do conserto

Em cada crate, escrevi a prova primeiro e a rodei sozinha, com o auxiliar ainda ausente ou de corpo vazio. Ela ficou vermelha pela razão certa: o rastro vazio, com a frase dela.

| Onde | Comando | Saída |
|---|---|---|
| `seele-core` | `cargo test -p seele-core --lib rastro_de_teste::` (antes de `a_partir_de` chamar o auxiliar) | `uma linha que outra thread tocou primeiro sumiu do rastro, … : []` |
| `seele-server` lib | `cargo test -p seele-server --lib rastro_de_teste::` (corpo vazio) | `… sumiu do rastro, … :` (vazio) |
| `desconhecida.rs` | `cargo test -p seele-server --test desconhecida uma_linha_que_outra_thread` (corpo vazio) | `… sumiu do rastro, e o teste da mensagem desconhecida passa ou falha conforme a ordem das threads:` (vazio) |
| `seele-ffi` | `cargo test -p seele-ffi --lib trilha_no_log::uma_linha_que_outra_thread` (corpo vazio) | `… sumiu do espião, … : []` |

Com o auxiliar, as quatro ficaram verdes.

## As rodadas sem o conserto

Fiz cada reversão depois do commit do crate, de dois jeitos:

- no core, tirando a linha `segundo_despachante();` de `Rastro::a_partir_de`;
- no server, no `desconhecida.rs` e no ffi, esvaziando o corpo do `segundo_despachante()`. No server, isso tira o auxiliar de todas as capturas da biblioteca de uma vez.

| Rodada sem o conserto | Vermelhas |
|---|---|
| **Prova do core, sozinha** | **10 em 10** |
| Biblioteca do core inteira (512 testes) | 0 em 5. A prova passa junto com os outros: ver a preocupação 1. |
| Biblioteca do core inteira com `--test-threads=4` (medida da rodada 1) | 12 em 12: `encontro::testes::a_consulta_que_estoura_o_prazo_diz_no_rastro_o_que_perguntou_e_o_que_ouviu` reprova em todas. Ver a seção «Rodada 1». |
| **Prova do server, sozinha** | **10 em 10** |
| **Módulo `recepcao::`** | **5 em 5**: `o toque e a recusa não deram uma linha cada`, só com a linha do toque. É a medida da F7 (3 em 3). |
| Biblioteca do server inteira (625 testes) | 3 em 3, sempre a prova. A recepção passou no conjunto. |
| Módulos `admissao::`, `mods::pedidos::` e `alcance::encontro::` | 0 em 3 cada |
| **Prova do `desconhecida.rs`, sozinha** | **10 em 10** |
| Binário `desconhecida` inteiro (2 testes) | 7 em 10, sempre a prova. O teste da mensagem desconhecida não reprovou nenhuma vez. |
| **Prova do ffi, sozinha** | **10 em 10** |
| Módulo `trilha_no_log::` (5 testes) | 0 em 5 |
| Biblioteca do ffi inteira (161 testes) | 3 em 3, sempre a prova |

Rodada sozinha, a prova reprovou em todas as 40 rodadas sem o conserto, nos quatro lugares. A ordem é forçada, e o resultado não depende de sorte.

## As reversões, conferidas byte a byte

Em cada uma, desfiz o conserto, rodei o que está acima e devolvi o arquivo com `git checkout --`. `git diff --exit-code` saiu 0 e a árvore ficou limpa. O `shasum -a 256` do arquivo restaurado bate com o do commit:

- `crates/seele-core/src/rastro_de_teste.rs`: `ff5fcc930b14042a75c15a5dbed1637d293ddf39ca0cdbce7a721510d93e6112`, igual ao `git show HEAD:…`;
- `crates/seele-server/src/rastro_de_teste.rs`: `8a801719cb90533a016d56860525f76a2917d7d20070e7af945d47075f1b2302`, igual ao `git show HEAD:…`. Este foi revertido duas vezes: a primeira para a prova, a recepção e a biblioteca, a segunda para os outros três módulos. As duas voltas conferiram limpo;
- `crates/seele-server/tests/desconhecida.rs`: `42558e9747f669a1fc9b6fa183815c7bb1e493c7c399f6904df60e392b51fda8`;
- `crates/seele-ffi/src/lib.rs`: `7b35984467612697d6f23006a1ad5e305fcdc0bce11495d7b3d91871e0c47cb4`.

## As rodadas com o conserto

| Rodada com o conserto | Vermelhas |
|---|---|
| Prova do core, sozinha | 0 em 10 |
| Biblioteca do core inteira | 0 em 5, mais 1 antes do commit e 1 no HEAD final |
| Prova do server, sozinha | 0 em 10 |
| **Módulo `recepcao::`** | **0 em 20** |
| Módulos `admissao::`, `mods::pedidos::` e `alcance::encontro::` | 0 em 5 cada |
| Biblioteca do server inteira | 0 em 5, mais 1 antes do commit e 1 no HEAD final |
| Prova do `desconhecida.rs`, sozinha | 0 em 10 |
| Binário `desconhecida` inteiro | 0 em 10, mais 1 no HEAD final |
| Prova do ffi, sozinha | 0 em 10 |
| Módulo `trilha_no_log::` | 0 em 10 |
| Biblioteca do ffi inteira | 0 em 5, mais 1 antes do commit e 1 no HEAD final |

## Os testes que rodaram no HEAD final

- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-core --lib`: 512 passaram.
- `cargo test -p seele-server --lib`: 625 passaram.
- `cargo test -p seele-server --test desconhecida`: 2 passaram.
- `cargo test -p seele-ffi --lib`: 160 passaram e 1 ficou ignorado. O ignorado já existia.
- `cargo fmt --all -- --check`: ok.
- `cargo clippy -p seele-core -p seele-server -p seele-ffi --all-targets --all-features -- -D warnings`: limpo.
  - Também rodei o de cada crate antes do commit dele.
  - Nenhum item público mudou, então não há crate dependente a acrescentar.

A conformidade não rodou, porque está pausada e o lote não precisa dela.

## A busca no fim

`grep -rln "subscriber::with_default\|subscriber::set_default" crates/` devolve 14 arquivos. São os 13 de antes, mais o `rastro_de_teste.rs` novo do server. O do core já aparecia antes, pelo exemplo da doc dele.

- `seele-core`:
  - `src/anexo_no_disco.rs`
  - `src/client.rs`
  - `src/encontro.rs`
  - `src/enlace.rs`
  - `src/par.rs`
  - `src/rastro_de_teste.rs`
  - `src/tofu.rs`
- `seele-ffi`: `src/lib.rs`
- `seele-server`:
  - `src/admissao.rs`
  - `src/alcance/encontro.rs`
  - `src/mods/pedidos.rs`
  - `src/rastro_de_teste.rs`
  - `src/recepcao.rs`
  - `tests/desconhecida.rs`

Também conferi ponto a ponto, com um script. Para cada uma das **35** chamadas de `with_default(`/`set_default(` fora de comentário, há um `segundo_despachante();` ou um `Rastro::a_partir_de(` nas 12 linhas acima. Resultado: **35 com o auxiliar, 0 sem**. As 35 se dividem assim:

- core: 25, mais a prova;
- server lib: 4, mais a prova;
- `desconhecida.rs`: 1, mais a prova;
- ffi: 1, mais a prova.

Uma busca mais larga também não achou outro caminho de captura em `crates/`:

- termos procurados: `set_default`, `with_default`, `Dispatch::new`, `set_global_default`, `.init()`, `try_init()`, `NoSubscriber`, `tracing::dispatcher`, `traced_test`;
- o único `.init()` é o do `main.rs` do `seeled`, que é binário e não teste.

## Para a v6

Nada. Nenhuma promessa pública nova: evento, campo ou frase de `api/README.md`.

## O que não rodou aqui

- **Nada deste lote tem `cfg(windows)` ou `cfg(target_os = "linux")`.** Tudo rodou neste Mac.
- **O app não foi compilado nem rodado.** Só li o auxiliar de referência dele.

## Preocupações

1. **No core, a prova só decide rodada sozinha.** *Resolvida na rodada 1: as quatro provas rodam a ordem forçada num processo filho.*
   - Sem o conserto, ela reprovou 10 em 10 sozinha. A biblioteca inteira, com ela dentro, passou 5 em 5.
   - Com os 25 rastros do crate de pé ao mesmo tempo, quase sempre há dois ou mais despachantes vivos, e a ordem forçada não chega ao caso de um só.
   - No server e no ffi, a prova reprovou na biblioteca inteira (3 em 3 nos dois). No binário `desconhecida`, reprovou 7 em 10.
   - Então quem tirar o auxiliar de `Rastro::a_partir_de` não vê o vermelho na bateria do core, só rodando a prova sozinha.
   - A prova do app tem o mesmo formato e, pelo mesmo mecanismo, deve ter a mesma limitação. Não medi.
   - Para ela decidir sempre, seria preciso rodar a ordem forçada num processo filho: o teste chamaria o próprio binário com um filtro. É outro formato, e ficou fora deste lote.
2. **Dois testes reproduziram o defeito, não um só** (corrigido na rodada 1, M3).
   - A recepção ficou vermelha 5 em 5 no módulo, sem o conserto.
   - A revisão achou a segunda vítima, e eu a reproduzi: `encontro::testes::a_consulta_que_estoura_o_prazo_diz_no_rastro_o_que_perguntou_e_o_que_ouviu`, do core.
     - Sem a chamada em `a_partir_de` e com `--test-threads=4`, ela reprovou 12 em 12.
     - Com as threads padrão, reprovou 0 em 14.
     - Quatro threads é um tamanho comum de runner de CI. Nas minhas rodadas da rodada 0, com as threads padrão, eu não a vi.
   - Os outros testes expostos passaram em todas as rodadas sem o conserto: os outros 24 do core, os de `admissao`, `pedidos` e `alcance/encontro`, o da trilha no ffi e o da mensagem desconhecida. O mecanismo os expõe, mas nenhum foi visto falhar.
3. **Nenhum guarda pega um teste de log novo que esqueça o auxiliar.** *Resolvida na rodada 1: o guarda `xtask/tests/log_com_segundo_despachante.rs`.*
   - No server, no `desconhecida.rs` e no ffi, a chamada é feita à mão em cada captura. Um teste novo com `with_default` e sem a chamada passa sozinho e pode falhar em paralelo.
   - No core, o risco é menor, porque tudo passa por `Rastro::a_partir_de`.
   - Um guarda de texto barato resolveria: todo arquivo com `subscriber::with_default|set_default` precisa ter `segundo_despachante()`. Não o fiz, porque o brief pedia a conferência pelo `grep`, e ela está acima.
4. **Cinco cópias do mesmo auxiliar de três linhas:** o app, o core, a biblioteca do server, o `desconhecida.rs` e o ffi. É o desenho do brief («um auxiliar por crate, e não um crate novo»). Cada cópia aponta a do app como referência.
5. **`desconhecida.rs` deixou de ter um teste só.** A doc dizia «Um teste só, e não dois, de propósito». Agora o arquivo tem o caso e a prova, e a doc foi reescrita para dizer por que isso passou a ser seguro. Com o conserto, o binário inteiro passou 10 em 10. Na rodada 1 (M1), a doc passou a dizer que a razão morreu pelo auxiliar, com a medida da revisão.

## Rodada 1

O brief é `lote-z1-rodada1.md`, com FIX_BASE `5ea62d22`. A revisão deu o conserto como certo e achou uma segunda vítima, no core. Esta rodada faz as provas decidirem sempre, acrescenta o guarda dos testes de log novos e acerta três docs.

### Os commits

| Item | Commit |
|---|---|
| 1. As quatro provas rodam a ordem forçada num processo filho | `f4cba99e` |
| 2. O guarda do xtask | `3230eb9a` |
| 3. M1 e M2 (docs) | `e6ee52ba` |
| M3 e esta seção | o commit deste relatório, o último do branch |

`apps/seele-app/src/main.rs` não foi editado: o guarda só o lê. A mutação 4, abaixo, o mudou por um instante, e ele voltou idêntico, conferido com `cmp`. `git status` ficou limpo depois de cada mutação.

### M3: a segunda vítima, no core

Reproduzi a medida da revisão. Sem a chamada em `Rastro::a_partir_de` e com `cargo test -p seele-core --lib -- --test-threads=4`, `encontro::testes::a_consulta_que_estoura_o_prazo_diz_no_rastro_o_que_perguntou_e_o_que_ouviu` reprovou **12 em 12**:

- 6 em 6 com a prova velha;
- 1 em 1 no vermelho da prova nova;
- 5 em 5 na reversão depois do commit.

A frase é a dela: «o resultado da consulta deixou de ir para o rastro (`info`): «o ponto calou» e «o anfitrião não está» voltam a não deixar dado nenhum». O rastro só traz a linha do prazo.

Com as threads padrão, ela reprovou 0 em 14. Com o conserto e 4 threads, 0 em 7.

### Item 1: as provas decidem sempre

A prova roda o próprio binário com `--exact <caminho da prova>` e `--test-threads=1`, marcada por `PROVA56_FILHO`. No filho, ela roda a `ordem_forcada()`, e o pai exige `ok. 1 passed;` na saída. A mensagem do pai traz a saída inteira do filho, com a frase da ordem forçada. O doc das quatro provas deixou de prometer que a ordem forçada decide sozinha, e diz por que o filho.

**Também mudei as provas do server e do ffi**, o que o brief deixava à minha escolha. As duas também passavam sem o conserto, conforme a companhia:

- a do server ficou verde 5 em 5 ao lado de `alcance::encontro::` e `mods::pedidos::`;
- a do ffi ficou verde 5 em 5 no módulo `trilha_no_log::`, na rodada 0.

Agora as quatro têm a mesma forma.

| Prova, sem o conserto | Prova velha | Prova no processo filho |
|---|---|---|
| core, biblioteca inteira, threads padrão | verde 3 em 3 | **vermelha 6 em 6** (1 no vermelho, 5 depois do commit) |
| core, biblioteca inteira, 4 threads | verde 6 em 6 | **vermelha 6 em 6** (1 + 5) |
| server, com `rastro_de_teste:: alcance::encontro:: mods::pedidos::` | verde 5 em 5 | **vermelha 5 em 5** |
| server, biblioteca inteira, padrão / 4 threads | vermelha 3 em 3 (rodada 0) / 3 em 3 | **vermelha 3 em 3 / 3 em 3** |
| `desconhecida`, binário inteiro | vermelha 7 em 10 (rodada 0) | **vermelha 10 em 10** |
| ffi, módulo `trilha_no_log::` | verde 5 em 5 (rodada 0) | **vermelha 5 em 5** |
| ffi, biblioteca inteira | vermelha 3 em 3 (rodada 0) | **vermelha 3 em 3** |

Com o conserto, tudo ficou verde:

| Rodada com o conserto | Vermelhas |
|---|---|
| core, padrão | 0 em 4, mais 1 no HEAD |
| core, 4 threads | 0 em 6, mais 1 no HEAD |
| server, ao lado dos dois módulos | 0 em 3 |
| server, biblioteca inteira, padrão e 4 threads | 0 em 4, mais 1 no HEAD |
| `desconhecida` | 0 em 10, mais 1 no HEAD |
| ffi, módulo `trilha_no_log::` | 0 em 5 |
| ffi, biblioteca inteira | 0 em 2, mais 1 no HEAD |

**O «1 passed» provado.** Apontei o `--exact` do core para `rastro_de_teste::nao_existe`, com o conserto no lugar. O filho saiu com sucesso e `ok. 0 passed`, e o pai reprovou com «… ou o filtro não achou a prova e ela não rodou».

**As reversões.** Guardei cada arquivo consertado no scratchpad antes de mexer: o do core duas vezes (no vermelho e na reversão depois do commit), o do server, o `desconhecida.rs` e o do ffi. Depois de cada rodada, devolvi a cópia, e o `cmp` deu igual em todas, inclusive na mutação do filtro.

### Item 2: o guarda dos testes de log novos

O guarda é `xtask/tests/log_com_segundo_despachante.rs`, da família de `folha_de_lints.rs` e `fronteira_do_celular.rs`. A raiz vem de `CARGO_MANIFEST_DIR/..`. Ele varre os `.rs` de `crates/` e `apps/`, fora de `target/`, e ignora comentários. São 10 testes:

- **`toda_captura_de_log_chama_o_segundo_despachante_antes`:**
  - cada `with_default(`/`set_default(` tem `segundo_despachante()` acima, na mesma função;
  - `Rastro::a_partir_de(` vale só sob `crates/seele-core`;
  - exige pelo menos 30 capturas, para não ficar cego. Hoje são **38**: 35 em `crates/` e 3 no app.
- **`o_rastro_do_core_chama_o_segundo_despachante_em_a_partir_de`:** o corpo de `a_partir_de` chama o auxiliar.
- **`toda_copia_do_segundo_despachante_tem_o_corpo_da_do_app`**, a asserção opcional do brief:
  - as cinco cópias da lista existem;
  - toda `fn segundo_despachante` achada tem o corpo da do app, linha a linha sem recuo;
  - o corpo do app ainda põe de pé um `NoSubscriber`.
- **Sete testes do próprio scanner:**
  - captura sem e com o auxiliar, e com o auxiliar depois da captura;
  - o auxiliar de outra função, e `pub(crate) async fn`;
  - a `fn` aninhada;
  - `a_partir_de` fora do core;
  - o comentário, inclusive o exemplo `//!` da doc do core;
  - `set_default_client_config` e `with_defaults`;
  - a leitura do corpo de uma `fn`.

**O que mudou do protótipo da revisão:**

- **A `fn` aninhada.** Uma `fn` aninhada continua não sendo a função da captura, como no protótipo. Além disso, o corpo dela, entre o começo da função e a captura, é pulado: um `segundo_despachante()` dentro de uma `fn` que ninguém chama não vale. O protótipo o contava.
- **O fim de uma função.** É a primeira `}` no mesmo recuo da linha que a abre, e não a heurística `"    }"`. Uma `fn` que abre e fecha na mesma linha, ou só declara, termina ali.
- **A abertura de uma função.** Aceita `pub`, `pub(crate)`, `pub(super)`, `async`, `const`, `unsafe` e `extern "C"` antes de `fn`.

**O vermelho antes.** Com o scanner vazio (as funções devolvendo nada), 9 dos 10 testes reprovaram. O guarda do repositório reprovou pela frase «o guarda ficou cego: achou só 0 capturas». O teste da `fn` aninhada passou, porque com o scanner vazio as duas asserções dele, na forma de então, exigiam lista vazia. Depois do scanner, 10 em 10 verdes.

**As mutações.** Cada uma foi aplicada, o guarda rodou e o arquivo voltou da cópia, com `cmp` igual:

| Mutação | Teste que reprovou | Onde |
|---|---|---|
| 1. Tirar a chamada de `admissao.rs` | `toda_captura…` | `crates/seele-server/src/admissao.rs:824` |
| 2. Tirar a chamada do teste da trilha do ffi | `toda_captura…` | `crates/seele-ffi/src/lib.rs:9368` |
| 3. Tirar a chamada da prova do server (`ordem_forcada`) | `toda_captura…` | `crates/seele-server/src/rastro_de_teste.rs:121` |
| 4. Tirar a chamada do teste do pânico do app (temporária) | `toda_captura…` | `apps/seele-app/src/main.rs:12101` |
| 5. Um teste novo sem o auxiliar em `pedidos.rs` | `toda_captura…` | `crates/seele-server/src/mods/pedidos.rs:516` |
| 6. `par.rs` sem passar por `a_partir_de` | `toda_captura…` | `crates/seele-core/src/par.rs:2295` |
| 7. `a_partir_de` sem a chamada | `o_rastro_do_core_chama…` | o corpo lido, sem a chamada |
| 8 (a mais). Esvaziar a cópia do ffi | `toda_copia…` | `crates/seele-ffi/src/lib.rs` |

As sete da revisão reprovaram, e a oitava também. A mensagem de cada uma diz o arquivo e a linha, e o que fazer.

### Item 3: docs

- **M1.** Em `crates/seele-server/tests/desconhecida.rs`, a doc do teste principal diz agora:
  - que o arquivo tinha um teste só por causa do defeito;
  - que essa razão morreu pelo auxiliar, e não por haver menos testes. A revisão mediu 10 em 20 vermelhas sem ele, com um vizinho sem captura, e 0 em 30 com ele. Não reproduzi essa medida, e cito a da revisão.

  Saiu a frase sobre o único outro teste do arquivo.
- **M2.** Em `crates/seele-server/src/rastro_de_teste.rs`, a lista «Quem chama hoje» virou a referência ao guarda do xtask, que acha também as capturas novas. Ficou a frase sobre a cópia do `desconhecida.rs`, que diz que o guarda confere o corpo dela.
- **M3.** Está acima, na tabela das rodadas sem o conserto e na preocupação 2.

### Os testes no HEAD da rodada 1

- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-core --lib`: 512 passaram, com as threads padrão e com `--test-threads=4`.
- `cargo test -p seele-server --lib`: 625 passaram.
- `cargo test -p seele-server --test desconhecida`: 2 passaram.
- `cargo test -p seele-ffi --lib`: 160 passaram, e 1 ignorado que já existia.
- `cargo test -p xtask`: 212 passaram, somando 73, 4, 118, 1, 3, 10, 1 e 2. Os 10 são do guarda novo.
- `cargo fmt --all -- --check`: ok.
- `cargo clippy -p seele-core -p seele-server -p seele-ffi -p xtask --all-targets --all-features -- -D warnings`: limpo.
- A conformidade não rodou.

### Preocupações da rodada 1

1. **O guarda lê texto; não é um parser.** Ele vale para código formatado pelo rustfmt, que é o deste repositório. O que ele não vê:
   - **Comentários.** `codigo()` corta a linha no primeiro `//`. Um `"https://…"` numa linha esconde o resto dela.
   - **Funções.** A função de uma captura é achada pelo recuo.
   - **Ramos.** Um auxiliar chamado dentro de um `if`, antes da captura, conta como chamado.
   - **As cópias.** São comparadas linha a linha, sem o recuo. Se o rustfmt quebrar uma delas em mais linhas, porque ela foi para um recuo maior, a comparação reprova até alguém igualá-las. Hoje as cinco têm a mesma forma.
2. **A prova do app tem a mesma limitação que a do core tinha.** Ela ficou fora desta rodada, como o brief manda, porque o K4 mexe no `main.rs`. O guarda lê o app e pega a chamada tirada do teste do pânico (mutação 4), mas a prova de ordem forçada do app continua rodando no mesmo processo.
3. **Cada prova sobe um processo filho.**
   - O custo é pequeno. Não o medi à parte, mas o libtest dá os tempos com o filho dentro: o módulo `trilha_no_log::` termina em 0,01 s, e o binário `desconhecida` em 0,02 s.
   - Isso pede um alvo que execute o próprio binário de teste. O passo de wasm do CI (`cargo xtask web`) roda só os crates do celular, e não estes quatro binários.
   - O filtro do filho é um texto fixo, mas um caminho errado reprova pelo «1 passed», em vez de passar calado.
