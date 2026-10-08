# Relatório do lote M1 da F5 (0.16) · a consulta ao quarto e as marcas

Itens: P51-17 e P51-19 (pendência 51). Brief: `.superpowers/sdd/lotes/lote-m1-brief.md`.
Branch `0.16/lotes-m1`, a partir de `fbeff8e`. Worktree:
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a598a49a547904a6c`.

**Status: DONE.** Os quatro passos do brief fecharam numa sessão. `enlace.rs`,
`hospedagem.rs`, o `lib.rs` do `seele-server`, o `seele-ffi` e o doc de
`onde_mora_hoje` não foram tocados, nem por reversão temporária.

## O que foi feito, item por item

| item | commit | o que mudou |
|---|---|---|
| T5a, T5c | `09accad` | `ordenar_os_achados(ponto, achados)`, pura: IPv4 na frente e a linha «resolveu para nenhum endereço» quando a lista é vazia. `resolver_ponto` devolve a lista inteira, e não um `Option`. |
| M2 do 19 | `d35488a` | `consultar(&[SocketAddr], …)` tenta os endereços um de cada vez, no mesmo `ate`. Por dentro, `PorUmEndereco { SemSocket, NadaSaiu, Perguntou(OndeMora) }`; a escolha é a função pura `escolher` (`Escolha::{Devolver, TentarOSeguinte}`). Para fora, `OndeMora` não muda. Uma linha `info` diz quando a consulta passa ao endereço seguinte. |
| T6f | `5e977ae` | `depois_da_volta(saidas, enviados)`, pura: só a primeira volta recusada inteira acaba a consulta (`NadaSaiu`); uma volta 2 ou 3 recusada conta como volta (`enviados += 1`, `proxima_volta += INTERVALO`), sem laço quente. A ida e volta do `ONDE` passou a ser medida da última volta que saiu (`saiu_em` só anda quando algo saiu). O teste de `consultar_por` com a volta 2 recusada pediu um traço privado `Canal` (ver as preocupações). |
| C1 | `2b20316` | Testes de `teto_depois_da_resposta` (1 ms → `+100 ms`; 200 ms → `+400 ms`) e de `consultar_por` com um ponto falso: a segunda marca dentro do teto, e o `ONDE` perdido (a escuta aos 300 ms entra; sem escuta, a espera vai até a volta seguinte e a pergunta não se repete). |
| T5d | `8e7175a` | `separar_ponto("")` → `Err(EnderecoInvalido)`. `Bilhete::novo` separa o ponto por `separar_ponto` e recusa o vazio junto (com `EnderecoInvalido`, o erro que o doc dele promete; `Bilhete::ler` continua com `BilheteInvalido`). `cargo build -p seele-encontro --examples` compila. |
| T1a | `cc645fb` | `Marcas::do_servidor` confere o texto inteiro, letra e número. `"0123456789abcdef!!!"` dá `None`. |
| T1b | sem commit | Já coberto: a reversão `..15` e a `..17` deixam `as_marcas_de_um_servidor_saem_da_impressao_digital` vermelho (abaixo). Fecha sem teste novo. |
| T1c | `6243f13` | Campos de `Marcas` privados, com `aviso()`, `escuta()` e `servidor()`; `do_servidor` é o único construtor, e um doctest `compile_fail,E0451` o guarda (sem dependência nova). Leituras trocadas: `seele-core`, `seele-server` (e o literal de `subir_atender_de_teste`, que passou a `marcas_da_impressao()`), `seele-conformance` e o trecho `datagrama_do_encontro` de `sementes_do_fuzz.rs` (os bytes das sementes não mudam). As três asserções de `a_escuta_nunca_tem_a_marca_do_aviso` dizem `{impressao}`. |
| T2c, T3c, M3 | `f8f85e3` | `cada_pedido_da_subida_e_do_registro_sai_com_a_marca_dele`: `abrir` de verdade contra um ponto que responde e anota (`ponto_que_responde_e_anota`), e a marca de cada pedido conferida por verbo e origem: o `ONDE` da escuta e os `LEVE` do servidor (subida e tique) com a da escuta; o `MORO` da escuta com a dela; o `MORO` do servidor com a dele. O T2c ganhou também a reversão própria em `o_primeiro_registro_no_quarto_sai_na_subida`. |
| T4a | `640240d` | O laço de reabertura de `largar_uma_hospedagem_sem_encerrar_devolve_a_porta` guarda o último resultado (o erro, ou «subiu em X, e não em Y») e o pânico o diz, sem afirmar «endereço em uso» sem prova. |
| T7a | `25b5aeb` | O teste virou `um_ponto_mudo_ou_que_responde_lixo_volta_mudo_dentro_do_prazo`: o nome do que mede, e um caso novo com um ponto que responde lixo (dois `AQUI` tortos, um curto, um `MORO`), que tem de dar `PontoMudo`. |
| T8b, T8d | `640240d` | `o_link_leva_ate_o_servidor_e_a_escuta_de_hoje` lê o bilhete do `enc=` de volta (`Convite` → texto → `analisar`), pergunta ao quarto com um `QUEM` cru (`quem_mora`) para separar a culpa do anfitrião da da FFI, percorre as duas formas do ponto e exige que sejam diferentes; o comentário falso desde `a6c2cc0` foi corrigido. |
| 1B daqui | `4b127e3` | `ponto_que_conta(aviso)` só conta o `Pedido::Leve` com a marca do aviso esperado; `expect` com motivo no lugar de `.ok()?` em `server_de_teste`, `ponto_que_conta`, `lembrar_de_ontem` e `server_com_banco` (`#![allow(clippy::expect_used)]` no arquivo, como os vizinhos). |
| T5e | `e2a5b72` | `resolver` do degrau 4 separa o ponto por `seele_proto::uri::separar_ponto`, com o comportamento de antes, guardado por `o_ponto_do_ambiente_segue_a_regra_do_ponto_do_link` (escrito e verde antes da troca). O comentário deixou de falar do `Bilhete`. |
| C4 | `56b4d02` | `NaoSaiu { motivo, passageira }`: `mandar_pelo_server` e `registrar` o devolvem, `perguntar_pelo_server` e `furar` compilam com ele (`Display`). `RegistroNoQuarto::anotar` não muda de estado numa falha passageira (fica em `debug`). `a_falha_que_conta` prefere a falha que não é passageira, para uma fila cheia num socket não esconder a recusa do outro no mesmo tique. |
| T3d, T3e | `d564d1d` | O literal `let fp = "3cbcfb02…"` usa `IMPRESSAO`; o bloco do alvo do furo virou `alvo_do_furo()`. |
| T7b | `07bd5c3` | Adendo no ADR 0047, no molde do ADR 0003: `onde_mora` virou `onde_mora_hoje` (`f390e98` e `d2ac298`, 29/09), onde ela mora hoje por âncora de texto, e o nome novo do teste que §2.1 cita. A linha `:83` não mudou. |
| os planos | `9ddd404` | Nota no topo de cada seção citada: no 1A, Global Constraints (o G1 de hoje, o G2 decidido), Task 4 (`AmbienteDoEncontro`, `e8db220`), Task 5 (G1, T5d), Task 9 (executada em `a6c2cc0`) e Riscos (G1); no 1E, Task 1 (`PUBLICADAS_ANTES_DO_GUARDA` e o `$tmp` viraram `fn linhas_que_faltam`, `28a9532`/`80f94e0`, com `git checkout -- api/congeladas.sha256`). |

