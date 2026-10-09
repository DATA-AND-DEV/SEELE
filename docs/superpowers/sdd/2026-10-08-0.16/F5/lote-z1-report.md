# Relatório do lote Z1 (F5, 0.16)

O item 7 do lote Z: a **pendência 56**. Os testes que leem o log perdiam linhas quando rodavam em paralelo, e o segundo despachante do app chega aos três crates.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a3b816a17b664446c`
- Branch: `0.16/lotes-z1`, aberto em `f60a1305`.
- Target semeado por `cp -c` do `abertos-da-1.0`, e tudo rodado com `CARGO_TARGET_DIR` vazio.
- Status: **DONE_WITH_CONCERNS**. As preocupações estão no fim, e nenhuma bloqueia.

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

1. **No core, a prova só decide rodada sozinha.**
   - Sem o conserto, ela reprovou 10 em 10 sozinha. A biblioteca inteira, com ela dentro, passou 5 em 5.
   - Com os 25 rastros do crate de pé ao mesmo tempo, quase sempre há dois ou mais despachantes vivos, e a ordem forçada não chega ao caso de um só.
   - No server e no ffi, a prova reprovou na biblioteca inteira (3 em 3 nos dois). No binário `desconhecida`, reprovou 7 em 10.
   - Então quem tirar o auxiliar de `Rastro::a_partir_de` não vê o vermelho na bateria do core, só rodando a prova sozinha.
   - A prova do app tem o mesmo formato e, pelo mesmo mecanismo, deve ter a mesma limitação. Não medi.
   - Para ela decidir sempre, seria preciso rodar a ordem forçada num processo filho: o teste chamaria o próprio binário com um filtro. É outro formato, e ficou fora deste lote.
2. **Só a recepção reproduziu o defeito.**
   - Sem o conserto, só a recepção ficou vermelha nas minhas rodadas: 5 em 5 no módulo.
   - Os outros testes expostos passaram em todas as rodadas sem ele: os 25 do core, os de `admissao`, `pedidos` e `alcance/encontro`, o da trilha no ffi e o da mensagem desconhecida.
   - Eles estão expostos pelo mecanismo, mas nenhum foi visto falhar. O conserto os protege sem que eu tenha uma medida de antes para cada um.
3. **Nenhum guarda pega um teste de log novo que esqueça o auxiliar.**
   - No server, no `desconhecida.rs` e no ffi, a chamada é feita à mão em cada captura. Um teste novo com `with_default` e sem a chamada passa sozinho e pode falhar em paralelo.
   - No core, o risco é menor, porque tudo passa por `Rastro::a_partir_de`.
   - Um guarda de texto barato resolveria: todo arquivo com `subscriber::with_default|set_default` precisa ter `segundo_despachante()`. Não o fiz, porque o brief pedia a conferência pelo `grep`, e ela está acima.
4. **Cinco cópias do mesmo auxiliar de três linhas:** o app, o core, a biblioteca do server, o `desconhecida.rs` e o ffi. É o desenho do brief («um auxiliar por crate, e não um crate novo»). Cada cópia aponta a do app como referência.
5. **`desconhecida.rs` deixou de ter um teste só.** A doc dizia «Um teste só, e não dois, de propósito». Agora o arquivo tem o caso e a prova, e a doc foi reescrita para dizer por que isso passou a ser seguro. Com o conserto, o binário inteiro passou 10 em 10.
