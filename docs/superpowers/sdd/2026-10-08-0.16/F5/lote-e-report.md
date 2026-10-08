# Lote E da F5: relatório

Base `3b0b6a7a` (branch `0.16/lotes`). Branch `0.16/lotes-e`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a69f343be8082b408`. Dois
commits de código, três de fecho depois da revisão e este relatório, todos
locais: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `bece2bd9` | P51-09 | `Session::recusar_se_morta`; as treze conferências `if !session.geracao_vale(` passam por ela; `liberar_reserva` recebe `mortos` e conta; a ressalva do doc de `registrar_reserva` sai |
| `9abe0678` | P51-11 | `imagem_lida_dita` leva as duas recusas de depois do `await` de `ler_imagem_mod`; a conexão ausente vai a `recusa_de_midia_dita` em DEBUG; os dois guardas de fonte da mídia acompanham; o doc de `recusa_de_midia_dita` perde o parágrafo da geração morta |
| `a2d86649` | fecho 1–4 | `recusa_do_servidor` passa por `imagem_lida_dita`; a conta de `recusar_se_morta` fora dos testes tem de ser igual à da lista do guarda (b); a conexão ausente de `ler_imagem_mod` não pode contar; o fim de `impl Session` que falta reprova |
| `0b6b9193` | fecho 5 | `reservar_no_executor` confere por `recusar_se_morta`, e entra na lista do guarda |
| `06256c27` | fecho 6 | `liberar_reserva` diz a recusa por geração morta com a linha DEBUG de `confere_geracao` |

A seção «Fecho», no fim, conta o que mudou depois da revisão. Ela reescreve
as preocupações 2, 4 e 5.

Arquivos mudados (`3b0b6a7a..06256c27`): só `apps/seele-app/src/main.rs`.
Nada no fio (`seele-proto`), no SEELE-ENC/1, nos arquivos da F6, em `api/`, na
CSP, em `ui/` ou nas bancadas. Nenhuma dependência nova, nada baixado. A frase
da tela (`camada-mods.js`, o contador do modo de desenvolvedor) não mudou.
`docs/pendencias.md` e o índice da 1.0 não foram tocados.

As reversões foram feitas copiando o `main.rs` para o scratchpad, trocando o
trecho exato com um script de troca única, rodando os testes, devolvendo a
cópia e conferindo com `cmp`: todas voltaram idênticas.

## 1. P51-09: todo comando de uma geração morta é contado (`bece2bd9`)

**Produto.**
- **`Session::recusar_se_morta(&self, geracao) -> Result<(), FalhaNoMod>`**, no
  `impl Session`, logo depois de `confere_geracao`. Ela **é**
  `confere_geracao` com o erro dos comandos de MOD: a mesma conta em
  `comandos_de_geracao_morta`, a mesma linha DEBUG («comando recusado: a sessão
  dele já acabou», com `geracao` e `atual`) e `sessao-encerrada` de volta. Uma
  regra só, e não uma segunda cópia dela.
- **As treze conferências** viraram `session.recusar_se_morta(geracao)?;`:

  | função | contava antes? |
  |---|---|
  | `mod_nativo_colher` | não |
  | `mod_nativo_entregar` | não |
  | `ler_codigo_do_mod` (auxiliar) | sim, à mão |
  | `midia_do_mod` | sim, à mão |
  | `som_do_mod` | sim, à mão |
  | `midia_em_bytes` | sim, à mão |
  | `enviar_imagem_mod`, antes e depois do `await` | não |
  | `ler_imagem_mod`, antes e depois do `await` | não |
  | `escolher_para_o_mod`, a primeira | sim, à mão |
  | `escolher_para_o_mod`, a segunda, depois do seletor | não |
  | `pedaco_do_escolhido` | sim, à mão |

  Sete não contavam. Das seis que contavam à mão, só a de `ler_codigo_do_mod`
  chegava ao registro, e por quem a chama: `codigo_do_mod` a diz em DEBUG por
  `recusa_de_carga_dita`. As outras cinco não diziam nada.
- **`liberar_reserva(mods, geracao_vale, mortos, geracao, instancia)`**: o
  `mortos: &AtomicU64` entra na mesma posição que em `registrar_reserva`, e a
  recusa por geração morta faz `fetch_add` antes de encerrar a reserva.
  `mod_nativo_ativar` passa `&session.comandos_de_geracao_morta`.
- **O doc de `registrar_reserva`** perdeu a ressalva («A de [`liberar_reserva`],
  logo abaixo, não é contada…») e diz que as duas contam. O de
  `liberar_reserva` ganhou uma linha dizendo o mesmo.

**Testes** (todos em `mod a_supervisao_dos_mods_nativos`, e todos com
`geracao` no nome, para o filtro do brief).
- **(a)** `a_ativacao_recusada_por_geracao_morta_e_contada`, no molde de
  `a_reserva_recusada_por_geracao_morta_e_contada`: reserva uma instância viva,
  chama `liberar_reserva` com `|| false` e exige `sessao-encerrada`, `mortos`
  de 0 para 1 e a reserva em `encerrando`. Depois chama com `|| true` sobre a
  mesma (já encerrada) e exige `instancia-nao-esta-reservada` com `mortos`
  parado em 1: a que vale não é contada, nem quando é recusada por outro
  motivo.
- **(c)** `o_comando_de_uma_geracao_morta_e_recusado_contado_e_dito`:
  `Session::default()` com a geração em 3; `recusar_se_morta(2)` sob
  `rastro_de_teste::capturar` exige `sessao-encerrada`, o contador em 1 e a
  linha DEBUG de `confere_geracao` no rastro; `recusar_se_morta(0)` (o «a casca
  não sabe») sobe para 2; `recusar_se_morta(3)` passa e não conta.
- **(b)** `nenhuma_geracao_morta_e_recusada_por_fora_de_recusar_se_morta`, sobre
  `fonte_sem_comentarios()`:
  - corta fora o corpo de `impl Session` e tira o espaço em branco do resto;
  - procura toda chamada `.geracao_vale(` cujo receptor venha negado
    (`!<identificador>.geracao_vale(`), e não só `!session.`: um
    `!sessao.geracao_vale(` também reprova. O padrão é montado com `format!`,
    para o teste não se achar. Os fechos `&|| session.geracao_vale(geracao)` e
    os `!geracao_vale()` dos parâmetros não casam, e ficam;
  - **e conta o lado positivo**: cada uma das dez funções tem de ter as suas
    `.recusar_se_morta(geracao)` (duas em `enviar_imagem_mod`, `ler_imagem_mod`
    e `escolher_para_o_mod`), e o total tem de ser pelo menos treze. Sem isto,
    apagar uma conferência, em vez de trocá-la, deixava o guarda verde.
  - É o arquivo inteiro, e não os corpos de `#[tauri::command]`: a conferência
    de `ler_codigo_do_mod` aparece no vermelho abaixo (terceira linha), e a
    reversão R1 é ela.

**RED.**
- (a), com a assinatura nova e sem o `fetch_add`:
  ```
  assertion `left == right` failed: a ativação recusada por geração morta não foi
  contada em `comandos_de_geracao_morta`, e o contador diz que nada da sessão
  anterior falou
    left: 0
   right: 1
  test result: FAILED. 3 passed; 1 failed
  ```
- (c), primeiro sem a função (`error[E0599]: no method named
  'recusar_se_morta' found for struct 'Session'`), depois com uma
  `recusar_se_morta` que recusava sem passar por `confere_geracao`:
  ```
  assertion `left == right` failed: um comando recusado por geração morta não
  foi contado em `comandos_de_geracao_morta`, e o contador diz que nada da
  sessão anterior falou
    left: 0
   right: 1
  ```
- (b), antes das treze trocas:
  ```
  13 conferência(s) de geração recusam com um `if` próprio, e não por
  `recusar_se_morta`: o comando de uma geração morta volta à janela sem ser
  contado em `comandos_de_geracao_morta`, e o zero dele mente — [
      "size,)->Result<Vec<FalaPendente>,FalhaNoMod>{if!session.geracao_vale(",
      "cia:u64,json:String,)->Result<(),FalhaNoMod>{if!session.geracao_vale(",
      "hash:&str,)->Result<(String,u32),FalhaNoMod>{if!session.geracao_vale(",
      … (mais dez)
  ```

**GREEN.** `cargo test -p seele-app --bin seele-app geracao`: 6 passaram (eram
3 na base; os três novos são os acima).

**Reversões** (cada uma restaurada e conferida com `cmp`).
- **R1** — `ler_codigo_do_mod` de volta ao `if` cru, com a conta à mão (a que
  um guarda só de comandos não veria): (b) vermelho com `1 conferência(s)…` e
  `"hash:&str,)->Result<(String,u32),FalhaNoMod>{if!session.geracao_vale("`.
- **R2** — a segunda conferência de `escolher_para_o_mod` apagada: (b)
  vermelho com «`async fn escolher_para_o_mod(` confere a geração 1 vez(es) por
  `recusar_se_morta`, e conferia 2: um comando de uma geração morta passa a ser
  admitido».
- **R3** — o `fetch_add` de `liberar_reserva` trocado por `let _ = mortos;`:
  (a) vermelho, `left: 0, right: 1`.
- **R4** — `recusar_se_morta` conferindo com `geracao_vale` e sem passar por
  `confere_geracao`: (c) vermelho, `left: 0, right: 1`.

## 2. P51-11: `ler_imagem_mod` diz cada recusa (`9abe0678`)

**Produto.**
- **`fn imagem_lida_dita(baldes: &BaldesDoRegistro, agora: Instant, id: &str,
  lida: Result<Vec<u8>, String>) -> Result<MidiaDoMod, FalhaNoMod>`**, logo
  depois de `ler_imagem_mod`. A assinatura segue o bloco «Depois da onda 1» do
  brief (baldes e instante, como as outras portas); `Connection::ler_imagem_mod`
  devolve `Result<Vec<u8>, String>` (`seele-ffi/src/lib.rs`), e bate. As duas
  recusas são duas chamadas explícitas a `recusa_de_midia_dita` com a origem
  `volume`: a do servidor, com o texto dele como motivo, e
  `formato-desconhecido`.
- **`ler_imagem_mod`**, na ordem:
  1. `session.recusar_se_morta(geracao)?` — conta a geração morta;
  2. `session.connection()` que falha → `recusa_de_midia_dita(…, "volume",
     sessao-encerrada)`, que vai em DEBUG («mídia de MOD não servida: a sessão
     acabou») e gasta da classe `Depuracao`. **Não conta**: a geração foi
     conferida no passo 1, e uma conexão ausente com a geração de pé não é
     comando de sessão encerrada;
  3. o `await` da leitura, guardado inteiro em `lida`;
  4. `if lida.is_ok() { session.recusar_se_morta(geracao)?; }`;
  5. `imagem_lida_dita(&session.baldes_do_registro, Instant::now(), &id, lida)`.
- **A ordem escolhida é a de antes.** Um `Err` do servidor sai por
  `imagem_lida_dita` como veio, mesmo que a geração tenha morrido durante o
  `await`: é o dado da recusa, e a janela continua recebendo o texto do
  servidor. Só o caminho `Ok` passa pela segunda conferência, antes de
  decodificar. O `sessao-encerrada` que a conexão devolve quando cai no meio da
  leitura continua indo em DEBUG, pela regra de `recusa_de_midia_dita`.
- O relógio continua lido depois do `await`, na chamada da porta.
- `ler_imagem_mod` ganhou doc, com `# Errors`, dizendo por onde cada recusa
  passa.
- **O doc de `recusa_de_midia_dita`** perdeu o parágrafo «**A geração morta que
  os comandos conferem na entrada não passa por aqui.**…».

**Guardas de fonte.**
- `os_quatro_comandos_de_midia_passam_pela_recusa_dita` exige
  `imagem_lida_dita(` em `ler_imagem_mod` (era `recusa_de_midia_dita(`), e o doc
  diz por quê: procurar `recusa_de_midia_dita` ali passava com uma só das duas.
- `os_comandos_passam_os_baldes_da_sessao` ganhou `imagem_lida_dita(` nas
  `PORTAS` (8 → 9). **O mínimo de `ler_imagem_mod` fica em 2**, mas as duas
  passam a ser outras: a conexão ausente (`recusa_de_midia_dita`) e
  `imagem_lida_dita`. Sem a porta nova na lista, o mínimo cairia para 1 — foi o
  vermelho que o guarda deu quando a extração entrou (abaixo). O doc do guarda
  diz quais são as duas.
- O irmão em `mod o_som_do_mod_sai_em_bytes`
  (`os_comandos_de_midia_dizem_o_que_serviram_pela_funcao_medida`) continua
  verde.

**Testes de rastro** (em `mod a_midia_recusada_e_dita_no_registro`, um por
caso, para que tirar uma chamada reprove só o dela):
- `a_imagem_recusada_pelo_servidor_sai_como_warn_com_o_id_e_o_motivo`:
  `Err("Prazo esgotado")` → recusa com o motivo intacto, e linha com `WARN`,
  `mod_id=prova/midia`, `motivo="Prazo esgotado"` e `origem="volume"`;
- `a_imagem_lida_que_nao_e_midia_sai_como_warn_com_o_motivo`:
  `Ok(b"so texto")` → `formato-desconhecido`, linha com `WARN`, `mod_id=`,
  `motivo=formato-desconhecido` e `origem="volume"`;
- `a_imagem_lida_aceita_nao_escreve_linha`: `Ok(PNG)` → `Ok` com papel
  `imagem`, e o rastro vazio.

**RED.**
- O guarda dos comandos de mídia, antes de mexer no comando:
  ```
  `async fn ler_imagem_mod(` não passa por `imagem_lida_dita(`: a recusa dele
  volta à janela e não chega ao registro
  test result: FAILED. 0 passed; 1 failed
  ```
- Os testes de rastro: primeiro sem a função (`error[E0432]: unresolved import
  'super::imagem_lida_dita'`), depois com a extração sem as duas chamadas — os
  dois casos de recusa vermelhos e o do PNG verde:
  ```
  a imagem que o servidor recusou não chegou ao registro: o MOD fica sem imagem
  e o seele.log sem uma palavra —
  a imagem lida que não é mídia não chegou ao registro: o MOD fica sem imagem e
  o seele.log sem uma palavra —
  test result: FAILED. 9 passed; 2 failed
  ```
- O guarda dos baldes, com o comando já usando `imagem_lida_dita` e a lista de
  portas ainda velha:
  ```
  `async fn ler_imagem_mod(` chama 1 das portas medidas, e chamava 2: a linha
  dele saiu por fora delas, e do teto
  ```

**GREEN.** `… --bin seele-app imagem`: 11 passaram (eram 8; os três novos são
os de rastro). Os dois guardas de fonte e o irmão do som, verdes. `… geracao`
continua com 6.

**Reversões** (cada uma restaurada e conferida com `cmp`).
- **R5** — a chamada da recusa do servidor tirada de `imagem_lida_dita`: só
  `a_imagem_recusada_pelo_servidor…` vermelho. Os dois guardas de fonte
  **continuam verdes** com ela — é o buraco que o brief nomeia, e que só o teste
  de rastro pega.
- **R6** — a chamada de `formato-desconhecido` tirada: só
  `a_imagem_lida_que_nao_e_midia…` vermelho.
- **R7** — o corpo antigo, com as duas `recusa_de_midia_dita` escritas no
  comando, no lugar de `imagem_lida_dita`: o guarda dos comandos de mídia
  vermelho («não passa por `imagem_lida_dita(`»).
- **R8** — `&BaldesDoRegistro::default()` no lugar de
  `&session.baldes_do_registro` na chamada de `imagem_lida_dita`: o guarda dos
  baldes vermelho («chama `imagem_lida_dita(` sem
  `&session.baldes_do_registro`»).
- **R9** — a conexão ausente de volta à recusa crua: o guarda dos baldes
  vermelho («chama 1 das portas medidas, e chamava 2»).
- **R10** — o `if lida.is_ok() { recusar_se_morta }` apagado: o guarda (b) do
  P51-09 vermelho («`async fn ler_imagem_mod(` confere a geração 1 vez(es)…»).

## O que rodou

- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --bin seele-app`,
  inteiro, sobre o código do último commit de código: **265 passaram, 0
  falharam** (12,0 s). Na base eram 259; os seis novos são os deste lote.
- Os filtros do brief, `geracao` (6) e `imagem` (11), verdes, e os guardas de
  fonte pelo nome.
- `cargo fmt --all -- --check` limpo antes de cada commit.
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings` limpo
  antes de cada commit, e de novo com `touch` no `main.rs` para forçar a
  conferência. O único aviso é o `ld: duplicate -rpath` do linker, ambiental.
- `CARGO_BUILD_JOBS=8` em todos, com o `target/` semeado depois do checkout.

## O que não rodou aqui, e por quê

- `cargo test --workspace` e a bateria inteira: proibidos no lote; rodam na
  integração.
- O clippy de outros crates: só o `seele-app` mudou, e ele é um binário.
- `cargo xtask check-api` e o guarda de congelamento: `api/` não foi tocado.
- `cargo xtask check-runtime` e as bancadas de navegador: nada em `ui/` nem em
  `bancada/` mudou. Nenhum comando mexido mudou de argumento nem de retorno, e
  a recusa que a janela recebe é o mesmo `sessao-encerrada` de antes.
- Nada de `cfg(windows)` ou `cfg(target_os = "linux")` foi tocado.
- O app de verdade não foi aberto: as linhas foram conferidas pelo formatador
  do `tracing` nos testes (`rastro_de_teste::capturar`). Nenhum teste de
  unidade monta um `State`, então os comandos são provados pelas funções que
  chamam e pelos guardas de fonte.

## Preocupações

1. **O `Err` de depois do `await` não é contado, mesmo com a geração morta.**
   É a ordem que o brief pediu para manter: em `ler_imagem_mod`, o `Err` do
   servidor sai por `imagem_lida_dita` sem passar pela segunda conferência. Em
   `enviar_imagem_mod` é igual: o `map_err(|m| recusa(&m))?` devolve antes da
   conferência de depois do `await`. Os dois são dados à janela; o primeiro
   vai ao registro (WARN, ou DEBUG quando o texto é `sessao-encerrada`), o
   segundo não (fora do escopo, como o brief diz). Então «todo comando de uma
   geração morta é contado» vale para toda conferência que existe, e não para
   o `Err` que chega de uma leitura em voo quando a sessão acaba.
2. **A linha DEBUG de `confere_geracao` não tem teto de vazão.** Ela não passa
   por `linha_admitida`, porque `recusar_se_morta` não sabe o id do MOD. Antes,
   só a geração morta de `codigo_do_mod` chegava ao registro; agora as treze
   conferências escrevem, em DEBUG. Um MOD de uma sessão encerrada que
   continuasse chamando num laço escreveria uma linha por chamada, só com
   `RUST_LOG=debug`. Na prática a janela desmonta os MODs com a sessão. Se a
   revisão quiser teto, é passar o id e os baldes, o que muda as treze
   chamadas. E `codigo_do_mod` com a geração morta passa a deixar duas linhas
   DEBUG: a de `confere_geracao` e a de `recusa_de_carga_dita`, que já existia.
   *(Fecho: a ativação também escreve a linha agora; ela continua sem dizer o
   comando e sem teto, e fica como nota.)*
3. **O contador sobe mais** (o risco que o brief já nomeia): sete caminhos e a
   ativação passam a contar. Ele só aparece no modo de desenvolvedor.
4. ~~**`reservar_no_executor` ficou como estava.** … Trocá-lo é estético.~~
   *(Fecho, `0b6b9193`: ele passa por `recusar_se_morta`, com o mesmo
   comportamento, e entrou na lista do guarda.)*
5. **O guarda (b) pega a negação, e não toda forma de conferir.** Um
   `if session.geracao_vale(g) { … } else { return Err(…) }` ou um `match`
   escapam dele. A contagem positiva por função cobre o caso de a conferência
   sumir dos dez lugares conhecidos; uma conferência nova, num comando novo,
   escrita sem negação, não é vista. *(Fecho: uma conferência nova **por
   `recusar_se_morta`** fora da lista agora reprova, pela igualdade. A
   conferência nova escrita sem negação e sem `recusar_se_morta` continua fora
   do alcance, e fica como nota.)*
6. **Os dois primeiros vermelhos de função nova foram de compilação**
   (`recusar_se_morta` e `imagem_lida_dita`). Para que o motivo certo
   aparecesse em tempo de execução, cada um teve um segundo vermelho com a
   função sem a parte que o teste mede (sem a conta, sem as duas chamadas),
   como está acima.

## Para a v6

Nada. O lote não cria evento, campo do retrato nem frase no `api/README.md`; o
que mudou é a contagem de um contador de diagnóstico e linhas do `seele.log`,
que não são promessa da API. A capacidade `volume` segue como estava.

## Fecho

A revisão aprovou sem crítico nem importante, e pediu seis itens antes do
merge. Os três primeiros são o «existir não é funcionar» do `CLAUDE.md`: um
teste que mede uma cópia, uma soma que nunca falha sozinha e uma regra sem
guarda. Cada reversão abaixo foi feita como as de cima (cópia no scratchpad,
troca única, testes, cópia de volta e `cmp`), e todas voltaram idênticas.

### Item 1: `recusa_do_servidor` passa por `imagem_lida_dita` (`a2d86649`)

O ajudante chamava `recusa_de_midia_dita` direto, e o doc dele dizia «É o
caminho de `ler_imagem_mod`». Os quatro testes que passam por ele (linha
forjada, prazo, teto e pior caso) mediam uma cópia do caminho. Agora o corpo é
`imagem_lida_dita(&BaldesDoRegistro::default(), origem(), ID,
Err(motivo.to_owned()))`, com a recusa tirada por `expect`, e o doc diz por
quê.

**Antes e depois, com a mesma reversão R5** (a chamada da recusa do servidor
tirada de `imagem_lida_dita`), no módulo `a_midia_recusada_e_dita_no_registro`
inteiro:
- antes: `FAILED. 17 passed; 1 failed` — só
  `a_imagem_recusada_pelo_servidor_sai_como_warn_com_o_id_e_o_motivo`;
- depois: `FAILED. 13 passed; 5 failed` —
  `a_imagem_recusada_pelo_servidor…` («não chegou ao registro»),
  `o_texto_do_servidor_no_motivo_nao_forja_linha` (`left: 0, right: 1`
  linhas), `o_motivo_em_texto_livre_sai_entre_aspas` e
  `o_motivo_do_servidor_tem_teto` («a recusa não chegou ao registro»), e
  `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas` («a linha não traz
  `motivo="…"`»).

### Item 2: a conta de `recusar_se_morta` é a da lista (`a2d86649`)

O `total >= 13` nunca falhava sozinho: os mínimos por função já somavam 13.
Agora o guarda conta `.recusar_se_morta(` no arquivo inteiro, fora de
`impl Session` **e fora dos itens `#[cfg(test)]`**, e exige igualdade com a
soma da lista. Os testes saem da conta porque três deles chamam
`recusar_se_morta` (com 2, 0 e 3): contados, a igualdade seria 16 contra 13. O
corte (`sem_itens_de_teste`) vai, a partir de cada `#[cfg(test)]` na coluna
zero, até a `}` na coluna zero ou até a linha na coluna zero que termina em
`;` (a `const` do fonte). Errar o fim só deixa teste na conta, e a igualdade
reprova: ele falha fechado. A igualdade verde é a prova de que o corte tira as
três chamadas dos testes. A contagem por função passou a ser sem espaço e com
o mesmo padrão montado (`format!(".{}(", "recusar_se_morta")`), para o teste
não se achar.

- **Antes**, com uma 14ª chamada fora da lista
  (`session.recusar_se_morta(geracao)?;` no começo de `mod_nativo_ativar`): o
  guarda **verde**, `1 passed`.
- **Depois**, a mesma reversão:
  ```
  assertion `left == right` failed: há 14 chamada(s) de `recusar_se_morta` fora
  de `impl Session` e dos testes, e as funções da lista deste guarda somam 13:
  uma conferência ficou fora da lista, e apagá-la não reprovaria nada
    left: 14
   right: 13
  ```

### Item 3: a conexão ausente não pode contar (`a2d86649`)

`os_quatro_comandos_de_midia_passam_pela_recusa_dita` ganhou, depois do laço,
uma exigência sobre `ler_imagem_mod`, no corpo sem comentários e sem espaço:
- o trecho de `session.connection()` até o primeiro `)?;` não tem
  `comandos_de_geracao_morta`, `confere_geracao(` nem `recusar_se_morta(`, e
  tem `recusa_de_midia_dita(`;
- o corpo inteiro não tem `comandos_de_geracao_morta`: a conta só por
  `recusar_se_morta`.

A forma mais precisa era necessária: com só «não tem
`comandos_de_geracao_morta`», um `recusar_se_morta` a mais na conexão passava,
porque sobe a conta da função e a do arquivo juntas no guarda (b).

- **Antes**, com `let _ = session.recusar_se_morta(0);` dentro do `map_err` da
  conexão: os três guardas de fonte **verdes** (`59 passed` em `os_`, e o (b)
  verde).
- **R11a**, a mesma reversão, depois: «a conexão ausente de `ler_imagem_mod`
  passou por `recusar_se_morta(`: com a geração de pé, ela não é comando de
  uma sessão encerrada, e o contador diria que a sessão anterior falou sem ela
  ter falado».
- **R11b**, `session.comandos_de_geracao_morta.fetch_add(1, …)` no mesmo
  lugar: a mesma frase, com `comandos_de_geracao_morta`.

### Item 4: o fim de `impl Session` que falta reprova (`a2d86649`)

O `map_or("", …)` virou `expect("`impl Session` não fecha mais na coluna
zero: sem o fim dela, o guarda leria só o que vem antes, e uma conferência
crua depois dela passaria calada")`. **Sem reversão**: o `expect` só dispara
se não houver nenhuma `}` na coluna zero depois de `impl Session` no arquivo
inteiro, e produzir isso é apagar o arquivo. Fechar a `impl` com a chave
recuada não serve: o corte acha a `}` do item seguinte, e o guarda lê um item
a mais como se fosse da sessão, em vez de ler um vazio.

### Item 5: `reservar_no_executor` por `recusar_se_morta` (`0b6b9193`)

O comportamento é idêntico: `recusar_se_morta` é
`confere_geracao(geracao).map_err(|_| Recusado { "sessao-encerrada" })`, e o
código antigo era `if confere_geracao(geracao).is_err() { return
Err(Recusado { "sessao-encerrada" }) }`. A mesma conta, a mesma linha DEBUG e
o mesmo erro, que `mod_nativo_reservar` diz por `recusa_de_carga_dita` como
antes.

- **RED**, trocada a conferência e a lista ainda sem ela — o guarda do item 2
  a pediu sozinho: `há 14 chamada(s)… somam 13`, `left: 14, right: 13`.
- **GREEN** com `("fn reservar_no_executor(", 1)` na lista: `geracao`, 6
  passaram.
- **R12**, a conferência de volta ao `confere_geracao` à mão: «`fn
  reservar_no_executor(` confere a geração 0 vez(es) por `recusar_se_morta`, e
  conferia 1: um comando de uma geração morta passa a ser admitido».

### Item 6: a ativação recusada é dita (`06256c27`)

`liberar_reserva`, ao recusar por geração morta, conta, encerra a reserva,
**solta o cadeado** e escreve `tracing::debug!(geracao, instancia, "comando
recusado: a sessão dele já acabou")` — a frase de `confere_geracao`, com a
instância no lugar de `atual`, que a função não conhece (ela recebe só o fecho
`geracao_vale`). O cadeado sai antes da linha pela regra de
`dizer_as_contas_do_registro`: com ele na mão, uma escrita lenta seguraria a
supervisão de todos os MODs.

O teste virou `a_ativacao_recusada_por_geracao_morta_e_contada_e_dita`: as
duas chamadas a `liberar_reserva` sob `capturar`. A da geração morta exige a
linha com `DEBUG`, `geracao=7` e `instancia=<n>`; a viva, recusada por
`instancia-nao-esta-reservada`, exige o rastro sem ela.

- **RED**: «a ativação recusada por geração morta foi contada e não foi dita:
  os outros comandos de MOD deixam a linha DEBUG de `confere_geracao`, e esta
  some do registro —» (rastro vazio).
- **GREEN**: `geracao`, 6 passaram.
- **R13**, a linha tirada: a mesma frase do RED.

### O que rodou no fecho

- `cargo test -p seele-app --bin seele-app` inteiro, sobre o código de
  `06256c27`: **265 passaram, 0 falharam** (12,2 s). A contagem não muda: o
  fecho reescreveu e renomeou testes, e não criou nenhum.
- `cargo fmt --all -- --check` limpo, e `cargo clippy -p seele-app
  --all-targets --all-features -- -D warnings` limpo com `touch` no `main.rs`,
  antes de cada um dos três commits.

### Fica de fora, como nota

- **O guarda (b) não vê `if … else`, `match` nem receptor com caminho.** Ele
  procura `!<identificador>.geracao_vale(`: `if session.geracao_vale(g) {} else
  { return … }`, um `match` e `!self.inner.geracao_vale(` escapam. A igualdade
  do item 2 pega a conferência nova **por `recusar_se_morta`**, e não a escrita
  por fora dela sem negação.
- **A linha DEBUG de `confere_geracao` não diz o comando nem passa pelo teto
  de vazão.** `recusar_se_morta` não recebe o nome do comando nem o id do MOD.
  A linha da ativação também não passa pelo teto.
- **A segunda conferência de `enviar_imagem_mod` vem depois do upload.** Com a
  geração morta durante o `await`, os bytes já foram ao servidor quando ela
  recusa. A recusa conta e é dita, mas o efeito no servidor já aconteceu.