O T5f ficou de fora, como o brief manda.

## Evidência de TDD: RED e GREEN

Onde o comportamento era novo, o teste veio antes e ficou vermelho pelo motivo
certo. Onde o brief pedia um guarda para o que já existia (T5a, T5c, C1, T2c,
T3c, M3, T5e), o vermelho vem da reversão (seção seguinte).

- **M2 do 19.** A escolha foi escrita primeiro com a de hoje (`SemSocket | NadaSaiu → Devolver(PontoMudo)`).
  `cargo test -p seele-core --lib encontro` →
  `a_consulta_passa_ao_endereco_seguinte_so_quando_nada_saiu_pelo_primeiro`: `left: Devolver(PontoMudo) right: TentarOSeguinte`;
  `um_endereco_sem_rota_nao_esconde_o_seguinte_do_mesmo_ponto`: `left: PontoMudo right: Achado { servidor: Some(203.0.113.7:9621), escuta: Some(203.0.113.7:51000) }`.
  GREEN: 23 passaram.
- **T6f.** `depois_da_volta` escrita primeiro com a semântica de hoje (`(0,0) → NadaSaiu`, `(0,_) → Parar`).
  `uma_volta_recusada_depois_da_primeira_conta_como_volta`: «a volta 2 não saiu, depois de 1 que saíram, e a consulta parou…» `left: Parar right: Contar`;
  `uma_volta_recusada_depois_da_primeira_ainda_ouve_a_resposta_tardia`: «a volta 2 não saiu e a consulta perdeu a resposta tardia à volta 1…» `left: Perguntou(PontoMudo) right: Perguntou(Achado { servidor: Some(203.0.113.7:9621), escuta: None })`.
  GREEN: 25, e 26 com o guarda da ida e volta. Os testes de tempo do `seele-core` ficaram verdes em cinco rodadas seguidas.
