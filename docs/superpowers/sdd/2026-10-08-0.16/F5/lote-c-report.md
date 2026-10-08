# Lote C da F5: relatório

Base `fbeff8e` (branch `0.16/lotes`). Branch `0.16/lotes-c`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-ae3bae8965103a010`. Quatro
commits de código e este relatório, todos locais: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `0844129b` | P51-05 | a frase da janela vai num campo `frase = ?…`, depois do `onde` e do `mod_id`, cortada por `executor::cortar_linha_do_console` com o aviso do corte |
| `37a5d902` | P51-08 | um balde de vazão por MOD (o `BaldeDoConsole`) para a janela, a mídia recusada, a servida e o som servido, num mapa de até 64 entradas com transbordo; `revogar` diz a conta pendente |
| `9f90888b` | P51-10 e a metade do app do P51-26 | as duas linhas de `mod_request` seguem os três braços da metade servidor; `midia_em_bytes` e `escolher_para_o_mod` escrevem `mod_id = %id_no_registro(&id)` |
| `53587a1f` | P51-12 | a recusa de carga é guardada com a geração, e dita quando a geração é outra |

Arquivos mudados (`fbeff8e..53587a1f`), só os três da lista do lote:
- `apps/seele-app/src/main.rs`;
- `apps/seele-app/src/executor.rs` (docs do corte, visibilidade de `cortar_linha_do_console` e de `LinhaCortada`, e um leitor `LinhaCortada::texto`; ver «Preocupações»);
- `docs/superpowers/plans/2026-09-29-1.0-plano-1d-mods-na-casca.md` (uma linha, a :3876).

Nada no fio (`seele-proto`), no SEELE-ENC/1, nos arquivos da F6, em `api/`, na
CSP, em `ui/` ou nas bancadas. Nenhuma dependência nova, nada baixado.
`docs/pendencias.md` e o índice da 1.0 não foram tocados.

Os vermelhos, verdes e reversões abaixo foram guardados na íntegra durante o
trabalho; aqui vai o trecho que diz o motivo. As reversões foram feitas por um
script que troca o trecho exato, roda os testes, desfaz a troca e compara o
`main.rs` byte a byte (`cmp`) com a cópia tirada antes: todas voltaram
«restaurado byte a byte».

## 1. P51-05: a frase da janela vai num campo (`0844129b`)

**Produto.**
- Os três braços de `registrar_da_janela` passam de
  `tracing::warn!(onde = %onde, mod_id, "{o_que}")` a
  `tracing::warn!(onde = %onde, mod_id, frase = ?frase, "a janela disse")`, e o
  mesmo com `error!` e `debug!`. A linha fica
  `WARN seele_app: a janela disse onde=… mod_id=… frase="…"`.
- O `.take(TETO_DA_FRASE_NO_REGISTRO)` saiu. A frase tem o caractere que quebra
  ou inverte a linha trocado por espaço (a segunda defesa, que fica) e é
  cortada por `executor::cortar_linha_do_console`, pelo tamanho já escapado e
  com o aviso `[…cortado: N caracteres]`. A regra não foi reescrita em
  `main.rs`.
- `executor.rs`: `cortar_linha_do_console` e `LinhaCortada` passam a
  `pub(crate)`; o campo de `LinhaCortada` continua privado, e quem está fora do
  executor só o lê por `LinhaCortada::texto()`.
- Docs: `TETO_DA_FRASE_NO_REGISTRO` deixa de dizer que a janela escreve sem
  escape, e conta quatro portas; `quebra_ou_inverte_a_linha` tira «a frase» dos
  campos crus e diz que a troca por espaço é a segunda defesa; em `executor.rs`,
  `cortar_linha_do_console` e `ate_o_teto_do_registro` contam a janela como mais
  uma porta.
- Plano 1D, só a linha «Esperado: no caso A…» (:3876): a recusa sem gesto é
  descrita como `a janela disse` com `onde=recusa-de-mod` e a frase em
  `frase="…não tocou: …"`. Os greps de :157 e :3874 procuram `recusa-de-mod`,
  que continua na linha.

**Testes** (em `mod o_registro_da_janela` e no módulo da mídia recusada):
- novo `um_mod_id_forjado_na_frase_nao_vem_antes_do_verdadeiro`: com
  `o_que = "x onde=forjado mod_id=vitima/mod"` e `mod_id = Some("a/b")`, o
  primeiro `mod_id=` da linha é `a/b`, o primeiro `onde=` é `atender-mod`, e a
  frase sai em `frase="x onde=forjado mod_id=vitima/mod"`;
- `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas` ganha a porta
  `janela`, com `campo(…, "frase")` e o aviso tirado por
  `strip_suffix(" […cortado: 2048 caracteres]")`, como o do `console`;
- `o_aviso_da_janela_sai_como_warn_com_o_id_do_mod_em_campo_proprio` passa a
  exigir a frase inteira em `frase="…"`;
- `o_id_do_mod_sai_igual_pelas_duas_portas_do_registro` continua verde.

**RED** (`cargo test -p seele-app --bin seele-app -- o_registro_da_janela o_pior_caso… o_id_do_mod_sai_igual…`, contra o produto de `fbeff8e`): 3 falhas, 6 verdes.
- forja: `o primeiro mod_id= da linha da janela é o que um MOD escreveu na frase… WARN seele_app: x onde=forjado mod_id=vitima/mod onde=atender-mod mod_id=a/b`, `left: "vitima/mod"`, `right: "a/b"`. Era o esperado: a frase era a mensagem, e a mensagem vem antes dos campos.
- aviso: `a frase da janela não saiu inteira no campo frase="…"`, com a linha `WARN seele_app: seele/perfis: «contribuir» falhou — … onde=atender-mod mod_id=seele/perfis`.
- pior caso: `a linha não traz frase="…"`, e a linha mostra 512 `U+100000` crus como mensagem.

**GREEN:** `o_registro_da_janela`, `a_midia_recusada_e_dita_no_registro` e
`executor`: 64 passaram, 0 falharam.

**Reversões.**

| mutação | vermelho |
|---|---|
| os três braços voltam a `"{frase}"` como mensagem | `um_mod_id_forjado_na_frase_nao_vem_antes_do_verdadeiro` (`left: "vitima/mod"`) e `o_aviso_da_janela…` (`a frase da janela não saiu inteira no campo frase="…"`) |
| a frase volta ao `.take(TETO_DA_FRASE_NO_REGISTRO)`, sem `cortar_linha_do_console` | `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas`: `a frase da janela não diz que foi cortada, e quem lê o registro toma o pedaço pelo todo: \u{100000}\u{100000}…` |

## 2. P51-08: um teto de vazão por MOD (`37a5d902`)

**Produto.**
- `struct Session` ganha `baldes_do_registro: BaldesDoRegistro`, com
  `type BaldesDoRegistro = Mutex<HashMap<String, executor::BaldeDoConsole>>`.
- A chave é `chave_do_balde(id)`: `id_no_registro(id)`, ou a chave fixa
  `BALDE_DO_CATALOGO` (`"\u{0}catalogo"`) quando não sobra nada — a linha sem id
  e a do id vazio. O `\u{0}` na frente garante que nenhum id filtrado por
  `id_no_registro` cai nela.
- `BALDES_NO_REGISTRO = 64`, contando o transbordo: 63 ids com balde próprio,
  e todo id novo que chega com o mapa cheio gasta do balde dividido
  `BALDE_DE_TRANSBORDO`. O mapa nunca passa de 64 entradas.
- `linha_admitida(&baldes, agora, mod_id) -> Option<u32>` é o único ponto que
  consulta o balde. Um cadeado envenenado é lido como está
  (`PoisonError::into_inner`), e não tira o teto.
- `registrar_da_janela_em(&baldes, agora, nivel, onde, o_que, mod_id)` é o corpo
  do comando; o comando `registrar_da_janela` recebe `State<Session>` e passa
  `Instant::now()`. Os argumentos do `invoke` (`nivel`, `onde`, `oQue`,
  `modId`) não mudaram.
- `recusa_de_midia_dita`, `midia_servida_dita` e `som_servido_dito` recebem
  `&baldes` e `agora`. A assinatura se propaga por `midia_declarada_do_mod`,
  `som_declarado_do_mod`, `midia_do_servidor_dita` e `ler_imagem_mod`, e pelos
  testes que os chamam. Os comandos passam `&session.baldes_do_registro` e um
  `Instant::now()` lido em cada chamada (em `ler_imagem_mod`, depois do
  `await`, dentro do `map_err`).
- O campo `suprimidas` segue o molde de `registrar_console_do_mod`: só aparece
  quando não é zero.
- `Session::revogar`, antes de esvaziar o mapa, diz uma linha por balde com
  conta pendente (`BaldeDoConsole::pendentes`):
  `WARN … linhas do registro seguradas pelo teto de vazão, sem outra linha depois que as contasse mod_id=… suprimidas=N`
  (ou `catalogo=true`, ou `transbordo=true`).
- `BaldeDoConsole` não mudou.

**Testes** (módulo novo `o_teto_de_vazao_do_registro`, relógio fixo; os de
`o_registro_da_janela` passam a chamar `registrar_da_janela_em`):
- `duzentas_linhas_de_um_mod_no_mesmo_instante_deixam_so_a_rajada`: 32 linhas;
- `a_linha_depois_do_aperto_diz_quantas_ficaram_para_tras`: 250 ms depois, a
  linha seguinte traz `suprimidas=168`, e nenhuma da rajada traz `suprimidas`;
- `um_mod_nao_segura_as_linhas_de_outro`: `c/d` passa depois das 200 de `a/b`;
- `com_o_mapa_cheio_um_id_novo_passa_pelo_transbordo`: o mapa fica em 64; um
  id novo deixa entre 1 e 32 linhas em 200; o id novo seguinte e mais 64 ids
  novos não deixam nenhuma (o transbordo é um só);
- `revogar_diz_a_conta_que_nenhuma_linha_levou`: 40 linhas de `a/b` e 40 sem
  id; `revogar` diz `mod_id=a/b … suprimidas=8` e `catalogo=true … suprimidas=8`,
  e o mapa fica vazio;
- `a_recusa_de_midia_tem_o_mesmo_teto_e_a_mesma_conta`: 33 linhas, a última
  com `suprimidas=168`;
- `as_portas_do_mesmo_mod_dividem_o_balde` (não estava na lista): 9 rodadas das
  quatro portas do mesmo MOD deixam 32 linhas, e não 36.

**RED** (o encanamento inteiro, com `linha_admitida` sem o `admitir`, que
devolvia sempre `Some(0)`): 6 falhas, 30 verdes. Era o esperado: hoje saem 200
linhas.
- `duzentas_linhas…`: `left: 200`, `right: 32`;
- `a_linha_depois_do_aperto…` e `a_recusa_de_midia…`: `left: 201`, `right: 33`;
- `as_portas…`: `left: 36`, `right: 32`;
- `revogar…`: `left: 0`, `right: 2`;
- `com_o_mapa_cheio…`: `com o mapa cheio, um id novo deixou 200 linhas em duzentas`.

**Um teste meu estava errado, e foi consertado antes do commit.** A primeira
versão de `com_o_mapa_cheio…` enchia o mapa com 128 ids: os 65 depois do
cheio secavam o transbordo antes do id medido, e com o `admitir` no lugar o
teste caiu (`deixou 0 linhas em duzentas`). Ele passou a encher o mapa até
ficar cheio, e não além. O vermelho da versão consertada é o da reversão (a),
abaixo.

**GREEN:** os quatro módulos do registro (`o_teto_de_vazao_do_registro`,
`o_registro_da_janela`, `a_midia_recusada_e_dita_no_registro`,
`o_som_do_mod_sai_em_bytes`): 36 passaram, 0 falharam.

**Reversões.**

| mutação | vermelho |
|---|---|
| (a) tirar o `admitir` (`Some(0)` sempre) | os 6 acima, com as mesmas contas; o do transbordo: `com o mapa cheio, um id novo deixou 200 linhas em duzentas` |
| (b) tirar o teto do mapa (todo id novo ganha balde) | `com_o_mapa_cheio…`: `o mapa dos baldes passou de 64 entradas` |
| (c) tirar a linha da conta do `revogar` | `revogar_diz_a_conta…`: `a sessão fechou com linhas seguradas em dois baldes e não disse uma conta por balde` |

## 3. P51-10, com a metade do app do P51-26: o id sempre escapado (`9f90888b`)

**Antes de escrever o doc, medi o que ele afirma.** No `tracing-subscriber`
0.3.23 do `Cargo.lock`, `fmt/format/mod.rs` só passa o campo `message` pelo
`EscapeGuard`, e o `EscapeGuard` (`escape.rs`) só escapa ESC, BEL, BS, FF, DEL
e C1. Os outros campos saem por `{:?}` do valor, e o `Debug` de um `%x` é o
`Display` dele: cru.

**Produto.**
- As duas linhas de `mod_request` saem por `pedido_a_caminho_dito(id, geracao,
  request, channel)` (DEBUG) e `pedido_recusado_na_saida_dito(id, request,
  erro)` (WARN). As duas passam pela mesma escolha, `IdDoPedido::de(id)`, com os
  três braços de `executar` (`crates/seele-server/src/mods/pedidos.rs`): id
  vazio é `catalogo = true`, sem `mod_id=`; id que o filtro esvazia (ou deixa
  só espaço, o `trim` do servidor) é `mod_id_ilegivel = true`; o resto é
  `mod_id = %legivel`, já por `id_no_registro`.
- `midia_em_bytes` («mídia de MOD vinda do servidor») e `escolher_para_o_mod`
  («arquivo escolhido para um MOD») passam a `mod_id = %id_no_registro(&id)`.
  Os outros `mod_id = %…` ficaram como estavam, como pede o brief.
- O parágrafo «O escape é o que importa…» de `motivo_no_registro` passa a
  dizer que o formatador escapa só a mensagem, e só ANSI, que um campo em `%`
  sai cru, e que quem protege a linha são as aspas e o escape da função.
- O doc de `id_no_registro` lista as portas por nome, agora com
  `mod_request`, `midia_em_bytes` e `escolher_para_o_mod`.

**Para o vermelho de verdade**, as duas funções foram extraídas primeiro com o
comportamento de hoje (`mod_id = %id` cru), e os testes rodaram contra elas.

**Testes** (módulo novo `o_id_do_pedido_de_mod_no_registro`, cada caso pelas
duas funções):
- `uma_quebra_de_linha_no_id_nao_forja_outra_linha`: com
  `id = "a/b\nWARN seele_app: forjada"`, sai uma linha só;
- `o_id_vazio_e_o_catalogo_e_nao_um_mod_id_em_branco`: `catalogo=true`, sem
  `mod_id`;
- `um_id_que_o_filtro_esvazia_e_ilegivel` (não estava na lista; é o terceiro
  braço): `"\n\u{202E} "` dá `mod_id_ilegivel=true`, sem `mod_id=`;
- `um_id_de_verdade_sai_como_mod_id`: `mod_id=seele/perfis` (verde antes e
  depois, como não-regressão);
- `nenhum_mod_id_em_display_usa_o_id_cru_do_comando`, o guarda de fonte: em
  `mod_request`, nas duas funções dele, em `midia_em_bytes` e em
  `escolher_para_o_mod`, nenhum `mod_id=%` (sem espaço, sem comentário) é
  seguido do identificador `id`; e `mod_request` chama as duas funções.

**RED** (`cargo test -p seele-app --bin seele-app -- o_id_do_pedido_de_mod_no_registro`): 4 falhas, 1 verde.
- forja: `uma segunda linha no seele.log com a cara do produto: DEBUG … mod_id=a/b` / `WARN seele_app: forjada geracao=7 request=3 channel=1`;
- catálogo: `… pedido de MOD a caminho do servidor mod_id= geracao=7 …`;
- ilegível: `mod_id=` seguido da quebra de linha e do U+202E, crus;
- guarda: `` `fn pedido_a_caminho_dito(` escreve `mod_id = %id` com o id cru ``, `left: "id"`.

**GREEN:** `o_id_do_pedido_de_mod_no_registro` e
`a_midia_recusada_e_dita_no_registro`: 20 passaram, 0 falharam.

**Reversões.**

| mutação | vermelho |
|---|---|
| (a) os braços legíveis das duas funções voltam a `mod_id = %id` | a forja (`uma segunda linha…`) e o guarda (`fn pedido_a_caminho_dito(` … `left: "id"`) |
| (b) `midia_em_bytes` volta a `mod_id = %id` | o guarda: `` `fn midia_em_bytes(` escreve `mod_id = %id` `` |
| (c) `escolher_para_o_mod` volta a `mod_id = %id` | o guarda: `` `async fn escolher_para_o_mod(` escreve `mod_id = %id` `` |
| (d) os três braços colapsam (catálogo e ilegível caem no `mod_id=`) | catálogo (`mod_id= geracao=7`) e ilegível (`mod_id=  geracao=7`) |

## 4. P51-12: a recusa de carga por geração (`53587a1f`)

**Produto.**
- `type RecusasDeCarga = Mutex<HashMap<String, (u64, String)>>`.
- `recusa_de_carga_dita(ditas, geracao, mod_id, falha)` e
  `carga_dita(ditas, geracao, mod_id, carga)`. `codigo_do_mod` e
  `mod_nativo_reservar` passam a `geracao` que já tinham.
- A recusa é dita quando a entrada não existe, quando a geração é outra ou
  quando o motivo mudou: compara-se o par `(geracao, motivo)` guardado.
  `sessao-encerrada` continua em DEBUG e sem mexer na memória. `carga_aceita`
  não mudou.
- Saiu do doc a ressalva de «**E a memória é de uma sessão.**» até «…com a
  linha dita segundos antes no arquivo.». O parágrafo de cima passa a dizer
  que o motivo é guardado com a geração, e que sai a primeira recusa de cada
  sessão. O doc do tipo e a primeira linha do doc da função dizem a geração.
- As strings exatas de `os_dois_comandos_de_carga_passam_pela_recusa_dita`
  passam a ter `geracao`; o ajudante `recusar` escreve o tipo novo por extenso
  e recebe a geração; as chamadas diretas do módulo passam a geração 1 (e 1 e
  2 em `a_mesma_recusa_numa_sessao_nova_e_dita_de_novo`, uma de cada lado do
  `revogar`).

**Teste novo**, ao lado de `a_mesma_recusa_numa_sessao_nova_e_dita_de_novo`:
`a_recusa_atrasada_da_sessao_que_acabou_nao_cala_a_da_seguinte`. Com a sessão
1 de pé, `revogar()` (que devolve 2), depois a recusa atrasada da geração 1,
depois a mesma recusa na geração 2: têm de sair duas linhas WARN.

**RED**, como o brief pede: primeiro a assinatura nova comparando só o motivo
(o comportamento de hoje). `cargo test -p seele-app --bin seele-app recusa`:
1 falha, 32 verdes — `a recusa atrasada da sessão que acabou calou a mesma
recusa na sessão seguinte…`, `left: 1`, `right: 2`. O guarda dos dois comandos
já estava verde nesse passo, porque as chamadas já tinham a geração.

**GREEN:** `recusa`: 33 passaram, 0 falharam.

**Reversões.**

| mutação | vermelho |
|---|---|
| (a) comparar só o motivo (`.map(\|(_, motivo)\| motivo)` contra `dita.1`) | `a_recusa_atrasada…`: `left: 1`, `right: 2` |
| (b) `codigo_do_mod` passa a geração fixa `0` em vez da dele | `os_dois_comandos_de_carga_passam_pela_recusa_dita`: `` `fn codigo_do_mod(` deixou de ser `ler_codigo_do_mod(..)` com o `map_err` de `recusa_de_carga_dita`… `` |

## O que rodou

- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --bin seele-app`,
  inteiro, antes do último commit de código: **202 passaram, 0 falharam**
  (12,4 s).
- Os filtros do brief, `o_registro_da_janela` e `recusa`, verdes dentro dela e
  separados (acima).
- `cargo fmt --all -- --check` limpo antes de cada commit.
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`
  limpo antes de cada commit (com `touch` no `main.rs` para forçar a
  conferência). O único aviso é o `ld: duplicate -rpath` do linker, ambiental.
- `CARGO_BUILD_JOBS=8` em todos.

## O que não rodou aqui, e por quê

- `cargo test --workspace` e a bateria inteira: proibidos no lote; rodam na
  integração, numa fila só.
- O clippy de outros crates: só o `seele-app` mudou, e ele é um binário — nada
  depende dele.
- `cargo xtask check-api` e o guarda de congelamento: `api/` não foi tocado.
- `cargo xtask check-runtime` e as bancadas de navegador: nada em `ui/` nem em
  `bancada/` mudou. As bancadas que olham `registrar_da_janela`
  (`contribuicoes-e-camadas.cjs`, `diagnostico-de-mods.cjs`,
  `regiao-do-mod.cjs`) conferem os argumentos do `invoke`, que não mudaram.
- Nada de `cfg(windows)` ou `cfg(target_os = "linux")` foi tocado.
- O app de verdade não foi aberto: a forma nova das linhas foi conferida pelo
  formatador do `tracing` nos testes (`rastro_de_teste::capturar`), e não num
  `seele.log` gravado pelo app.

## Preocupações

1. **`LinhaCortada::texto()` é um pouco mais que visibilidade.** O brief deixa
   em `executor.rs` «só os docs do corte e, se for preciso, a visibilidade
   dele». Abrir o campo (`pub(crate) String`) deixaria qualquer parte do crate
   construir uma `LinhaCortada` sem passar pelo corte, contra o doc do tipo
   («Só `cortar_linha_do_console` a faz»). Preferi um leitor de três linhas, que
   mantém a garantia. Se a revisão preferir o campo aberto, é uma troca de uma
   linha.
2. **«Não usar `Instant::now()` dentro dos testes», ao pé da letra, não tem
   como.** `Instant` não tem outro construtor. O relógio fixo é
   `relogio_de_teste::origem()`: um `Instant::now()` lido uma vez no processo,
   num `OnceLock`, e todo instante de teste é essa origem mais uma `Duration`
   escrita no teste. Nenhum teste lê o relógio da máquina como relógio, e
   nenhum depende de quanto tempo a execução leva.
3. **A conta do transbordo pode ser de mais de um id.** Uma linha que passa
   pelo balde dividido leva em `suprimidas` as linhas seguradas de todos os ids
   que caíram nele. Está escrito no doc de `linha_admitida`. Só acontece com
   mais de 63 ids diferentes numa sessão, o que só uma janela que inventa ids
   faz. *(Rodada 1: a linha que passa pelo transbordo agora diz
   `transbordo=true`.)*
4. **A conta pendente só é dita quando a sessão fecha** (`revogar`, que roda
   ao conectar e ao desmontar). Fechar o app com a sessão de pé
   (`ExitRequested` → `despedir_se`) não passa por `revogar`, e a última conta
   pendente, se houver, cai com o processo. É a mesma regra da memória das
   recusas de carga, e mexer no caminho de saída estava fora do lote.
   *(Rodada 1: a conta saiu para `Session::dizer_as_contas_do_registro`, que o
   lote F, dono de `despedir_se`, vai chamar. Até lá, a saída do app ainda
   perde a última conta.)*
5. ~~**A mídia servida (INFO) gasta do mesmo balde que os avisos do MOD.** …
   um aviso logo depois espera a próxima ficha (250 ms) e sai com a conta. Nada
   some; o aviso só chega um pouco depois, contado.~~ **Isto estava errado, e
   a revisão achou.** Uma linha segurada não espera ficha nenhuma: quando
   `linha_admitida` devolve `None`, a porta dá `return`, a linha não é
   escrita, e só a conta segue, na próxima linha do mesmo MOD — que podia ser
   outra INFO de «mídia de MOD servida» — ou no `revogar`. Com um balde só por
   MOD, um MOD que montasse mais de 32 mídias de uma vez fazia a recusa da
   35ª sumir: o defeito de 23/09. Consertado na rodada 1 (Importante 1), com
   um balde por MOD e por classe.
6. **A forma da linha da janela mudou no `seele.log`.** A frase agora vai entre
   aspas e escapada: um `grep` por uma frase que tenha `"` ou `\` precisa do
   escape. Os greps do guia (`mod_id=…`) e do Plano 1D (`recusa-de-mod`, as
   mensagens do Rust) continuam casando. Os testes do lote D sobre
   `registrar_da_janela_em` devem procurar campos (`frase=`, `mod_id=`,
   `suprimidas=`), e não a linha inteira.
7. **`IdDoPedido` repete a regra do servidor**, como `id_seguro` do servidor já
   repete a do app: um enum de três braços em cada metade. Mudar a regra numa
   metade e não na outra não reprova nada hoje; quem fizer uma deve fazer a
   outra.

## Para a v6

Nada. O lote não cria evento, campo do retrato nem frase no `api/README.md`; o
que mudou é texto do `seele.log`, que não é promessa da API.

## Rodada 1 de conserto

A revisão deu «precisa de conserto», com nove achados: um Importante, sete
menores e uma nota para o guia que não é defeito do lote. Mesmo
worktree, mesmo branch (`0.16/lotes-c`), a partir de `c7431ae7`. Seis commits
de código e este relatório.

| commit | item | o que muda |
|---|---|---|
| `b9d0aea9` | menor 5 | a chamada do teste do id forjado volta ao formato do arquivo (o resto do script estava na coluna zero) |
| `6979c2b7` | Importante 1 (e a `const` do menor 6) | um balde por MOD e por classe de linha; o transbordo é por classe; a conta sai no nível da classe |
| `8e55ccbb` | menores 1 e 2 | a conta sai com o cadeado solto, por `Session::dizer_as_contas_do_registro`, que `revogar` chama |
| `33401b5e` | menores 3 e 7 | a linha do transbordo diz `transbordo=true`; o id que o filtro esvazia ganha o balde e o rótulo de ilegível |
| `8f342d73` | menor 4 | guarda de fonte: os cinco comandos passam `&session.baldes_do_registro` às portas |
| `8419adbd` | menor 6 | o guarda do id cru usa `fonte_sem_comentarios` e `corpo_de` |

Arquivo mudado nos seis: só `apps/seele-app/src/main.rs`.

Todos os testes desta seção rodaram com
`perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --bin seele-app …`,
com `CARGO_BUILD_JOBS=8`. As reversões, como na primeira rodada, pelo script
que troca o trecho, roda, desfaz e compara o `main.rs` byte a byte: todas
voltaram «restaurado byte a byte».

### Importante 1: um aviso segurado não atrasava, sumia (`6979c2b7`)

**Produto.**
- `enum Classe { Aviso, Servida, Depuracao }`, com `Classe::TODAS`. A chave do
  mapa vira `(Classe, String)`:
  - `Aviso`: a janela em `aviso` e `erro`, e a mídia recusada;
  - `Servida`: a mídia e o som servidos;
  - `Depuracao`: o braço `_` da janela (DEBUG) e o DEBUG de `sessao-encerrada`
    em `recusa_de_midia_dita`.
- `linha_admitida(&baldes, agora, classe, mod_id)`. `registrar_da_janela_em`
  escolhe a classe pelo `nivel`; `recusa_de_midia_dita` lê o motivo antes do
  balde, para escolher a classe.
- O limite de 64 entradas fica, e o transbordo é por classe: um lugar é
  guardado para o transbordo de cada uma (61 baldes próprios e 3 de
  transbordo). As servidas que transbordam não gastam a ficha de um aviso que
  transborda.
- A conta do fim da sessão (`conta_do_balde_dita`) sai no nível da classe que
  conta: WARN, INFO ou DEBUG. A frase dela virou
  `const CONTA_DO_BALDE_DO_REGISTRO` (é o menor 6, feito aqui porque a função
  foi reescrita).

**Testes** (em `o_teto_de_vazao_do_registro`):
- novo `uma_recusa_depois_de_quarenta_servidas_do_mesmo_mod_sai`: 40
  `midia_servida_dita` e uma recusa, do mesmo MOD e no mesmo instante; a
  recusa sai, e sem `suprimidas`;
- novo `a_depuracao_nao_gasta_a_ficha_nem_infla_a_conta_de_um_aviso`: 40
  linhas DEBUG da janela e um aviso; o aviso sai, sem `suprimidas`;
- `as_portas_do_mesmo_mod_dividem_o_balde` passa a afirmar a divisão dentro
  de cada classe: 17 rodadas das seis portas (janela em aviso, recusa, mídia
  servida, som servido, janela em DEBUG, sessão encerrada) deixam 32 WARN, 32
  INFO e 32 DEBUG;
- `com_o_mapa_cheio_um_id_novo_passa_pelo_transbordo` passa a encher os 61
  baldes próprios, mandar 200 servidas e depois 200 avisos de ids novos: os
  dois deixam entre 1 e 32 linhas (as servidas não secam o transbordo dos
  avisos); os ids novos seguintes não deixam nenhuma; e o mapa fecha em 64,
  com as três classes transbordando.

**RED** (`… -- o_teto_de_vazao_do_registro`, contra `b9d0aea9`): 3 falhas, 6
verdes. O do transbordo, reescrito para as classes, foi provado pela reversão
(b), abaixo.
- `uma_recusa_depois_de_quarenta_servidas…`: `quarenta mídias servidas do mesmo MOD gastaram a ficha da recusa, e ela sumiu do seele.log`;
- `a_depuracao_nao_gasta…`: `quarenta linhas DEBUG da janela seguraram o aviso do mesmo MOD`;
- `as_portas…`: `as duas portas da classe aviso do mesmo MOD não deixaram uma rajada só`, `left: 12`, `right: 32`.

**GREEN:** os quatro módulos do registro, 38 passaram, 0 falharam.

**Reversões.**

| mutação | vermelho |
|---|---|
| (a) uma classe só na chave e no transbordo (`Classe::Aviso` fixo) | as quatro: a recusa atrás das servidas, a depuração, `as_portas…` (`left: 12`) e o transbordo (`left: 62`, `right: 64`) |
| (b) um transbordo só, dividido pelas classes | `com_o_mapa_cheio…`: `` `novo/x` deixou 0 linhas em duzentas: … ou outra classe secou o transbordo desta `` |
| (c) o braço `_` da janela na classe dos avisos | `a_depuracao_nao_gasta…`, `as_portas…` e o transbordo |

Na reversão (b), a primeira versão do teste do transbordo caía na asserção do
tamanho do mapa (62 em vez de 64), antes da que mede a classe. Inverti a ordem
das asserções antes do commit, para o vermelho dizer o motivo de verdade.

### Menores 1 e 2: a conta com o cadeado solto, e uma porta para a saída do app (`8e55ccbb`)

**Produto.** `Session::revogar` passa a chamar
`Session::dizer_as_contas_do_registro()`, que tira o mapa com
`std::mem::take(&mut *guarda)` — a guarda é um temporário da instrução e cai
nela — e só então diz as contas. `revogar` faz o que fazia. O doc de
`linha_admitida` diz que a conta de um MOD que se cala no meio da sessão só
sai na próxima linha dele, da mesma classe, ou no fim da sessão.
`despedir_se` não foi tocado: o lote F vai chamar a função nova.

**Testes:**
- `a_conta_e_dita_com_o_cadeado_dos_baldes_solto`: uma camada do
  `tracing_subscriber` tenta `try_lock` nos baldes a cada evento, na thread
  que escreve. Com a guarda na mão, o `try_lock` volta `WouldBlock`. A sessão
  é `Box::leak`, porque o assinante é `'static`;
- `dizer_as_contas_do_registro_diz_uma_vez_e_esvazia`: 35 avisos de `a/b`,
  e duas chamadas: uma conta só, com `suprimidas=3`, o mapa vazio e a geração
  em 0 (não revoga nada).

**RED.**
- `a_conta_e_dita_com_o_cadeado…`: `a conta dos baldes foi escrita com o cadeado deles na mão`, `left: 1`, `right: 0`;
- `dizer_as_contas…`: não compila, `error[E0599]: no method named dizer_as_contas_do_registro found for struct Session`. É o vermelho de uma extração: o teste chama uma função que ainda não existe.

**GREEN:** `o_teto_de_vazao_do_registro` e `a_carga_recusada…`, 19 passaram,
0 falharam.

**Reversões.**

| mutação | vermelho |
|---|---|
| (a) a conta volta a ser dita com a guarda na mão (`iter_mut` e `clear`) | `a_conta_e_dita_com_o_cadeado…`: `a conta dos baldes foi escrita com o cadeado deles na mão` |
| (b) `revogar` sem chamar `dizer_as_contas_do_registro` | `revogar_diz_a_conta…` (`não disse uma conta por balde`) e `a_conta_e_dita_com_o_cadeado…` (`a sessão fechou com uma linha segurada e não disse a conta dela`) |

### Menores 3 e 7: `transbordo=true`, e o id ilegível fora do catálogo (`33401b5e`)

**Produto.**
- `linha_admitida` devolve `Option<Admitida>`, com `suprimidas: Option<u32>`
  (só quando não é zero) e `transbordo: Option<bool>` (`Some(true)` quando a
  linha passou pelo balde dividido). As quatro portas escrevem os dois
  campos. O doc de `Admitida::transbordo` diz o porquê: a conta de lá é de
  todos os ids que caíram nele.
- `chave_do_balde` sai dos três braços de `IdDoPedido`: vazio é
  `BALDE_DO_CATALOGO`, o que o filtro esvazia é `BALDE_ILEGIVEL`
  (`"\u{0}ilegivel"`), e o resto é o id filtrado. A conta desse balde sai
  como `mod_id_ilegivel=true`.

**Testes:**
- `a_linha_que_passa_pelo_transbordo_diz_que_passou`: com os 61 baldes
  próprios cheios, a linha de `novo/x` traz `transbordo=true`, e a de
  `cheio/0` não traz `transbordo`;
- `um_id_que_o_filtro_esvazia_conta_como_ilegivel_e_nao_como_catalogo`: 34
  avisos com o id `"\u{202E}\n"` não seguram a linha do catálogo, e a conta
  sai uma vez, com `mod_id_ilegivel=true`, `suprimidas=2` e sem `catalogo`.

**RED:** 2 falhas.
- transbordo: `a linha que passou pelo balde dividido não diz transbordo=true`;
- ilegível: `a linha do catálogo foi segurada pelas linhas de um id ilegível`.

**GREEN:** os quatro módulos do registro, 42 passaram, 0 falharam.

**Reversões.**

| mutação | vermelho |
|---|---|
| (a) `transbordo` sempre `None` | `a_linha_que_passa_pelo_transbordo…`: `não diz transbordo=true` |
| (b) o braço ilegível de `chave_do_balde` volta ao `BALDE_DO_CATALOGO` | `um_id_que_o_filtro_esvazia…`: `a linha do catálogo foi segurada pelas linhas de um id ilegível` |

### Menor 4: os comandos passam os baldes da sessão (`8f342d73`)

**Teste:** `os_comandos_passam_os_baldes_da_sessao`. Em `registrar_da_janela`,
`midia_do_mod`, `som_do_mod`, `midia_em_bytes` e `ler_imagem_mod`, lidos por
`fonte_sem_comentarios` e `corpo_de` e sem espaço, toda chamada de uma das
sete portas (`registrar_da_janela_em`, `midia_declarada_do_mod`,
`midia_servida_dita`, `som_declarado_do_mod`, `som_servido_dito`,
`midia_do_servidor_dita`, `recusa_de_midia_dita`) tem de começar com
`&session.baldes_do_registro,`. E cada comando ainda chama tantas portas quanto
chamava (1, 2, 2, 1 e 2).

Ele nasceu verde, porque o código estava certo. A prova é a reversão:

| mutação | vermelho |
|---|---|
| (a) `registrar_da_janela` passa `&BaldesDoRegistro::default()` | `` `fn registrar_da_janela(` chama `registrar_da_janela_em(` sem `&session.baldes_do_registro` `` |
| (b) o mesmo na `midia_servida_dita` de `midia_do_mod` | `` `fn midia_do_mod(` chama `midia_servida_dita(` sem … `` |
| (c) o mesmo na segunda `recusa_de_midia_dita` de `ler_imagem_mod` | `` `async fn ler_imagem_mod(` chama `recusa_de_midia_dita(` sem … `` |
| (d) `midia_em_bytes` chama `ler_midia_do_servidor` direto, por fora da porta | `` `fn midia_em_bytes(` chama 0 das portas medidas, e chamava 1 `` |

### Menor 5: a linha sem formato (`b9d0aea9`)

A chamada de `recusa_de_midia_dita` em `o_id_que_a_janela_manda_nao_forja_linha`
tinha `&BaldesDoRegistro::default(), origem(),` na coluna zero, na mesma linha
do id. O id é longo demais para o `rustfmt`, que não reformata a chamada; foi
consertada à mão. `cargo fmt --all -- --check` limpo, e o teste verde (1
passou).

### Menor 6: os ajudantes duplicados (`8419adbd`, e a `const` em `6979c2b7`)

`nenhum_mod_id_em_display_usa_o_id_cru_do_comando` deixou de ler o `main.rs`
por conta própria: a fonte vem de `fonte_sem_comentarios()`, e o corpo de
`corpo_de`, sem espaço. O que ele confere não muda. Os outros dois ajudantes
parecidos (`corpo_sem_espaco` de `a_carga_recusada…`, que corta pela
assinatura e tira o `,)` do `rustfmt`, e o laço de
`os_quatro_comandos_de_midia…`) não foram tocados: eles provam outra forma, e
não são deste conserto.

**Prova de que o guarda ainda morde:** as três reversões do P51-10, de novo,
depois da troca:

| mutação | vermelho |
|---|---|
| os braços legíveis de `mod_request` voltam a `%id` | `` `fn pedido_a_caminho_dito(` escreve `mod_id = %id` `` (e a forja) |
| `midia_em_bytes` volta a `%id` | `` `fn midia_em_bytes(` escreve `mod_id = %id` `` |
| `escolher_para_o_mod` volta a `%id` | `` `async fn escolher_para_o_mod(` escreve `mod_id = %id` `` |

### O que rodou na rodada

- A suíte `--bin seele-app` inteira, antes do último commit de código: **209
  passaram, 0 falharam** (12,2 s).
- `cargo fmt --all -- --check` e
  `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`
  limpos antes de cada commit. O único aviso é o `ld: duplicate -rpath`.

### Nota para o guia (o nono achado, que não é defeito do lote)

Desde o P51-05, um `mod_id=` que um MOD escreva na frase da janela sai entre
aspas, dentro de `frase="…"`, e depois do `mod_id=` verdadeiro. Mas o grep que
`docs/como-se-faz-um-mod.md` ensina (`grep "mod_id=fulano/meu-mod"`) casa com
qualquer pedaço da linha, e acha também a linha de outro MOD cuja frase cite
`mod_id=fulano/meu-mod`. Quem atualizar o guia pode dizer que o campo
verdadeiro é o que vem antes de `frase=`, ou ensinar a procurar o campo e não o
texto. O lote não mexe no guia.

### Preocupações da rodada

1. **A memória por MOD triplica.** Um MOD que escreve nas três classes ocupa
   três entradas do mapa, e o limite de 64 continua: cabem 61 pares (MOD,
   classe) com balde próprio, cerca de vinte MODs que usam as três classes.
   Os que chegam depois dividem o transbordo da classe. Com os MODs de uma
   sessão de verdade, é folga; com uma janela que inventa ids, é o teto.
2. **A conta na saída do app continua perdida até o lote F** chamar
   `dizer_as_contas_do_registro` em `despedir_se`.
3. **O teste do cadeado depende de uma propriedade do `std`**: `try_lock` na
   thread que já tem a guarda volta `WouldBlock`, e não trava. É o que a
   documentação de `Mutex::try_lock` promete; se um dia o mapa trocar de
   cadeado, o teste tem de ser revisto junto.

## Rodada 2

A re-revisão resolveu os nove achados e pediu uma passada curta. Mesmo
branch, a partir de `6c6f8a1a`. Dois commits de código e este relatório; só
`apps/seele-app/src/main.rs` mudou.

| commit | item | o que muda |
|---|---|---|
| `6e71e894` | 2 e 3 | a «mídia de MOD vinda do servidor» passa pelo teto, na classe `Depuracao`; o doc de `conta_do_balde_dita` diz quem a chama |
| `bc2043a1` | 1 | o teste do cadeado conta só a linha da conta, e chama `segundo_despachante` |
| (este) | 4 | a contagem dos achados no relatório |

### Item 2: a última porta sem teto (`6e71e894`)

O DEBUG «mídia de MOD vinda do servidor» de `midia_em_bytes` sai por
`midia_vinda_do_servidor_dita(&baldes, agora, mod_id, geracao, papel, bytes)`,
que passa por `linha_admitida` e escreve `suprimidas` e `transbordo` como as
outras portas. **Na classe `Depuracao`, e não `Servida`:** a linha sai em
DEBUG, que o filtro padrão do `seele.log` não escreve, e na classe das
servidas ela gastaria a ficha de uma INFO que o arquivo mostra.

Testes:
- novo `a_midia_vinda_do_servidor_tem_teto_na_classe_da_depuracao`: 200
  chamadas deixam 32 linhas, todas DEBUG, e a «mídia de MOD servida» do mesmo
  MOD, logo depois, ainda sai;
- novo `a_midia_vinda_do_servidor_nao_forja_linha`: com
  `"a/b\nWARN seele_app: forjada"`, uma linha só, com o id filtrado;
- `os_comandos_passam_os_baldes_da_sessao` conta a porta nova, e
  `midia_em_bytes` passa a ter duas chamadas;
- `nenhum_mod_id_em_display_usa_o_id_cru_do_comando` confere a função nova,
  pelo nome que o id cru tem nela (`mod_id`, e não `id`).

**RED**, com a linha extraída primeiro sem balde: `duzentas mídias vindas do
servidor no mesmo instante passaram de outro jeito que a rajada`, `left: 200`,
`right: 32`.

**Reversões:**

| mutação | vermelho |
|---|---|
| (a) sem `linha_admitida` | `left: 200`, `right: 32` |
| (b) `Classe::Servida` em vez de `Depuracao` | `as mídias vindas do servidor gastaram a ficha das servidas do mesmo MOD` |
| (c) `midia_em_bytes` passa `&BaldesDoRegistro::default()` | `` `fn midia_em_bytes(` chama `midia_vinda_do_servidor_dita(` sem `&session.baldes_do_registro` `` |
| (d) `mod_id = %mod_id` cru | `a_midia_vinda_do_servidor_nao_forja_linha` e o guarda: `` escreve `mod_id = %mod_id` com o id cru `` |

**Um tropeço do script de reversão, e o que fiz.** Na primeira tentativa da
(b), o texto trocado (`Classe::Servida, mod_id)`) aparecia em três lugares
depois da troca, e o script parou antes de desfazê-la. As reversões (c) e (d)
rodaram em cima desse arquivo mudado. Achei pela saída («esperava 1 ocorrência,
achei 3»), restaurei a linha à mão, conferi o `git diff` inteiro contra o
commit, guardei uma cópia do arquivo pronto e refiz a (b), a (c) e a (d) com
trechos únicos nos dois sentidos. As três deram o vermelho da tabela, e o
arquivo voltou igual à cópia (`cmp`).

### Item 3: o doc da conta (`6e71e894`)

O doc de `conta_do_balde_dita` deixa de dizer «quando a sessão fecha
(`Session::revogar`)»: quem a chama é `Session::dizer_as_contas_do_registro`,
ao fechar a sessão e na saída do app, que não passa por `revogar`.

### Item 1: a asserção que podia ficar vazia (`bc2043a1`)

A camada de `a_conta_e_dita_com_o_cadeado_dos_baldes_solto` passa a olhar só
os eventos cuja mensagem é `CONTA_DO_BALDE_DO_REGISTRO` (um `Visit` no campo
`message`), e a asserção pede exatamente uma conta, em vez de `>= 1` de
qualquer evento. O teste chama `rastro_de_teste::segundo_despachante()`, como
`capturar` faz, e a função passou a `pub(crate)` para isso.

**Reversões:**

| mutação | vermelho |
|---|---|
| (a) a conta dita com a guarda na mão | `a conta dos baldes foi escrita com o cadeado deles na mão` |
| (b) `revogar` sem `dizer_as_contas_do_registro` | `a sessão fechou com um balde de linhas seguradas e não disse a conta dele uma vez`, `left: 0`, `right: 1` |
| (c) o mesmo, com outra linha (`tracing::info!`) no lugar da conta | o mesmo vermelho: é o caso que o `>= 1` antigo deixava passar |

### Item 4: a contagem

A rodada 1 tinha «um Importante e oito menores» e «o nono menor». Os achados
eram nove: um Importante, sete menores e a nota para o guia, que não é defeito
do lote. As duas frases foram acertadas.

### O que rodou

- A suíte `--bin seele-app` inteira: **211 passaram, 0 falharam** (11,6 s).
- `cargo fmt --all -- --check` e
  `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`
  limpos antes de cada commit; só o aviso ambiental `ld: duplicate -rpath`.
