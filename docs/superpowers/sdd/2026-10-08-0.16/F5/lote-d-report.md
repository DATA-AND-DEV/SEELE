# Lote D da F5: relatório

Base `0f94be66` (branch `0.16/lotes`). Branch `0.16/lotes-d`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-ac2f880ac94306916`. Cinco
commits de código e este relatório, todos locais: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `3f1d2e93` | P51-01 | o `Falhou` da promessa rejeitada sem tratamento sai do rastreador e passa a esperar o fim da volta com a linha: sai só se ninguém a pegou, cortado no teto do registro, depois da linha |
| `59d66aa0` | P51-14 m8 | a pilha (`\n    at `) passa a ser exigida na linha do temporizador, na do ouvinte e no `console.error` cru |
| — | P51-14 m9 | nada a fazer: a porta da janela já está em `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas` (ver §3) |
| `6bf9ddd7` | P51-14 T3 M2 | dois testes por `registrar_da_janela_em`: «erro» sai em ERROR e «outro» em DEBUG, os dois com `mod_id=a/b` |
| `bead90c2` | P51-14 T1 | `semMentir` diz o `Error`, a função e o símbolo aninhados; um ciclo vira «[cíclico]» só onde é ancestral, numa segunda passada |
| `74a5c427` | P51-14 T2 N1 | `BaldeDoConsole::ate_a_proxima_ficha`; a espera do laço de `rodar` acorda no menor prazo entre o temporizador e a ficha |
| `7817f871` | T2 N1, rodada 1 | a espera só acorda quando sobra uma ficha além da do MOD (`ate_sobrar_uma_ficha`, duas fichas); guardas da escrita contínua, da parada e do fim de volta de resposta; o teste do laço volta aos 400 ms; `assert` de compilação da vazão |
| `8719c9ee` | T1, rodada 1 | orçamento de 1024 objetos por passada em `emTexto` |
| `20404362` | P51-01, rodada 1 | o `Falhou` da conta além da espera ganha o sujeito |
| `047c40ea` | T2 N1, rodada 2 | o fim de uma volta em que nenhuma linha tentou passar pede duas fichas; guardas de eventos e dos dois fins de volta; docs com a regra de verdade |
| `b7ac8fc7` | T2 N1, rodada 3 | o fim de toda volta pede duas fichas, e o ramo de uma ficha sai; guarda do MOD que escreve e trabalha; docs conferidos um a um |

As rodadas 1, 2 e 3 de conserto estão no fim deste relatório. Elas reescrevem
o que as seções 5 e 6 e as «Preocupações» 1, 2, 5, 6 e 7 dizem; a rodada 2
reescreve a preocupação 1 da rodada 1, e a rodada 3 apaga o ramo de uma ficha
que a rodada 2 manteve.

Arquivos mudados (`0f94be66..74a5c427`), só os da lista do lote:
- `apps/seele-app/src/executor.rs`;
- `apps/seele-app/src/main.rs`: só testes (`mod a_falha_do_mod_chega_ao_registro`
  e `mod o_registro_da_janela`);
- `docs/como-se-faz-um-mod.md`: a frase das seguradas (opcional do brief).

Nada no fio (`seele-proto`), no SEELE-ENC/1, nos arquivos da F6, em `api/`, na
CSP, em `ui/` ou nas bancadas. Nenhuma dependência nova, nada baixado.
`capacidades_da_api` e `comCapacidade` não foram tocados. `docs/pendencias.md`
e o índice da 1.0 não foram tocados.

As reversões foram feitas copiando o arquivo para o scratchpad, trocando o
trecho exato, rodando os testes, devolvendo a cópia e conferindo com `cmp`:
todas voltaram «RESTAURADO_IGUAL».

## 1. P51-01: a rejeição é dita depois da volta, e com teto (`3f1d2e93`)

**Produto** (`executor.rs`).
- **(a)** O rastreador não chama mais `avisar(…Falhou…)` no caminho normal. Ele
  monta `falhou = "promessa rejeitada sem tratamento: <texto>"` e o entrega à
  espera junto da linha. O `avisar` ficou só no braço `Err(_)` do
  `try_borrow_mut` (a reentrada que não existe): ali a linha sai na hora pelo
  balde e, depois dela, o `Falhou`, cortado por `cortar_linha_do_console`. O
  comentário do rastreador («A janela já recebeu o `Falhou` da primeira…»)
  passou a dizer que nem a janela nem o registro ficam sabendo na primeira
  chamada.
- **(b)** `RejeicoesDaVolta::guardadas` passou de `Vec<(Persistent, LinhaCortada)>`
  a `Vec<Rejeitada>`, uma struct com `promessa`, `linha` e `falhou`, os dois
  textos cortados por `cortar_linha_do_console` na entrada. `guardar` ganhou o
  parâmetro `falhou: String`. `alem` continua só número. `sem_tratamento`
  devolve `Vec<(LinhaCortada, LinhaCortada)>` (linha, falhou).
- **(c)** `dizer_as_rejeitadas(rejeicoes, balde, manda, fila: &Arc<Fila>)`: para
  cada uma, a linha pelo balde (`linha_ja_cortada`) e **depois**
  `avisar(manda, fila, Falhou(falhou))`. Com `alem > 0`, uma linha e um
  `Falhou` só, os dois com `alem_da_espera(alem)` («e mais N rejeições sem
  tratamento além da espera»). Os três chamadores passam `fila`: `escoar_jobs`,
  a parada de `rodar` e o teste da linha que espera.
- **(d)** `tratada` continua tirando a rejeição da espera — agora com o
  `Falhou` junto.
- **(e)** O doc de `REJEICOES_GUARDADAS` perdeu o parágrafo «O `Falhou` que vai
  à janela não passa por esta espera, nem por este corte…», e o parágrafo da
  memória passou a contar a linha **e** o `Falhou` de cada uma. Os docs de
  `RejeicoesDaVolta`, `escoar_jobs` e `dizer_as_rejeitadas` acompanham (este
  último diz por que a linha vem antes: a bomba lê o canal em ordem, e quem vê
  «falhou» na gestão já acha a linha no `seele.log`).

**Testes.**
- `uma_rejeicao_pega_logo_depois_nao_vira_linha_de_erro` (`main.rs`) ganha a
  exigência de que nenhum `ParaOFora::Falhou` contenha «pega logo depois».
- Novo `o_falhou_da_rejeicao_tem_o_teto_do_registro` (`executor.rs`):
  `Promise.reject(new Error('x'.repeat(100000)))` exige o `Falhou` começando
  pela frase, com o aviso `[…cortado: …]` e o corpo até
  `TETO_DA_FRASE_NO_REGISTRO`.
- `a_linha_que_espera_o_fim_da_volta_ja_cabe_no_teto_e_sai_sem_novo_corte`
  ganha uma `Fila`, continua lendo o primeiro `try_recv()` como `Console`, e
  exige que o segundo seja o `Falhou` cortado uma vez só:
  `"promessa rejeitada sem tratamento: " + 477 × "y" + " […cortado: 1000035 caracteres]"`.
- `a_espera_das_rejeicoes_tem_teto_e_conta_a_que_nao_coube` só passa o novo
  argumento de `guardar`.

**RED** (antes do conserto, com os testes novos):
```
thread 'executor::testes::o_falhou_da_rejeicao_tem_o_teto_do_registro' panicked at apps/seele-app/src/executor.rs:3986:13:
o `Falhou` de uma rejeição de cem mil caracteres saiu com 100035 caracteres, sem o corte do registro: o canal e a fila da janela levam o texto inteiro a cada aviso
thread 'a_falha_do_mod_chega_ao_registro::uma_rejeicao_pega_logo_depois_nao_vira_linha_de_erro' panicked at apps/seele-app/src/main.rs:11844:9:
uma rejeição que o MOD pegou com `try`/`await` chegou à janela como `Falhou`, e a gestão diz que o MOD falhou por uma promessa que ele tratou: [Falhou("promessa rejeitada sem tratamento: pega logo depois"), Falhou("promessa rejeitada sem tratamento: cannot read property 'x' of null"), Mensagem("fim")]
test result: FAILED. 0 passed; 2 failed
```
A extensão do teste da linha que espera não teve RED próprio: ela precisa da
assinatura nova de `guardar` e de `dizer_as_rejeitadas`, e antes do conserto
não compilava. A prova dela é a reversão da ordem, abaixo.

**GREEN** (`cargo test … -- rejei`): os oito testes de rejeição passam,
inclusive `uma_promessa_rejeitada_sem_tratamento_nao_morre_calada` (executor) e
`uma_promessa_rejeitada_sem_tratamento_chega_ao_registro_com_o_texto` (main),
que o brief manda continuarem verdes; `-- executor a_falha_do_mod_chega_ao_registro`:
`54 passed; 0 failed`.

**Reversões.**
1. O `avisar(…Falhou(falhou.clone())…)` devolvido ao caminho normal do
   rastreador (a do brief): a primeira asserção fica vermelha, e o teste do
   teto também.
   ```
   uma rejeição que o MOD pegou com `try`/`await` chegou à janela como `Falhou`, … [Falhou("promessa rejeitada sem tratamento: pega logo depois"), …]
   o `Falhou` de uma rejeição de cem mil caracteres saiu com 100035 caracteres, sem o corte do registro: …
   test result: FAILED. 6 passed; 2 failed
   ```
2. O `avisar` antes de `linha_ja_cortada` em `dizer_as_rejeitadas`:
   `a linha guardada não saiu no fim da volta, ou saiu depois do `Falhou`` —
   `FAILED. 0 passed; 1 failed`.
3. `falhou: LinhaCortada(falhou)` (sem o corte) em `guardar`: os dois testes do
   teto ficam vermelhos (`saiu com 100035 caracteres, sem o corte do registro`).

## 2. P51-14 m8: a pilha tem prova (`59d66aa0`)

**Testes.**
- `um_temporizador_que_lanca_deixa_linhas_limitadas_pelo_balde` e
  `um_ouvinte_de_evento_que_lanca_deixa_a_linha` (`main.rs`): a linha de erro
  achada passa a exigir `r"\n    at "` — no `seele.log` a quebra sai escapada.
- `cada_metodo_do_console_escreve_no_seu_nivel` (`executor.rs`, texto cru):
  exige `"\n    at "`, a quebra de verdade.

**RED/GREEN.** São guardas sobre comportamento que já existia: os três passam
com o código de `0f94be66` (`3 passed`). O vermelho é o da reversão.

**Reversão** (a do brief): `+ (valor.stack ? '\n' + String(valor.stack) : '')`
tirado de `emTexto` reprova os três:
```
um `Error` passado ao console perdeu a pilha, que diz onde no MOD ele nasceu: "Error: quebrou"
a linha do erro de um ouvinte de evento perdeu a pilha, … texto="erro num ouvinte de evento: TypeError: cannot read property 'b' of null"
a linha do erro de um temporizador perdeu a pilha, … texto="erro num temporizador: TypeError: cannot read property 'b' of null"
test result: FAILED. 0 passed; 3 failed
```

## 3. P51-14 m9: conferido, sem commit

O C pôs a porta da janela em `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas`
(`main.rs`, `mod a_midia_recusada_e_dita_no_registro`): ela entrou em
`0844129b` (P51-05) e passou a `registrar_da_janela_em(&BaldesDoRegistro::default(), origem(), "aviso", "recusa-de-mod", pior.clone(), Some(ID))`
em `37a5d902` (P51-08). O caso usa `"\u{100000}".repeat(TETO_DA_FRASE_NO_REGISTRO * 4)`
(2048 caracteres), tira o campo `frase="…"`, exige o aviso
`" […cortado: 2048 caracteres]"`, o teto escapado (`tamanho <= 512`), que não
cortou além do que precisava, e o mesmo texto que o `console` — o que
`ate_o_teto_do_registro` promete. Nada a escrever.

## 4. P51-14 T3 M2: o erro e a depuração da janela levam o id (`6bf9ddd7`)

**Testes** (`mod o_registro_da_janela`, com `BaldesDoRegistro::default()` e
`origem()`, no molde dos vizinhos):
- `o_erro_da_janela_sai_como_error_com_o_id_do_mod`: nível «erro»,
  `Some("a/b")`, exige os campos `ERROR` e `mod_id=a/b`;
- `um_nivel_desconhecido_da_janela_sai_como_debug_com_o_id_do_mod`: nível
  «outro», exige `DEBUG` e `mod_id=a/b`.
Procuram campos (pedaços separados por espaço), e não a forma da linha.

**RED/GREEN.** Guardas sobre comportamento que já existia: `9 passed` em
`o_registro_da_janela`. O vermelho é o da reversão.

**Reversões.**
1. `mod_id` tirado do braço «erro» (a do brief):
   `o erro da janela saiu sem `mod_id=`, … ERROR seele_app: a janela disse onde=atender-mod frase="uma frase da janela"`
   — `8 passed; 1 failed`.
2. `mod_id` tirado do braço DEBUG:
   `a linha DEBUG da janela saiu sem `mod_id=`, … DEBUG seele_app: a janela disse onde=atender-mod frase="uma frase da janela"`
   — `8 passed; 1 failed`.

## 5. P51-14 T1: o aninhado e o ciclo em `emTexto` (`bead90c2`)

**Produto** (o `PRELUDIO`, antes de `globalThis.console = {`).
- `semMentir` passa a dizer `name: message` para um `Error` aninhado,
  `[função nome]` para uma função (por `funcaoEmTexto`, que o `emTexto` também
  usa) e `String(v)` para um símbolo.
- `semCiclo()`: a segunda passada. Só roda quando a primeira
  (`JSON.stringify(valor, semMentir)`) lança. Os ancestrais saem do `this` do
  substituto: a pilha desce até o degrau cujo valor devolvido é o `this`, e o
  que sobra é o caminho da raiz até o valor de agora. Cada degrau guarda o
  valor como o MOD o tem e como `semMentir` o devolveu (um `Map` vira uma lista
  nova, e é ela o `this` dos filhos). Sem `WeakSet`, e sem `push`/`pop`: um
  índice de fundo.
- `comoJson(valor, substituto)` junta o `stringify` e o recuo de `undefined`.
- O comentário «**Cada parte se protege sozinha.**» troca o exemplo do objeto
  cíclico sem protótipo pelo do getter que lança.

**Testes** (`executor.rs`).
- Novo `um_error_uma_funcao_e_um_simbolo_aninhados_dizem_o_que_sao`:
  `console.log({ e: new Error('x'), f() {}, s: Symbol('s') })` contém
  «Error: x», «[função f]» e «Symbol(s)».
- Novo `um_objeto_ciclico_diz_onde_e_ciclico_e_nao_vira_object_object`:
  `o.o = o` contém «cíclico» e «raiz», e não «[object Object]»; e
  `p = { x: r, y: r }; p.p = p` sai exatamente
  `{"x":{"a":1},"y":{"a":1},"p":"[cíclico]"}` — a referência repetida não é
  ciclo.
- `uma_parte_que_nao_vira_texto_nao_derruba_a_linha_do_console` troca o exemplo
  por `Object.create(null)` com um getter enumerável que lança; o resultado
  esperado («contexto importante: [valor que não virou texto]») não muda.

**RED:**
```
um objeto cíclico saiu sem os campos dele, ou sem dizer onde é cíclico: [object Object]
um `Error` aninhado sumiu da linha do console, ou saiu como `{}`: esperava «Error: x» em {"e":{}}
test executor::testes::uma_parte_que_nao_vira_texto_nao_derruba_a_linha_do_console ... ok
test result: FAILED. 1 passed; 2 failed
```

**GREEN:** `-- executor`: `45 passed; 0 failed`; `mod_de_referencia`: `4 passed`.
Uma sonda temporária (não commitada) conferiu os casos de borda no motor:
`Map` que contém a si mesmo → `[["eu","[cíclico]"]]`; lista e `Set` que contêm
a si mesmos → `[1,"[cíclico]"]` e `["[cíclico]"]`; um `Error` com `e.self = e`
aninhado → `{"e":"Error: x"}`; `d.filho.avo = d; d.irmao = d.filho` →
`{"filho":{"avo":"[cíclico]"},"irmao":{"avo":"[cíclico]"}}`; funções anônimas e
símbolos numa lista → `["Symbol(k)","[função anônima]"]`.

**Reversões.**
1. Sem a segunda passada: `um objeto cíclico saiu sem os campos dele… [object Object]`
   — `2 passed; 1 failed`.
2. A pilha que nunca desce (o `while` que acha o `this` tirado), que é o que um
   `WeakSet` faria:
   ```
   left: "{\"x\":{\"a\":1},\"y\":\"[cíclico]\",\"p\":\"[cíclico]\"}"
   right: "{\"x\":{\"a\":1},\"y\":{\"a\":1},\"p\":\"[cíclico]\"}"
   ```
3. `semMentir` sem os três casos novos:
   `um `Error` aninhado sumiu da linha do console, ou saiu como `{}`: esperava «Error: x» em {"e":{}}`.

## 6. P51-14 T2 N1: a conta sem esperar o MOD parar (`74a5c427`)

**Produto** (`executor.rs`).
- `BaldeDoConsole::ate_a_proxima_ficha(&self, agora) -> Option<Duration>`,
  privado, só leitura: `None` sem `suprimidas`; senão, quanto falta para uma
  ficha inteira, arredondado para cima ao microssegundo (`Some(ZERO)` com a
  ficha já lá).
- No laço de `rodar`, `prazo = [proximo_vencimento(&relogios), ficha].into_iter().flatten().min()`:
  `recv_timeout(prazo)`, e o `recv()` sem prazo só quando os dois são `None`. O
  ramo do timeout já chamava `dizer_as_seguradas` com `pendentes_se_couber`.
- Docs: o do balde (:476), o de `pendentes_se_couber`, o de
  `dizer_as_seguradas` (:581, «…tem a conta dita quando para»), o comentário da
  espera (:1275, «sem temporizador ela dorme no canal») e os dois comentários
  do ramo do timeout.
- `docs/como-se-faz-um-mod.md`: depois de «a linha seguinte diz quantas foram
  seguradas (`suprimidas=`)», a frase de que o produto escreve essa linha, em
  WARN, se o MOD se calar, em até um quarto de segundo.

**Testes** (`executor.rs`).
- Novo `a_conta_das_seguradas_sai_sem_esperar_o_mod_parar`:
  `for (let i=0;i<100;i++) console.log(i);`, sem temporizador e sem Encerrar,
  recebe em até 1 s um `Console` com `CONTA_DO_BALDE`, em aviso, e
  `suprimidas >= 60`.
- Novo `o_balde_diz_quanto_falta_para_a_proxima_ficha` (relógio fixo): `None`
  cheio e seco sem nada segurado; `Some(250 ms)` com uma segurada; 150 ms
  depois de 100 ms; um microssegundo antes do prazo `pendentes_se_couber` dá
  `None`, no prazo dá `Some(1)`; depois da conta, `None`.
- `a_conta_das_seguradas_sai_na_primeira_volta_com_ficha` **renomeado** para
  `a_conta_das_seguradas_nao_espera_um_temporizador_mais_tarde_nem_uma_resposta`,
  com o doc dizendo o que ele mede agora (o nome antigo fica no doc):
  - o temporizador passou de 400 ms para **um minuto**: a conta só chega nos
    cinco segundos de `Soma::ate` se a espera for o menor dos dois prazos — a
    metade do relógio virou o guarda do `min`;
  - a metade da resposta perdeu o `sleep` de 400 ms: espera a conta, entrega a
    resposta, e confere pelo diagnóstico que a volta dela não a diz de novo
    nem a perde (a soma fecha em mil).
- `um_mod_em_laco_no_console_nao_passa_da_rajada_e_conta_o_resto` **mudou**
  (fora da lista do brief; ver «Preocupações», 1): o temporizador da linha
  «depois» passou de 400 ms a 1 s, e a linha `CONTA_DO_BALDE` entra na soma sem
  contar como linha do laço.
- `uma_rajada_no_console_seguida_de_silencio_tem_a_conta_dita_ao_parar` não
  mudou e continua verde.

**RED** (o teste do brief, antes do conserto):
```
um MOD que escreveu cem linhas e se calou, sem temporizador nem Encerrar, não teve a conta das seguradas dita em um segundo — ela espera a parada, e o seele.log mostra 32 linhas como se fossem tudo; veio None
test result: FAILED. 0 passed; 1 failed (1.01s)
```
O teste de `ate_a_proxima_ficha` foi escrito antes do método, e o vermelho
dele foi de compilação.

**GREEN:** `-- executor`: `47 passed`; a bateria do binário inteira
(`cargo test -p seele-app --bin seele-app`): `248 passed; 0 failed`;
`mod_de_referencia`: `4 passed`. `-- executor a_falha_do_mod_chega_ao_registro o_registro_da_janela`
rodou cinco vezes seguidas: `67 passed` nas cinco.

**Reversões.**
1. A espera só pelo vencimento (`let prazo = proximo_vencimento(&relogios);`):
   ```
   a_conta_das_seguradas_sai_sem_esperar_o_mod_parar: … não teve a conta das seguradas dita em um segundo … veio None
   a_conta_das_seguradas_nao_espera_um_temporizador_mais_tarde_nem_uma_resposta: a conta das seguradas esperou o temporizador de um minuto, e não a ficha: a espera do laço usou o vencimento, e não o menor dos dois prazos: nada mais chegou em cinco segundos
   test result: FAILED. 3 passed; 2 failed
   ```
2. O vencimento antes da ficha (`proximo_vencimento(&relogios).or(ficha)`, sem
   o `min`): só o renomeado fica vermelho, com a mesma frase — é ele que prova
   o `min`.
3. `+ 1` µs no prazo: `left: Some(250.001ms) right: Some(250ms)`.
4. Sem o `if self.suprimidas == 0 { return None; }`:
   `left: Some(0ns) right: None` (sem ele a thread giraria em
   `recv_timeout(0)`).
5. `falta / por_micro` no lugar de `div_ceil`: **continua verde** — ver
   «Preocupações», 6.

## O que rodou, e o que não rodou

Rodou, neste worktree, com `CARGO_BUILD_JOBS=8` e `perl -e 'alarm 600; …'`:
- `cargo test -p seele-app --bin seele-app` inteiro: `248 passed`;
- `cargo test -p seele-conformance --test mod_de_referencia`: `4 passed`;
- `cargo fmt --all -- --check` e `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`
  antes de cada commit: limpos (só o `ld: duplicate -rpath`, ambiental).

Não rodou:
- `cargo test --workspace` e a bateria inteira: proibidos pelo brief; ficam
  para a integração.
- `cargo xtask check-api`: `api/` não foi tocado.
- `cargo xtask check-runtime` e as bancadas: `ui/` e `bancada/` não foram
  tocados.
- O clippy de outros crates: só o `seele-app` mudou, e nenhum item dele é
  usado por outro crate.
- Nada de `cfg(windows)` ou `cfg(target_os = "linux")` foi tocado.
- O aplicativo nativo: nenhuma medição no app. O que muda no `seele.log` (o
  texto dos aninhados, a hora da conta) e na gestão (a hora do `Falhou`) só foi
  medido nos testes.

## Preocupações

1. **Um teste fora da lista do brief mudou: `um_mod_em_laco_no_console_nao_passa_da_rajada_e_conta_o_resto`.**
   Com o T2 N1, a linha da conta sai aos 250 ms e gasta a ficha, como manda a
   regra que já existia («a linha que diz a conta… gasta ficha», doc de
   `pendentes_se_couber`). A linha «depois» do MOD, aos 400 ms, achava o balde
   com 0,6 ficha e era segurada; o teste esperava por ela e reprovou com
   `veio None no meio das linhas de console`. Ajustei o teste (1 s em vez de
   400 ms, e a linha da conta fora da contagem do laço), sem mudar a tese dele:
   o balde segura e conta, e a soma fecha em mil. **O efeito no produto:** depois
   de uma rajada, a primeira linha do MOD entre ~250 e ~500 ms depois pode ser
   segurada (e contada), porque a do produto levou a ficha; antes, era ela que
   levava a conta. É a regra de antes aplicada mais cedo, mas o dono pode
   querer julgar.
2. **O texto do `Falhou` da conta** das rejeições além da espera é
   `alem_da_espera(alem)` («e mais N rejeições sem tratamento além da espera»),
   sem o prefixo «promessa rejeitada sem tratamento: ». O brief diz «um
   `Falhou` só, com a conta», sem o texto.
3. **O corte do `Falhou` conta a frase do produto.** O que é cortado é
   `"promessa rejeitada sem tratamento: " + texto` inteiro, como na linha do
   registro: o corpo cabe nos 512 com a frase, e o aviso conta os caracteres da
   frase também (`100035` para uma mensagem de cem mil). É a mesma regra da
   linha.
4. **A gestão sabe da rejeição no fim da volta, e não no meio dela** (o risco
   que o brief nomeia). E a cota dos avisos (`AVISOS_NA_FILA`, 16) continua
   limitando os `Falhou` em número: das 31 rejeições que cabem na espera, só 16
   chegam se a janela não colher — como antes, quando o rastreador mandava um
   por rejeição.
5. **A segunda passada do T1 roda em qualquer exceção da primeira**, e não só
   no ciclo: um getter do MOD que lança é chamado duas vezes no caminho do
   `console`. Separar o ciclo das outras exceções pediria casar a mensagem do
   QuickJS («circular reference»), e um getter pode lançar `TypeError` também.
   E o T1 muda o texto dos objetos aninhados no `seele.log` (risco do brief).
6. **O `div_ceil` de `ate_a_proxima_ficha` não tem prova por reversão.** Com
   `FICHA = 1_000_000` e quatro linhas por segundo, as fichas andam sempre em
   múltiplos de quatro e a divisão é exata: trocá-lo por `/` continua verde. O
   doc do método diz isso. O que o teste prova é que o prazo é exato ao
   microssegundo (a reversão de `+ 1` µs reprova).
7. **`uma_rajada_no_console_seguida_de_silencio_tem_a_conta_dita_ao_parar`**
   continua verde, mas só prova a parada quando o `Encerrar` chega antes da
   primeira ficha (250 ms depois do laço), que é o normal; numa máquina parada
   esse tempo, a conta sai pela ficha antes, e o teste continua verde por ela.
8. **`api/README.md` (:44-55)** diz que «a gestão de MODs continua dizendo
   «falhou» como antes, na exceção do topo e na promessa». Continua certo, mas
   agora só para a promessa que ninguém pegou até o fim da volta. O arquivo não
   é deste lote; ver «Para a v6».

## Para a v6

Nenhuma promessa pública nova de API: o `console` não está em nenhum
`api/v*.json`, e nada mudou no fio, no retrato ou nos eventos. Dois textos que
a F3 pode querer levar ao `api/README.md` da API 6:
- a promessa rejeitada sem tratamento faz a gestão dizer «falhou» **só se
  ninguém a pegou até o fim da volta**, e o motivo vem cortado em 512
  caracteres com o aviso do corte;
- a conta das linhas de `console` seguradas sai sozinha, em WARN, em até meio
  segundo depois que o MOD se cala, e não toma a vez de quem continua
  escrevendo (hoje dita só em `docs/como-se-faz-um-mod.md`, que não é API; o
  «um quarto de segundo» da primeira versão mudou na rodada 1).

## Rodada 1 de conserto

A revisão aprovou o P51-01, o m8, o m9 e o T3 M2, e pediu conserto por dois
Importantes medidos e três menores. Mesmo worktree, mesmo branch, a partir de
`6ab208e4`. Três commits de código e este relatório. As reversões seguem o
método de antes (cópia no scratchpad, troca, teste, cópia de volta, `cmp`),
e todas voltaram «RESTAURADO_IGUAL».

### Importante 1: a linha da conta tomava a vez do MOD (`7817f871`)

**O defeito.** A espera acordava no microssegundo em que a ficha se
completava, e a linha `CONTA_DO_BALDE` a gastava. A linha seguinte do MOD
achava menos de uma ficha e era segurada, o que fazia o produto acordar de
novo na ficha seguinte, e o ciclo se repetia. A minha preocupação 1 era esse
defeito, e ele é permanente. Ajustar `um_mod_em_laco…` para 1 s tirou a
disputa do teste em vez de consertá-la: adaptei o teste ao código. Esse ajuste
foi desfeito.

**A escolha: duas fichas.** `ate_a_proxima_ficha` virou
`ate_sobrar_uma_ficha`, que mira `2 * FICHA`. O produto só fala quando há uma
ficha que o MOD não vai precisar:
- quem escreve quatro linhas por segundo ou mais gasta a primeira ficha antes
  de juntar a segunda, e leva a conta na própria linha;
- quem se calou tem a conta em até meio segundo (medido: 504 ms), e ainda acha
  uma ficha quando volta a escrever.

A outra opção — guardar a última tentativa de `admitir` e mirar
`max(ficha, última + 250 ms)` — pedia um campo a mais. E deixava a conta gastar
a única ficha: um MOD que voltasse a escrever logo depois do prazo achava o
balde seco, o mesmo defeito numa janela menor.

O fim de uma volta continua pedindo uma ficha só (`pendentes_se_couber`): ali
o MOD acabou de ter a vez, e a ficha que ficou é uma que ele não usou. *(A
rodada 2 mostrou que essa justificativa não vale para a volta em que o MOD não
escreveu, e mudou a regra dela; ver «Rodada 2».)* Os temporizadores vencidos
rodam antes da conta no ramo do timeout, e por isso, quando os dois vencem
juntos, a linha do MOD leva a conta.

**A medida** (3 rodadas de 4 s, depois de uma rajada de 100, linhas do MOD /
linhas da conta; mesma sonda para as três linhas, removida depois):

| | P = 100 ms | P = 16 ms |
|---|---|---|
| antes (`74a5c427`) | 1/14, 0/15, 0/15 | 7/8, 5/11, 2/13 |
| sem a espera da ficha (prazo só do vencimento) | 15/0, 15/0, 15/0 | 15/0, 15/0, 15/0 |
| depois (`7817f871`) | 15/0, 15/0, 15/0 | 15/0, 15/0, 15/0 |

**Testes.**
- Novo `um_mod_que_continua_escrevendo_nao_perde_a_vez_para_a_conta`: uma
  rajada de 100 e depois `setInterval(() => console.log('do MOD'), 100)` por
  2 s. Exige no máximo uma linha de conta, mais linhas do MOD que da conta, e
  pelo menos quatro do MOD.
- `um_mod_em_laco_no_console_nao_passa_da_rajada_e_conta_o_resto` voltou byte
  a byte ao de `0f94be66` (`cmp` do trecho contra `git show 0f94be66:…`), com
  os 400 ms originais. Passa sem mudança.
- `o_balde_diz_quanto_falta_para_a_proxima_ficha` virou
  `o_balde_diz_quanto_falta_para_sobrar_uma_ficha`. Exige 500 ms com uma
  segurada e 400 ms depois de 100 ms. Exige também que, acordado no prazo, a
  conta saia e a linha seguinte do MOD passe, e que um microssegundo antes a
  conta tome a ficha do MOD.
- Menor 3a: `uma_rajada_no_console_seguida_de_silencio_tem_a_conta_dita_ao_parar`
  foi renomeado `…_tem_a_conta_dita_ate_parar`. O doc diz que o caminho
  depende do relógio. O guarda que não depende é o novo
  `a_conta_que_sobra_quando_a_instancia_para_sai_antes_do_parou`: o MOD
  escreve duas linhas a cada 4 ms. A primeira leva cada ficha, e a segunda é
  sempre segurada. O balde nunca junta a ficha que sobraria, o fim de cada
  volta o acha seco, e o teste exige que a última linha antes do `Parou` seja a
  conta, com `suprimidas >= 1`.
- Menor 3b: o teste renomeado na primeira passada foi dividido em dois.
  - `a_conta_das_seguradas_nao_espera_um_temporizador_mais_tarde` (o
    temporizador de um minuto) é o guarda do `min`.
  - `a_conta_das_seguradas_sai_no_fim_da_volta_de_uma_resposta_com_ficha`
    entrega a resposta 350 ms depois da rajada, entre a primeira ficha e a
    segunda, e exige a conta antes do diagnóstico que vem atrás. Um atraso da
    máquina não o reprova; só o faz deixar de distinguir os caminhos.

  **Conferido por reversão: o guarda de escrita contínua não exercita a conta
  do fim da volta.** Sem ela, ele continua verde, e quem reprova é o teste da
  resposta.
- `a_conta_das_seguradas_sai_sem_esperar_o_mod_parar` não mudou. Ele dá 1 s
  para os ~500 ms.
- Menor 2: `const _: () = assert!(FICHA.is_multiple_of(LINHAS_DE_CONSOLE_POR_SEGUNDO as u64));`,
  junto de `FICHA`. Usei `is_multiple_of` porque o clippy recusa o `%` com
  `manual_is_multiple_of`. Prova: com a vazão em 3, a compilação falha com
  `error[E0080]: evaluation panicked: assertion failed: FICHA.is_multiple_of(…)`.
- Docs: o do balde, o de `pendentes_se_couber`, o de `dizer_as_seguradas`, os
  comentários da espera e do ramo do timeout, e `docs/como-se-faz-um-mod.md`
  («em até meio segundo… e deixa ao seu código a vez seguinte»).

**RED** (o código de `74a5c427`, com o guarda novo e o teste do laço
original):
```
um MOD que escreve a cada 100 ms deixou, em dois segundos depois de uma rajada, 1 linhas dele e 6 da conta: a linha da conta toma a ficha que era do MOD, e o seele.log diz «quantas» no lugar de «o quê»
um_mod_em_laco_no_console_nao_passa_da_rajada_e_conta_o_resto: veio None no meio das linhas de console
test result: FAILED. 0 passed; 2 failed
```

**GREEN.**
- `-- executor`: `50 passed`.
- O binário inteiro: `251 passed`.
- `mod_de_referencia`: `4 passed`.
- Os oito testes do balde e da conta, rodados dez vezes seguidas: `8 passed`
  nas dez.

**Reversões.**
1. `FICHA` no lugar de `2 * FICHA` (a espera da primeira versão): reprova o
   guarda novo («0 linhas dele e 7 da conta»), o teste do laço («veio None…») e
   o do balde (`left: Some(250ms) right: Some(500ms)`). Resultado:
   `47 passed; 3 failed`.
2. Sem a conta da parada: `a última linha antes do Parou não foi a conta… ("a",
   suprimidas=87)`. O de rajada e silêncio também reprova. A primeira versão
   do teste da parada descartava as linhas de antes do `Encerrar` e, na
   reversão, reprovava pelo motivo errado («nenhuma linha de console
   chegou…»). Corrigi para guardar a última linha do começo ao fim, e o motivo
   passou a ser o certo.
3. Sem a conta do fim de volta (a chamada tirada do fim do laço): só
   `a_conta_das_seguradas_sai_no_fim_da_volta_de_uma_resposta_com_ficha`
   reprova («a conta… não saiu em aviso (no fim da volta de uma resposta…)»),
   com `49 passed; 1 failed`.
4. O prazo com `+ 1` µs: `left: Some(500.001ms) right: Some(500ms)`.

### Importante 2: a segunda passada não tinha orçamento (`8719c9ee`)

**O conserto.** `semCiclo` virou `passada(marcaCiclos)`, e as duas passadas
de `emTexto` usam o mesmo substituto, com e sem a marca de ciclos. Cada
passada conta os objetos que vê. Do de número `OBJETOS_POR_PASSADA = 1024` em
diante, ela devolve `'[…]'`. A conta pega todo valor que vira objeto depois de
`semMentir`, e não só os expandidos: o que vira «[cíclico]» também conta, e
por isso a busca entre os ancestrais também tem teto (no máximo 1024 buscas
de no máximo 1024 degraus).

**Por que 1024, e nas duas passadas.**
- Cada objeto que a passada vê escreve ao menos um caractere (`{`, `[` ou o
  texto que o troca) antes de ela ver o seguinte, e o JSON sai nessa ordem.
- O objeto de depois do teto começa além do caractere 1024, e o Rust corta a
  linha em 512, contados já escapados — o que, num JSON cheio de aspas, é
  menos ainda.
- Os 512 caracteres que chegam ao `seele.log` não mudam; **o número do aviso
  do corte, sim**. O aviso conta o texto inteiro que o `console` montou, e
  acima de 1024 objetos esse texto tem `[…]` no lugar do resto, e sai menor do
  que o objeto inteiro daria. 512 já bastaria, e 1024 é a folga. *(Corrigido
  na rodada 2: a primeira versão dizia que o orçamento não mudava nada do que
  chega ao registro.)*
- Na primeira passada, ele fecha o mesmo peso num DAG, que não tem ciclo.
  Antes do lote (`0f94be66`), um DAG de 20 nós segurava a thread por 1,13 s e
  saía `[object Object]`, mas o MOD seguia sem `Falhou` (medido na
  re-revisão). Com as duas passadas deste lote e sem o orçamento na primeira,
  ele derruba a volta (a reversão 2, abaixo). *(Corrigido na rodada 2: a
  primeira versão dizia que o DAG «já estourava».)*
- As duas juntas veem no máximo 2048 objetos. É aqui que entra o custo da
  passada dobrada (a minha preocupação 5 de antes).

**Teste.** Novo `um_grafo_denso_no_console_nao_estoura_a_volta`:
- um grafo de 10 nós ligados todos a todos (ciclos, segunda passada);
- um DAG de 20 nós, cada um ligado a todos os de depois (2¹⁹ caminhos,
  primeira passada);
- depois, `seele.postar('fim')`.

Exige que a mensagem `fim` chegue (o código depois do `console.log` rodou) e
as duas linhas com o começo certo
(`{"i":0,"n1":{"i":1,"n0":"[cíclico]"…` e `{"i":0,"n1":{"i":1,"n2":{"i":2,…`).

**RED:**
```
um `console.log` de um grafo denso não deixou o MOD chegar ao fim da volta (758.965542ms depois de subir, com 0 linhas): veio Some(Falhou("o MOD lançou"))
```

**GREEN.**
- `-- executor`: `51 passed`.
- O binário inteiro: `252 passed`.
- `mod_de_referencia`: `4 passed`.
- Os quatro testes do T1, dez vezes seguidas: verdes nas dez.

Uma sonda temporária (removida depois) mediu o tempo de um `console.log`,
dentro do motor, no build de teste:

| caso | tempo |
|---|---|
| completo, k = 8 | 5–6 ms |
| completo, k = 10 | 4 ms |
| completo, k = 14 | 4–5 ms |
| DAG, k = 20 | 5 ms |
| cadeia de 100 / 300 / 500 / 700 nós, com 2000 referências ao pai no fim (o pior caso da busca entre ancestrais) | 20 / 38 / 53 / 65 ms |

Uma cadeia de 1000 estoura a pilha do QuickJS nas duas passadas, e sai
`[object Object]` em 55 ms. Isso é anterior ao lote: a primeira passada já
estourava ali.

**Reversões.**
1. Sem o orçamento (`if (false && …)`): `…(718.550167ms depois de subir, com 0
   linhas): veio Some(Falhou("o MOD lançou"))`.
2. O orçamento só na segunda passada (`if (marcaCiclos && …)`):
   `…(928.880708ms depois de subir, com 1 linhas): veio Some(Falhou("o MOD
   lançou"))`. A linha do grafo completo sai, e o DAG estoura a primeira
   passada.

### Menor 1: o `Falhou` da conta além da espera ganha o sujeito (`20404362`)

`falhou_alem_da_espera(n)` dá
`"promessa rejeitada sem tratamento: e mais N além da espera"`. A linha do
registro continua `alem_da_espera(n)`. Teste novo:
`o_falhou_da_conta_alem_da_espera_diz_de_que_e_a_conta`, com
`RejeicoesDaVolta { guardadas: vec![], alem: 3 }`. Exige a linha e depois o
`Falhou`, nesta ordem e com estes textos.

**RED** (o código de antes, que é a reversão):
```
left: "e mais 3 rejeições sem tratamento além da espera"
right: "promessa rejeitada sem tratamento: e mais 3 além da espera"
```

**GREEN:** `9 passed` com os testes de rejeição; binário inteiro `253 passed`;
`mod_de_referencia` `4 passed`.

### O que rodou nesta rodada, e o que não rodou

Rodou, depois de cada conserto:
- `cargo test -p seele-app --bin seele-app` inteiro;
- `cargo test -p seele-conformance --test mod_de_referencia`;
- `cargo fmt --all -- --check`;
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`
  (limpo; só o `ld: duplicate -rpath`).

Não rodou o mesmo que antes: `--workspace`, `check-api`, `check-runtime`, as
bancadas, o app nativo.

### Preocupações desta rodada

1. **O fim de volta continua pedindo uma ficha só.** Um MOD que escreve num
   temporizador e também recebe respostas sem escrever nelas ainda pode
   perder uma linha para a conta dita no fim de uma volta de resposta. É o
   comportamento de antes do lote (`sem a espera da ficha` na tabela), e não
   foi medido com respostas misturadas.
2. **Quem se cala tem a conta em ~500 ms**, e não mais em ~250 ms.
3. O teste da resposta só distingue os dois caminhos se o `sleep` de 350 ms
   não passar de 500 ms. Ele nunca reprova à toa, mas, numa máquina muito
   carregada, a reversão 3 pode ficar verde.
4. **O orçamento conta objetos, e não primitivos.** Um objeto com um milhão
   de campos primitivos continua custando um milhão de chamadas do
   substituto na primeira passada. Isso é anterior ao lote.
5. **O pior caso medido do orçamento é de 65 ms no build de teste**, para uma
   cadeia de 700 nós. Ele cabe nos 500 ms da volta, mas não é de graça.
6. Continuam valendo as preocupações 3 e 4 de antes (o corte do `Falhou`
   conta a frase do produto; a gestão sabe da rejeição no fim da volta). A 5
   (o getter chamado duas vezes) agora tem o teto do orçamento, mas o getter
   continua sendo chamado duas vezes.

## Rodada 2

A re-revisão da rodada 1 deu tudo resolvido, e refez a medida (o 1/14 voltou a
15/0). O controlador pediu para fechar mais um defeito no mesmo mecanismo — a
minha preocupação 1 da rodada 1, medida — e duas correções de texto. Mesmo
branch, a partir de `750c866a`. Um commit de código (`047c40ea`) e este
relatório.

### 1. O fim de uma volta em que nenhuma linha tentou passar pede duas fichas (`047c40ea`)

**O defeito.** O fim de volta pedia uma ficha só, com a justificativa de que
«o MOD acabou de ter a vez». Isso só vale para a volta em que ele escreveu. Um
MOD com ouvinte de eventos que escreve num temporizador recebe muitas voltas
sem escrever, e cada uma dizia a conta com a primeira ficha que achasse. A
linha seguinte do MOD achava o balde seco. O defeito vem de antes do lote
(`7953a3b0`), no mesmo balde e no mesmo laço.

**O conserto.**
- `BaldeDoConsole` ganha `escreveu_nesta_volta`. `admitir` o liga, e
  `pendentes_se_couber` (o fim de volta) o lê e o desliga.
- Numa volta em que uma linha tentou passar — do MOD, ou dita pelo produto por
  ele —, a regra de antes continua: uma ficha. Se a linha passou, levou a
  conta; se foi segurada, a ficha que se completou depois, ainda na volta,
  chegou tarde para ela.
- Numa volta em que nenhuma tentou, duas fichas, a regra da espera
  (`ate_sobrar_uma_ficha`). O prazo da espera é exatamente o instante em que o
  fim de uma volta sem linha acha as duas.
- `gastar_ficha` foi dividido em `recarregar` e o gasto, para o fim de volta
  conferir duas fichas e gastar uma.
- Os docs de `pendentes_se_couber`, de `ate_sobrar_uma_ficha` e do balde
  dizem a regra nova, e não mais «o MOD acabou de ter a vez».

**A medida** (3 rodadas de 4 s, depois de uma rajada de 100, linhas do MOD /
linhas da conta; eventos `{"tipo":"evento"}` para um `SeeleUI.aoEvento` que não
escreve; o MOD escreve a cada 100 ms; sonda removida depois):

| | eventos a cada 30 ms | eventos a cada 70 ms |
|---|---|---|
| antes (`750c866a`) | 5/10, 1/14, 2/13 | 4/11, 4/11, 6/9 |
| depois (`047c40ea`) | 15/0, 15/0, 15/0 | 15/0, 15/0, 15/0 |

Na mesma sonda, depois do conserto, o temporizador sozinho não mudou: P = 100
ms deu 15/0 ×3, e P = 16 ms deu 15/0, 16/0, 15/0. Um MOD que escreve três
linhas a cada 400 ms dá 10/5, 9/5, 10/4: o produto usa as vagas que sobram
entre as rajadas dele. É o que o guia agora diz (item 2).

**Testes.**
- Novo `um_mod_que_recebe_mais_do_que_escreve_nao_perde_a_vez_para_a_conta`:
  uma rajada de 100, `setInterval(() => console.log('do MOD'), 100)`, e um
  evento a cada 30 ms por 2 s. Exige no máximo uma linha de conta, mais linhas
  do MOD que da conta, e pelo menos quatro do MOD.
- `a_conta_das_seguradas_sai_no_fim_da_volta_de_uma_resposta_com_ficha`
  **deixou de valer**: ele provava que a resposta, sem escrever, dizia a conta
  com uma ficha só — o defeito. Virou
  `o_fim_de_uma_volta_em_que_o_mod_nao_escreveu_espera_sobrar_uma_ficha`: a
  resposta chega aos 260 ms, entre a primeira ficha e a segunda, e o teste
  exige que a conta **não** tenha saído quando o diagnóstico de trás volta, e
  que saia depois, quando sobra a segunda. A folga é de 240 ms.
- Novo `o_fim_de_uma_volta_em_que_o_mod_escreveu_diz_a_conta_com_uma_ficha`:
  a rajada e uma espera ocupada de 300 ms na mesma volta. No fim há uma ficha
  e um quinto, e a conta sai antes do diagnóstico de trás. Este é o guarda do
  caminho que fica com uma ficha.
- `o_balde_devolve_a_conta_que_nenhuma_linha_levou` foi reescrito na regra
  nova. Uma volta sem linha com uma ficha não diz a conta; com duas, diz e
  deixa uma ao MOD; numa volta com a linha do MOD segurada, a ficha que se
  completa depois basta.
- `o_balde_diz_quanto_falta_para_sobrar_uma_ficha` termina a volta da rajada
  antes de medir. Um microssegundo antes do prazo, o fim de uma volta sem linha
  não diz a conta; no prazo, diz, e deixa a ficha do MOD.

**RED** (o código de `750c866a`, com os guardas novos):
```
o_fim_de_uma_volta_em_que_o_mod_nao_escreveu_espera_sobrar_uma_ficha: a volta de uma resposta que não escreveu disse a conta com uma ficha só: a linha seguinte do MOD acha o balde seco, e o seele.log diz «quantas» no lugar de «o quê» (left: Some(Aviso), right: None)
um_mod_que_recebe_mais_do_que_escreve_nao_perde_a_vez_para_a_conta: … deixou, em dois segundos depois de uma rajada, 0 linhas dele e 7 da conta: o fim da volta de cada evento toma a ficha que era do MOD
o_fim_de_uma_volta_em_que_o_mod_escreveu_diz_a_conta_com_uma_ficha ... ok
test result: FAILED. 1 passed; 2 failed
```
O de «escreveu» passa no código de antes porque prende o caminho que fica. O
vermelho dele é o da reversão 2.

**GREEN.**
- `-- executor`: `54 passed`.
- O binário inteiro: `255 passed`.
- `mod_de_referencia`: `4 passed`.
- Os 13 testes do balde, da conta e da espera, dez vezes seguidas: `13 passed`
  nas dez.

**Reversões.**
1. A regra de hoje (`if escreveu || true`, uma ficha sempre) reprova quatro
   testes:
   - o guarda dos eventos («0 linhas dele e 7 da conta»);
   - o da resposta que não escreveu («disse a conta com uma ficha só»);
   - `o_balde_devolve…` («o fim de uma volta sem linha disse a conta com uma
     ficha só»);
   - `o_balde_diz…` («um microssegundo antes do prazo, o fim de uma volta sem
     linha disse a conta…»).

   Resultado: `50 passed; 4 failed`.
2. Duas fichas sempre (`if escreveu && false`) reprova dois:
   - `o_fim_de_uma_volta_em_que_o_mod_escreveu_diz_a_conta_com_uma_ficha`
     («a conta… não saiu em aviso (no fim de uma volta em que o MOD
     escreveu…)»);
   - `o_balde_devolve…` («numa volta em que a linha do MOD foi segurada, a
     ficha que se completou depois não disse a conta»).

   Resultado: `52 passed; 2 failed`.

Depois da reversão 1, a mensagem do `o_balde_diz…` foi ampliada para nomear
as duas causas possíveis (o prazo inexato, ou a volta sem linha que não pede
duas fichas).

### 2. Os docs prometiam mais do que o código faz

- `docs/como-se-faz-um-mod.md` dizia que «quem continua escrevendo leva a
  conta na própria linha». Agora diz a regra de verdade:
  - o produto só escreve a conta com uma vaga de sobra no teto;
  - quem tenta escrever ao menos uma linha a cada quarto de segundo leva a
    conta na própria linha;
  - quem escreve mais espaçado (várias de uma vez, a cada meio segundo) pode
    ver a linha do produto entre as suas.

  A medida «3 linhas a cada 400 ms», acima, é esse caso.
- `executor.rs`: os dois «o MOD acabou de ter a vez» (o doc de
  `pendentes_se_couber` e o de `ate_sobrar_uma_ficha`) foram reescritos com a
  regra do item 1.

### 3. O orçamento muda o número do aviso do corte

O comentário do orçamento no `PRELUDIO` agora diz que os 512 caracteres que
chegam ao `seele.log` não mudam, mas o número do aviso do corte muda: ele
conta o texto inteiro que o `console` montou, e acima de 1024 objetos esse
texto tem `[…]` no lugar do resto. A mesma frase foi corrigida na rodada 1
deste relatório.

### 4. O que o DAG fazia antes do lote

O doc de `um_grafo_denso_no_console_nao_estoura_a_volta` e o comentário do
orçamento diziam que o DAG «já estourava». A frase agora diz o que a
re-revisão mediu em `0f94be66`: o DAG de 20 nós segurava a thread por 1,13 s e
saía `[object Object]`, e o MOD seguia sem `Falhou`. Com as duas passadas
deste lote e sem o orçamento na primeira, ele derruba a volta (a reversão 2
da rodada 1). Corrigido também na rodada 1 deste relatório.

### O que rodou nesta rodada, e o que não rodou

Rodou:
- `cargo test -p seele-app --bin seele-app` inteiro (`255 passed`);
- `cargo test -p seele-conformance --test mod_de_referencia` (`4 passed`);
- `cargo fmt --all -- --check`;
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`
  (limpo; só o `ld: duplicate -rpath`).

Não rodou o mesmo de antes: `--workspace`, `check-api`, `check-runtime`, as
bancadas e o app nativo. Não medi a base `0f94be66` com a sonda de eventos; a
re-revisão a mediu (1/14 ×3).

### Preocupações desta rodada

1. **Uma linha dita pelo produto em nome do MOD também conta como «tentou
   passar».** Por exemplo, a de um ouvinte que lança, ou a de uma rejeição. Uma
   volta de evento cujo ouvinte lança volta à regra de uma ficha. Não medi
   esse caso.
2. **O guarda da resposta que não escreveu tem 240 ms de folga.** Uma máquina
   parada mais que isso entre o `sleep` e o diagnóstico deixaria a espera
   dizer a conta antes dele, e o teste reprovaria à toa. Ele passou dez vezes
   seguidas aqui.
3. **Quem escreve mais espaçado que um quarto de segundo** — três linhas a
   cada 400 ms — vê a linha do produto ocupar as vagas que sobram entre as
   rajadas dele (10/5 em 4 s). É a regra das duas fichas, e não um roubo de
   vez, mas o `seele.log` desse MOD tem uma linha de conta a cada duas dele.
4. Continuam valendo as preocupações 4 a 6 da rodada 1.

## Rodada 3

A re-revisão da rodada 2 deu a regra dos eventos como resolvida (refez a
medida: 15/0) e concluiu que a linha dita em nome do MOD não reabre o defeito
(a minha preocupação 1 da rodada 2). Mas ela mediu uma brecha no ramo de uma
ficha que a rodada 2 manteve, e essa brecha deixava os docs falsos. A decisão
do controlador foi apagar o ramo. Mesmo branch, a partir de `9b4b36c9`. Um
commit de código (`b7ac8fc7`) e este relatório.

### A brecha, e o conserto (`b7ac8fc7`)

**A brecha.** Na volta em que o MOD escreveu, a conta continuava com uma
ficha. Se ela se completava depois da linha segurada, ainda na mesma volta —
o MOD escreve e segue trabalhando —, a conta a tomava, e a próxima linha do
MOD achava o balde seco. A justificativa da rodada 2 («a ficha que se
completou depois chegou tarde para ela e não é de ninguém») estava errada: ela
era da próxima linha do MOD.

**O conserto.**
- O fim de toda volta exige duas fichas para dizer a conta, a regra da
  espera. `pendentes_se_couber` recarrega, confere duas e gasta uma.
- A marca `escreveu_nesta_volta` sai, com o ramo.
- Com a regra única, toda conta que o produto diz com o MOD de pé deixa uma
  ficha para ele. Quem tenta escrever ao menos uma linha a cada quarto de
  segundo nunca deixa juntar duas, e o produto não fala.

**A medida** (3 rodadas de 4 s, depois de uma rajada de 100, linhas do MOD /
linhas da conta; `setInterval` de 100 ms com `console.log` seguido de X ms de
trabalho no mesmo callback; mesma sonda antes e depois, removida):

| X | antes (`9b4b36c9`) | depois (`b7ac8fc7`) |
|---|---|---|
| 5 ms | 15/0, 14/1, 12/3 | 15/0, 15/0, 15/0 |
| 20 ms | 15/0, 12/4, 10/5 | 15/0, 15/0, 15/0 |
| 60 ms | 9/6, 6/9, 10/5 | 15/0, 15/0, 15/0 |
| 120 ms | 0/15, 0/15, 0/15 | 15/0, 15/0, 15/0 |

Na mesma sonda, depois do conserto, os eventos continuam em 15/0 (a cada 30
ms e a cada 70 ms, ×3), e um MOD calado tem a conta em 514 ms.

**Testes.**
- Novo `um_mod_que_escreve_e_trabalha_na_mesma_volta_nao_perde_a_vez_para_a_conta`:
  o caso de X = 60 ms, com `console.log` e trabalho no mesmo callback, por 3 s.
  Exige no máximo uma linha de conta, mais linhas do MOD que da conta, e pelo
  menos seis do MOD.
- `o_fim_de_uma_volta_em_que_o_mod_escreveu_diz_a_conta_com_uma_ficha` virou
  `o_fim_de_uma_volta_em_que_o_mod_escreveu_tambem_espera_sobrar_uma_ficha`:
  a rajada e 300 ms de trabalho na mesma volta. No fim dela há uma ficha e um
  quinto, e a conta **não** sai no diagnóstico de trás; ela sai depois, quando
  sobra a segunda.
- `o_balde_devolve_a_conta_que_nenhuma_linha_levou`: as asserções do ramo de
  uma ficha foram reescritas. Numa volta com a linha do MOD segurada, a ficha
  que se completa depois não diz a conta. Com duas, diz, e deixa a do MOD.
- **O relógio conferido** (o opcional), nos dois testes de fim de volta. Em vez
  de supor a folga, eles medem o tempo desde antes de `iniciar` — o mais cedo
  que a rajada pode ter secado o balde — e só afirmam `conta == None` se o
  diagnóstico voltou antes de 480 ms (`AINDA_SEM_A_SEGUNDA_FICHA`). Antes de
  meio segundo, a segunda ficha não pode ter se completado. Uma tentativa lenta
  é refeita, e três lentas seguidas reprovam dizendo que nada foi medido: o
  teste não passa calado nem reprova à toa.

**RED** (o código de `9b4b36c9`, com o guarda novo):
```
um MOD que escreve e trabalha 60 ms a cada 100 ms deixou, em três segundos depois de uma rajada, 4 linhas dele e 7 da conta: o fim da volta dele toma a ficha que se completou durante o trabalho, e que era da próxima linha
test result: FAILED. 0 passed; 1 failed
```

**GREEN.**
- `-- executor`: `55 passed`.
- O binário inteiro: `256 passed`.
- `mod_de_referencia`: `4 passed`.
- Os 14 testes do balde, da conta e da espera, dez vezes seguidas: `14 passed`
  nas dez.

**Reversões.**
1. **O ramo de uma ficha de volta** — a marca `escreveu_nesta_volta` em
   `admitir`, e `if escreveu { FICHA } else { 2 * FICHA }` no fim de volta,
   como na rodada 2. Reprovam três:
   - o guarda novo («7 linhas dele e 5 da conta…»);
   - `o_fim_de_uma_volta_em_que_o_mod_escreveu_tambem_espera_sobrar_uma_ficha`
     («…disse a conta com uma ficha só: a ficha que se completou durante o
     trabalho era da próxima linha do MOD»);
   - `o_balde_devolve…` («numa volta em que a linha do MOD foi segurada, a
     conta tomou a ficha que se completou depois…»).

   Resultado: `52 passed; 3 failed`.
2. **Uma ficha sempre** (a regra de antes do lote) reprova seis:
   - os três de cima;
   - o guarda dos eventos;
   - `o_fim_de_uma_volta_em_que_o_mod_nao_escreveu_espera_sobrar_uma_ficha`;
   - `o_balde_diz_quanto_falta_para_sobrar_uma_ficha`.

   Resultado: `49 passed; 6 failed`.

### Os docs, conferidos um a um

- `docs/como-se-faz-um-mod.md` (a frase das seguradas).
  - «Ele só a escreve com uma vaga de sobra no teto» e «o produto não fala»
    agora são verdade em todo caminho.
  - Pus «enquanto o MOD está de pé», porque na parada a conta sai de qualquer
    jeito, e acrescentei essa frase.
- `executor.rs`:
  - o doc do balde: só com uma ficha de sobra, ao fim da volta ou pela
    espera; na parada, sem regra de ficha;
  - o título de `pendentes_se_couber`: «só com uma ficha de sobra além da que
    fica com o MOD»;
  - o «não é de ninguém» saiu. O doc agora conta os dois caminhos por onde a
    ficha única tomava a vez do MOD, os dois medidos;
  - o doc de `ate_sobrar_uma_ficha`: «o fim de toda volta pede as mesmas
    duas»;
  - o comentário do fim do laço (era «se houver ficha»);
  - o doc de `a_conta_das_seguradas_nao_espera_um_temporizador_mais_tarde`:
    «a da resposta é o teste de baixo» deixou de valer, e agora diz no que a
    metade da resposta virou;
  - `cortar_linha_do_console`: «o aviso do corte conta os caracteres que o MOD
    escreveu» agora diz que conta o texto que chegou ali (o do MOD, ou o que o
    `console` montou de um objeto), e que isso só vale até 1024 objetos —
    acima, o texto montado tem `[…]` no lugar do resto.

### O que rodou nesta rodada, e o que não rodou

Rodou:
- `cargo test -p seele-app --bin seele-app` inteiro (`256 passed`);
- `cargo test -p seele-conformance --test mod_de_referencia` (`4 passed`);
- `cargo fmt --all -- --check`;
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`
  (limpo; só o `ld: duplicate -rpath`).

Não rodou o mesmo de antes: `--workspace`, `check-api`, `check-runtime`, as
bancadas e o app nativo.

### Preocupações desta rodada

1. **Uma tentativa lenta é refeita em silêncio.** Os dois testes de fim de
   volta refazem a tentativa que passou de 480 ms e não dizem quantas vezes
   refizeram. Numa máquina sempre lenta, eles reprovam («três tentativas
   seguidas…»); numa máquina às vezes lenta, passam pela segunda, sem rastro.
2. **A conta de quem escreve espaçado** (três linhas a cada 400 ms) continua
   como na rodada 2: o produto usa as vagas que sobram entre as rajadas dele.
   Os docs dizem isso.
3. Continuam valendo as preocupações 4 a 6 da rodada 1.