- **T5d.** `cargo test -p seele-proto --lib uri::tests::um_ponto_vazio` → «um ponto vazio virou ponto de encontro…» `left: Ok(Alvo { maquina: "", porta: 8384 }) right: Err(EnderecoInvalido)`. GREEN: `seele-proto` inteiro.
- **T1a.** `cargo test -p seele-proto --lib encontro::` → «um texto com algo que não é letra nem número depois do 16º caractere deu marcas…» `left: Some(Marcas { aviso: Marca("0123456789abcdef"), … }) right: None`. GREEN.
- **1B daqui.** Primeiro a lacuna, medida: com `Batida::preparar` usando `.escuta()` e o contador antigo, `cargo test -p seele-conformance --test volta_pela_trilha` → `ok. 6 passed` (verde com o defeito). Com o contador novo e o defeito ainda no lugar: «nenhum `LEVE` com a marca do aviso (a9087129f2d5da54) chegou ao ponto de encontro…» `FAILED`. GREEN com o `seele-core` restaurado.
- **C4.** Primeiro a troca do tipo sem mudar comportamento (109 verdes). Depois:
  `uma_fila_cheia_num_tique_nao_vira_linha_no_log`: «a fila de saída cheia num tique, seguida de um tique bom, deixou linhas no log…» `left: (1, 1) right: (0, 0)` (as duas linhas `info` de hoje);
  `uma_fila_cheia_num_socket_nao_esconde_a_recusa_do_outro`: `left: Err(… passageira: true) right: Err(… passageira: false)`.
  GREEN: 111.

## Cada reversão, e o vermelho que ela deu

Cada uma foi desfeita copiando o arquivo de volta e conferindo com `cmp` (e
`git status` limpo).

