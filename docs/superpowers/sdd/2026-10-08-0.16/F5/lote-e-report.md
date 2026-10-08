# Lote E da F5: relatório

Base `3b0b6a7a` (branch `0.16/lotes`). Branch `0.16/lotes-e`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a69f343be8082b408`. Dois
commits de código e este relatório, todos locais: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `bece2bd9` | P51-09 | `Session::recusar_se_morta`; as treze conferências `if !session.geracao_vale(` passam por ela; `liberar_reserva` recebe `mortos` e conta; a ressalva do doc de `registrar_reserva` sai |
| `9abe0678` | P51-11 | `imagem_lida_dita` leva as duas recusas de depois do `await` de `ler_imagem_mod`; a conexão ausente vai a `recusa_de_midia_dita` em DEBUG; os dois guardas de fonte da mídia acompanham; o doc de `recusa_de_midia_dita` perde o parágrafo da geração morta |

Arquivos mudados (`3b0b6a7a..9abe0678`): só `apps/seele-app/src/main.rs`.
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
3. **O contador sobe mais** (o risco que o brief já nomeia): sete caminhos e a
   ativação passam a contar. Ele só aparece no modo de desenvolvedor.
4. **`reservar_no_executor` ficou como estava.** Ele usa
   `session.confere_geracao(geracao).is_err()` e monta o `sessao-encerrada` à
   mão, o que hoje é exatamente `recusar_se_morta`. Não está nas treze, conta e
   diz, e o guarda (b) não o vê (não é `!….geracao_vale(`). Trocá-lo é estético.
5. **O guarda (b) pega a negação, e não toda forma de conferir.** Um
   `if session.geracao_vale(g) { … } else { return Err(…) }` ou um `match`
   escapam dele. A contagem positiva por função cobre o caso de a conferência
   sumir dos dez lugares conhecidos; uma conferência nova, num comando novo,
   escrita sem negação, não é vista.
6. **Os dois primeiros vermelhos de função nova foram de compilação**
   (`recusar_se_morta` e `imagem_lida_dita`). Para que o motivo certo
   aparecesse em tempo de execução, cada um teve um segundo vermelho com a
   função sem a parte que o teste mede (sem a conta, sem as duas chamadas),
   como está acima.

## Para a v6

Nada. O lote não cria evento, campo do retrato nem frase no `api/README.md`; o
que mudou é a contagem de um contador de diagnóstico e linhas do `seele.log`,
que não são promessa da API. A capacidade `volume` segue como estava.