- **T5a** (sem o `sort_by_key`): «o AAAA que o DNS deu primeiro ficou na frente…» `left: [[::1]:8384, 127.0.0.1:8384]`.
- **T5c** (sem a linha da lista vazia): «o nome que resolveu para nenhum endereço não disse no rastro… Rastro: []».
- **M2, fiação** (`consultar` só com o primeiro, `.take(1)`): «nenhuma pergunta saiu pelo primeiro endereço do ponto, e a consulta não chegou ao segundo…» `left: PontoMudo`. A reversão da escolha é o RED acima.
- **T6f** (`if saidas == 0 → NadaSaiu`, sem olhar `enviados`): os três testes do T6f vermelhos (`left: NadaSaiu`). E (`saiu_em = agora` em toda volta): `a_ida_e_volta_se_mede_da_ultima_volta_que_saiu` vermelho em 3 de 3 rodadas, «a escuta respondeu antes da hora da volta seguinte e se perdeu…» `left: … escuta: None`.
- **C1** (sem `.max(FOLGA_MINIMA)`): «com 1 ms de ida e volta o teto não ficou no piso de 100 ms…» e «a escuta respondeu 50 ms depois do servidor, dentro do teto de 100 ms, e ficou de fora…». (Sem o dobro): «com 200 ms de ida e volta o teto não ficou no dobro…». (Teto inventado sem `ONDE`, `.zip(ida_e_volta.or(Some(ZERO)))`): «o `ONDE` se perdeu, e a escuta que respondeu aos 300 ms… ficou de fora: sem ida e volta medida, a consulta inventou um teto». (Repetir ao ponto no ar): «sem o `ONDE`, a espera… devia ir até a hora da volta seguinte (500ms) e parar ali, e levou 1.502654917s».
- **T5d** (sem o `is_empty` de `separar_ponto`): o mesmo vermelho do RED. (`Bilhete::novo` de volta a `validar_alvo`): «um bilhete de ponto vazio foi montado: `Bilhete::ponto` falha para um bilhete que veio de `Bilhete::novo`, contra o doc dele».
- **T1b** (`..15`): «a marca do aviso mudou: todo cliente 0.15.0 passa a bater com uma marca que o anfitrião ignora» `left: "3cbcfb0212da738"`. (`..17`): a mesma frase, `left: "3cbcfb0212da738f8"`.
- **T1c** (campos `pub` de novo): `cargo test -p seele-proto --doc` → «Test compiled successfully, but it's marked `compile_fail`» `FAILED`. (Escuta com a marca do aviso): «com a impressão 3cbcfb02…de5b9, a escuta se registra com a marca do aviso…».
- **T2c/T3c/M3.** (`ONDE` da subida com a marca do aviso): «o ONDE da subida… saiu com a marca 3cbcfb0212da738f, e não com 3cbcfb0212da738fe…». (`LEVE` da subida com a do aviso): «o LEVE do socket do servidor… saiu com a marca 3cbcfb0212da738f…». (`LEVE` do tique com a do aviso): a mesma, e `o_eco_do_proprio_registro_nao_vira_furo_contra_si_mesmo` também. (`MORO` do servidor com a da escuta): «o MORO do socket do servidor saiu com a marca …fe, e não com …fs…», e `o_primeiro_registro_no_quarto_sai_na_subida` também. **T2c, a reversão própria** (o `MORO` do servidor não sai): `o_primeiro_registro_no_quarto_sai_na_subida` → «o socket do servidor não se registrou no quarto com a marca dele (3cbcfb0212da738fs) no primeiro segundo…».
- **T4a** (o `Drop` de `Encontro` sem `abort`): «a porta [::]:55840 não voltou cinco segundos depois de a hospedagem ser largada (47 tentativas). A última tentativa deu: não deu para ligar em 0.0.0.0:55840: Address already in use (os error 48)…».
- **T7a** (`no_ar = true` antes do `ler_aqui`): «um ponto que responde lixo virou NinguemMora…».
- **T8b**, lado do anfitrião (`MORO` da escuta com a marca do aviso): «o anfitrião não deixou a escuta no quarto sob a marca dela (5e1e0a0b0c0d0e0fe)… O conserto é no anfitrião (o `registrar` do `seele-server`), e não na FFI». Lado da consulta (`quem(servidor)` no lugar de `quem(escuta)`, no `seele-core`): «a FFI respondeu Achado { servidor: Some(…), escuta: None }, e o quarto tem os dois (perguntado acima sem ela)… é a FFI, ou a consulta do `seele-core` por baixo dela, perguntando por outra marca».
- **T8d** (o `enc=` escrito sem a porta do ponto, em `uri.rs`): o `assert_ne` das duas formas ficou vermelho. O teste antigo, que usava `bilhete.ponto`, não via esse defeito.
- **1B daqui**: o RED acima é a reversão (marca do `LEVE` trocada em `Batida::preparar`).
- **T5e** (`separar`, a regra do servidor): «o ponto escrito sem porta não foi procurado na porta do ponto de encontro…» `left: Ok([127.0.0.1:8383]) right: Ok([127.0.0.1:8384])`.
- **C4** (classificar por `Interrupted` em vez de `WouldBlock`): os dois testes do C4 vermelhos.

## Arquivos mudados

- `crates/seele-core/src/encontro.rs`
- `crates/seele-proto/src/uri.rs`
- `crates/seele-proto/src/encontro.rs`
- `crates/seele-proto/tests/sementes_do_fuzz.rs` (só o trecho `datagrama_do_encontro`)
- `crates/seele-server/src/alcance/encontro.rs`
- `crates/seele-conformance/tests/quarto.rs`, `quarto_pelo_link.rs`, `ocupante_da_escuta.rs`, `volta_pela_trilha.rs`
- `docs/adr/0047-o-link-que-volta-a-funcionar-amanha.md`
- `docs/superpowers/plans/2026-09-29-1.0-plano-1a-o-quarto.md`, `docs/superpowers/plans/2026-09-29-1.0-plano-1e-a-api-congelada.md`
- este relatório

## Os testes, como rodaram no fim

- `cargo test -p seele-core --lib encontro`: 29 ok.
- `cargo test -p seele-proto`: 259 + 5 + 1 + 2 (doc) ok.
- `cargo test -p seele-server --lib alcance`: 111 ok.
- `cargo test -p seele-conformance --test quarto --test quarto_pelo_link --test ocupante_da_escuta --test volta_pela_trilha`: 9, 4, 2 e 6 ok. A 8384 foi conferida livre com `lsof -nP -iUDP:8384` (saída vazia) antes de cada rodada do `quarto_pelo_link`, e nunca esteve ocupada. Ele mede nesta máquina: sem `PULADO`, «a porta [::]:57150 voltou em 2 tentativa(s)».
- `cargo build -p seele-encontro --examples`: compila.
- `cargo fmt --all -- --check` limpo, e o clippy de `seele-proto`, `seele-core`, `seele-server`, `seele-ffi`, `seele-conformance` e `seele-encontro` com `--all-targets --all-features -- -D warnings` limpo antes de cada commit.

## O que não rodou aqui, e por quê

- A bateria inteira e o clippy do workspace: ficam para a integração, como o brief manda.
- Os testes do `seele-ffi`: o lote não mexe nele. Ele compila sob o clippy, só chama `Marcas::do_servidor`, e o teste dele de `SemMarca` usa `"curta"`, que continua dando `None` com o T1a.
- `apps/seele-app` não foi compilado (Tauri). O traço `Canal` mudou a forma do futuro de `onde_mora_hoje`; ele continua `Send`, o que o `tokio::spawn(onde_mora_hoje(…))` de `quarto.rs` prova ao compilar.
- `cargo xtask check-api` e as bancadas: o lote não toca `api/` nem `apps/seele-app/ui`.
- Código com `cfg(windows)` ou `cfg(target_os = "linux")`: nenhum foi escrito. A medida do ponto vazio do T5d é do macOS; Linux e Windows não foram medidos.

## Preocupações

1. **O traço `Canal`** (privado, no `seele-core`) existe só para o teste do T6f: nenhum socket de verdade aceita um envio na volta 1 e recusa o da volta 2 do mesmo jeito nos três sistemas. É uma costura de teste no código de produto, pequena e documentada; a revisão decide se ela vale.
2. **Além da letra do brief, com guarda próprio:** no T6f, `saiu_em` só anda quando algo saiu (sem isso, a resposta tardia à volta 1 media a ida e volta da volta recusada, e o teto vencia cedo); no C4, `a_falha_que_conta` prefere a falha não passageira (com a primeira valendo, uma fila cheia no `LEVE` escondia a recusa do `MORO` do servidor naquele tique); no M2, uma linha `info` nova diz quando a consulta passa ao endereço seguinte.
3. **O doc de `onde_mora_hoje` ficou atrás do código**, como o brief manda (o M2, P51-18, o reescreve): ele diz que «nenhuma pergunta sai… volta na hora, como `PontoMudo`», e agora a consulta tenta antes o endereço seguinte do ponto, e uma volta recusada depois da primeira espera a resposta tardia.
4. **A pendência supunha que o ponto vazio resolvia para o loopback.** Medido neste Mac, o `getaddrinfo` do Rust recusa o nome vazio («nodename nor servname provided»): o defeito real era o diagnóstico errado («não resolveu» no lugar de «não é um endereço»). O comentário do teste diz o que foi medido.
5. **Testes de tempo novos no `seele-core`** (T6f e C1): margens de 50 a 100 ms, verdes em cinco rodadas seguidas com outros lotes compilando nesta máquina. Um executor de CI muito mais lento pode apertá-las.
6. **O T1c muda um item público do `seele-proto`** (campos de `Marcas` viram métodos). Todo leitor do workspace foi trocado; o app não lê os campos.
7. Nenhuma promessa nova de API, então não há seção «Para a v6». Nenhum lote ganha cadeia (G1).
