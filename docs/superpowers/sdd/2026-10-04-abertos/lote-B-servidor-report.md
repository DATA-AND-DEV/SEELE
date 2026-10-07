# Lote B-servidor — relatório

Base `ec5134d`, branch `conserto/abertos-da-1.0`. HEAD depois do lote: `b2dd6d1`. Três commits locais, nenhum
empurrado. Nenhum assunto contém uma das catorze cadeias do G1 (conferido com as linhas entre `CADEIAS` da receita do
plano 1B contra cada assunto, antes de cada commit). Todos terminam com
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

| Commit | Item |
|---|---|
| `cd71f8e` | P51-27 — o que a metade de servidor lançou chega à linha de quem hospeda, escapado e no teto |
| `fbf2d0d` | P51-26 (metade do servidor) — o id do fio vai à linha da recusa sem quebrar nem inverter a linha; o catálogo diz `catalogo=true` |
| `b2dd6d1` | F1-portaria — a política de admissão fecha quando o banco falha |

Ordem seguida: P51-27, depois P51-26, depois F1 (independente).

## O que foi feito, por item

### 1. P51-27 (`mods/mod.rs`, `mods/despacho.rs`, `docs/como-se-faz-um-mod.md`)

- `Falha` deixa de ser `Copy` e passa a `Debug, Clone, PartialEq, Eq, thiserror::Error`.
  `Falha::Lancou { texto, onde }` e `Falha::NaoCarregou { texto, onde }`; as outras três variantes ficam como eram.
- `Display`: `mod threw: {texto:?}` e `mod threw while loading: {texto:?}`, seguidos de ` at {onde:?}` quando há
  onde (`onde_dito`). Texto e onde vão pelo `Debug`, porque **os dois** são do MOD: o texto é o que ele lançou, e o
  onde é a primeira linha da `stack`, que o MOD pode reescrever. Exemplos reais, medidos com o código deste lote:
  - `mod threw: "ReferenceError: console is not defined" at "<anonymous> (eval_script:1:29)"`
  - `mod threw: "TypeError: cannot read property 'x' of null" at "aoPedir (eval_script:1:42)"`
  - `mod threw while loading: "Error: topo" at "<eval> (eval_script:1:10)"`
  - `mod threw while loading: "SyntaxError: Unexpected identifier 'é'" at "eval_script:1:1"`
  - `mod threw: "só texto\u{202e}"` (um `throw` de string) e `mod threw: "[valor que não virou texto]"` (um `Symbol`)
- Leitura (`o_que_o_mod_lancou`, molde `Lancado::da_excecao` + `o_que_o_mod_lancou` do executor da janela, copiados):
  `ctx.catch()`, `name` por `Coerced<String>` com recuo `Error`, `message()`, `stack()` → primeira linha não vazia,
  aparada, sem o `at ` do começo; valor que não é `Error` sai como o JavaScript o diria; a exceção que a leitura deixar
  pendente é tirada. Erro do motor que não é exceção (`FromJs` etc.) sai com o texto do motor (`Lancado::do_motor`).
- `falha_da_volta(ctx, passos, teto, erro, como)`: **confere o teto antes de ler** (um getter do MOD rodando na
  leitura conta consultas e não pode transformar «lançou» em «passou do tempo»); quando passou, `ctx.catch()` e
  `PassouDoTempo`. A leitura roda sob o mesmo teto da chamada — nada zera `passos` entre a chamada e a leitura.
  Usada em `chamar`, `pedir`, `recolher_quintal` e `carregar`, sempre **dentro** do `contexto.with`.
- `carregar`: o corpo que monta o contexto (`arquivos`, `volume`, `mundo`, `eval`) foi para um método privado
  `montar(&self, ctx, id, pasta, fonte) -> rquickjs::Result<()>`, para a falha ser lida dentro do `with` sem um
  fecho chamado na hora (o `clippy::redundant_closure_call` recusaria). O corpo é o mesmo; os `pasta.clone()` viraram
  `pasta.to_path_buf()` porque `pasta` passou a ser `&Path`. `Runtime::new` e `Context::full` que falham viram
  `NaoCarregou` com o texto do motor.
- Corte (`TETO_DO_LANCADO_NO_REGISTRO = 512`): texto e onde **juntos** cabem em 512 pelo tamanho escapado
  (`tamanho_no_registro` = `escape_debug().count()`, com o apóstrofo valendo 1 — a regra de
  `ate_o_teto_do_registro` reescrita como `ate_o_teto`). O texto vem primeiro; um texto cortado termina em `…`
  (`MARCA_DO_CORTE`) e leva o onde embora (o pedaço que sobraria, `"<an…"`, não diz onde).
- Testes antigos: `assert_eq!(…, Err(Falha::Lancou))` (o de :846) passou a `assert!(matches!(…, Err(Falha::Lancou
  { .. })), "…")`, com mensagem; os `matches!` de :776, :1083 e :1127 ganharam `{ .. }`. `lib.rs:667` e
  `despacho.rs:155` compilam como estão (`%falha` por valor); `despacho.rs:429` (`matches!` em `&Falha`) também.
  `lib.rs` não foi tocado.
- Guia (`docs/como-se-faz-um-mod.md`, depois da frase do `mundo.registrar`): um parágrafo dizendo que a linha de quem
  hospeda traz o texto entre aspas e escapado, a primeira linha da pilha quando o lançado é `Error`, o exemplo do
  `ReferenceError`, `mod threw while loading: …` para o topo e o erro de sintaxe, e o corte em 512 contados escapados.
  Cada frase foi conferida contra a saída real acima.

### 2. P51-26, metade do servidor (`mods/pedidos.rs`)

- Em `executar`: id vazio → `warn!(catalogo = true, %error, "MOD request refused")`, sem `mod_id=`; senão
  `warn!(mod_id = %id_seguro(id), %error, …)`. Como o conserto está em `executar`, cobre o `ModRequest` e o
  `PedidoDeImagem::Ler` do volume.
- `id_seguro`: `take(TETO_DO_ID_NO_REGISTRO = 128)` e filtra `quebra_ou_inverte_a_linha` (controle, U+2028/U+2029,
  `Bidi_Control`), cópia da regra de `id_no_registro` do app, sem depender dele. Sem aspas: `mod_id=autor/nome`
  continua a grafia que o guia procura.
- Testes: um ajudante `o_registro_da_recusa(id)` captura o registro (o servidor é montado **antes** de o registro
  ouvir, senão as 13 linhas de migração entram na conta); o teste do grep passou a usá-lo, com as asserções
  intactas.
- A resposta ao cliente (`bridge-refused`) e o fio não mudaram. A metade do app (`mod_request`) é do lote C.

### 3. F1-portaria (`admissao.rs`)

- `use rusqlite::{params, Connection, OptionalExtension}`.
- `Politica::carregar`: senha por `.optional()?`, contagem de convites por `?`.
- `conferir_convite`: `.optional()?`.
- Os três chamadores ficam como estão (conferidos): `session.rs:507-510` → `CredentialRejected`;
  `seeled` `politica_aberta` (`main.rs:123-128`) → `unwrap_or(false)`, sem o aviso de porta aberta;
  `apps/seele-app/src/main.rs:7899` → `FalhaNaPortaria::BancoNaoRespondeu` (frase «O SERVIDOR DESTA MÁQUINA NÃO
  RESPONDEU»).

## RED / GREEN

Todos os testes novos foram escritos antes do conserto e vistos vermelhos com a frase deles. Comandos:
`perl -e 'alarm 600; exec @ARGV' cargo test -p seele-server --lib mods::` (e `mods::pedidos`, e `admissao`).

**P51-27 — RED no código de `ec5134d`** (7 vermelhos, 85 verdes):
- `despacho::um_mod_que_lanca_nao_leva_o_vizinho_junto` — «a falha do MOD que lançou não traz o que ele lançou, e a
  linha que o desliga no seele.log de quem hospeda diz só que ele lançou: mod threw»
- `um_aopedir_que_chama_console_diz_reference_error` — «… quem hospeda lê só que o MOD lançou: mod threw»
- `um_throw_no_topo_diz_o_que_foi_lancado_ao_carregar` — «…: mod source did not compile»
- `o_texto_lancado_com_quebra_de_linha_fica_numa_linha_so` — «a falha não traz o texto que o MOD lançou: mod threw»
- `o_texto_lancado_cabe_no_teto_depois_de_escapado` — «a falha não traz o texto que o MOD lançou, escapado: mod threw»
- `um_aopedir_que_nao_termina_passa_do_tempo_e_nao_lanca` — `left: Err(Lancou)`, `right: Err(PassouDoTempo)`
- `um_getter_que_nao_termina_nao_segura_a_leitura_do_que_foi_lancado` — «…: mod threw»

Dois testes entraram depois, com o conserto já escrito, e foram provados vermelhos por reversão (D e B2 abaixo):
`um_topo_que_nao_termina_passa_do_tempo_ao_carregar` (no código antigo seria `NaoCarregou`) e
`o_onde_que_o_mod_escreve_tambem_fica_numa_linha_so`. GREEN: `mods::` 94 passaram, 0 falharam.

**P51-26 — RED no código de `cd71f8e`** (2 vermelhos):
- `a_recusa_do_catalogo_diz_catalogo_e_nao_um_mod_id_em_branco` — linha `MOD request refused mod_id= error=cannot read`
- `um_id_com_quebra_de_linha_nao_forja_uma_linha_no_registro` — `left: 2, right: 1`, registro
  `" WARN seele_server::mods::pedidos: MOD request refused mod_id=a/b\nWARN seele_server: forjada error=cannot read\n"`
- o teste do grep continuou verde.

`o_id_da_recusa_perde_o_que_inverte_a_linha_e_cabe_no_teto` (unidade de `id_seguro`) entrou depois do conserto e foi
provado por reversão (G3, G4). GREEN: `mods::pedidos` 5 passaram.

**F1 — RED no código de `fbf2d0d`** (2 vermelhos):
- `uma_falha_ao_ler_a_politica_fecha_a_porta_em_vez_de_abrir` — «o banco não respondeu ao ler a senha, e a política
  saiu como lida — aberta, se não houver convite: Ok(Politica { senha_hash: None, aceita_convites: false })»
- `uma_falha_ao_conferir_o_convite_e_erro_e_nao_segredo_invalido` — «…: Ok(Err(SegredoInvalido))»

GREEN: `admissao` 20 passaram. Depois, `cargo test -p seele-server --no-fail-fast` inteiro: lib 545, seeled 3, os sete
binários de `tests/` e os doctests verdes.

## Reversão de cada guarda

Cada uma: cópia do arquivo verde no scratchpad, a reversão aplicada por substituição exata (uma ocorrência), os
testes do módulo rodados, o arquivo restaurado da cópia e conferido com `cmp` («RESTAURADO-IGUAL» nas 18).

| # | Reversão | Vermelho (frase) |
|---|---|---|
| A | `falha_da_volta` deixa de ler (texto vazio) — o equivalente central de `map_err(\|_\| Falha::Lancou)` | 7: despacho, console, topo, quebra, comprido, getter, onde — todos com `mod threw: ""` |
| A2 | só o `f.call` de `pedir` volta a `map_err(\|_\| Falha::Lancou { vazio })` | console («…: mod threw: ""») e `aoPedir` eterno (`left: Err(Lancou { texto: "", onde: None })`) |
| B | `{texto:?}` → `{texto}` no `Display` de `Lancou` | quebra: «o texto que o MOD lançou quebra a linha do seele.log…: "mod threw: Error: a\nWARN seele_server: forjada at …"»; e comprido |
| B2 | `{onde:?}` → `{onde}` em `onde_dito` | onde: «o onde que o MOD escreveu quebra a linha…: "… at a\rWARN seele_server: forjada\u{2028}outra"» |
| C | `cortado_no_teto` nunca corta | comprido: «a falha passou do teto da linha: 20500 caracteres, para um teto de 512 mais a moldura de 33» |
| C2 | o tamanho conta caracteres, e não o escape (`tamanho_no_registro` → 1) | comprido: «… 5061 caracteres …» |
| D | `falha_da_volta` sem a conferência do teto | `aoPedir` eterno, topo eterno (`Err(NaoCarregou { texto: "InternalError: interrupted", … })`) e o antigo `o_teto_de_tempo_e_aplicado` |
| E | lê a exceção **antes** de conferir o teto | getter: «um getter que não termina tirou da falha o texto que o MOD lançou: mod went past its step ceiling» |
| F | o onde fica mesmo com o texto cortado | comprido: «… leva junto o pedaço do onde que sobrou…: …\u{100000}…" at "<an…"» |
| G1 | os dois ramos voltam a `mod_id = %id` | catálogo (`mod_id= error=cannot read`) e quebra (`left: 2`); grep verde |
| G2 | só o ramo do id volta a `%id` | quebra (`left: 2, right: 1`) |
| G3 | `quebra_ou_inverte_a_linha` sem a lista de bidi | unidade: `left: "a/\u{202e}bc\u{2066}de"`, `right: "a/bcde"` |
| G4 | `id_seguro` sem o `take(128)` | unidade: `left: 300, right: 128` |
| G5 | sem o ramo do catálogo | catálogo: `… mod_id= error=cannot read` |
| H1 | a senha volta a `.ok()` | caso (1): «… ao ler a senha …: Ok(Politica { senha_hash: None, aceita_convites: false })» |
| H2 | a contagem volta a `.unwrap_or(0)` | caso (2): «… ao contar os convites …: Ok(Politica { senha_hash: None, aceita_convites: false })» |
| H3 | `conferir_convite` volta a `.ok()` | «… vai ao log como segredo inválido: Ok(Err(SegredoInvalido))» |

Sobre o «voltar a `.ok()` ou a `.unwrap_or(0)` deixa os dois vermelhos» da lista: cada reversão deixa vermelho **o
caso dela** (H1 o (1), H2 o (2)); o teste para na primeira asserção, por isso cada uma foi feita sozinha.

## Arquivos mudados

- `crates/seele-server/src/mods/mod.rs`
- `crates/seele-server/src/mods/despacho.rs` (só o teste)
- `crates/seele-server/src/mods/pedidos.rs`
- `crates/seele-server/src/admissao.rs`
- `docs/como-se-faz-um-mod.md`

Não tocados: `lib.rs` (o fim do `Copy` não pediu), `apps/seele-app/src/main.rs`, `api/`, `crates/seele-proto/`,
pendências e índice. `cargo xtask check-api` verde; o diff em `api/` e `seele-proto/` é vazio.

## Verificação

- `cargo fmt --all -- --check` limpo antes de cada commit.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` limpo antes de cada commit (só o
  `ld: duplicate -rpath`, ambiental).
- `cargo test -p seele-server --no-fail-fast` verde no fim. A bateria inteira (workspace, conformidade, check-runtime,
  Playwright) não foi rodada aqui: é o passo próprio do fluxo do lote. Nenhum teste de conformidade foi escrito.

## O que não pôde ser compilado ou rodado aqui

Nada: nenhum código com `cfg(windows)` ou `cfg(target_os = "linux")` foi escrito ou tocado.

## Autorrevisão

- O texto do MOD só entra no seele.log pelo `Display` de `Falha`, que escreve texto e onde pelo `Debug` e os corta
  juntos em 512 escapados; as três portas (`despacho.rs:155` e `:437-442`, `lib.rs:667`, `pedidos.rs` `%error`) são
  `%` desse `Display`. O P51-27 não reabre o que o P51-26 fecha: B, B2, C e C2 provam.
- Nada no fio: `pedidos::executar` responde `bridge-refused` como antes, e `Falha` não atravessa.
- O `falhou` de `chamar`/`pedir` é um fecho só de referências (Copy), passado a cada `map_err`; o teto é o mesmo da
  chamada, sem `store(0)` entre ela e a leitura.
- `unwrap`/`expect` só em teste; nada de índice ou fatia em `src/` nas linhas novas (`texto.get(..onde)`).

## Preocupações e desvios (para o controlador decidir)

1. **Além da lista, de propósito**: `pedir` e o topo de `carregar` passam a devolver `PassouDoTempo` quando o teto os
   cortou. Antes diziam `Lancou` / `NaoCarregou` sem texto; com o texto deste lote diriam
   `mod threw: "InternalError: interrupted"`, que manda procurar um `throw` que não existe — o mesmo motivo do
   executor da janela. A lista só pedia o `ctx.catch()` em `PassouDoTempo`; o efeito é um lugar a mais que diz o
   teto. Testes: `um_aopedir_que_nao_termina_passa_do_tempo_e_nao_lanca`, `um_topo_que_nao_termina_passa_do_tempo_ao_carregar`.
2. **Além da lista**: o onde também vai por `Debug` e tem teste (B2); o corte põe `…` e tira o onde quando o texto
   encheu a linha (F); `conferir_convite` ganhou teste próprio (H3); `id_seguro` ganhou teste de unidade (G3, G4).
3. **Palavra solta**: `NaoCarregou` de um `Runtime::new`/`Context::full` que falha (alocação) diz «mod threw while
   loading: "<erro do motor>"», e um erro de conversão do motor (um `aoPedir` que falta ou devolve número, um valor
   não-texto em `dados`) diz «mod threw: "Error converting from js 'int' into type 'string'"». O texto diz o que
   houve, mas «threw» é largo. Antes era «mod threw» sem texto nenhum.
4. **Nível do log da portaria**: o erro de banco ao conferir um convite chega ao log como
   `session closed … error=handshake refused: could not evaluate admission: <erro>` em **INFO** (`lib.rs:809`), e não
   em ERROR. A lista mandou deixar os chamadores como estão; o texto passou a ser o erro do banco em vez de
   `SegredoInvalido`, mas o nível continua o de toda recusa de handshake.
5. **Doc tocada fora do item**: o `# Errors` de `chamar` dizia que um MOD não carregado também é `Lancou`, o que não é
   verdade desde o `NaoCarregadoAqui`; passou a nomear `NaoCarregadoAqui`.
6. **Prova que falta**: «sem zerar o teto entre a chamada e a leitura» não tem teste afiado. O getter que não
   termina prova que a leitura é limitada e que a classificação vem antes (E); um `store(0)` antes da leitura só
   dobraria o orçamento, e a diferença de tempo (dezenas de ms) não daria um teste estável.
7. **Pendência 51 desatualizada**: `docs/pendencias.md:7835-7857` (P51-27) ainda descreve `mod threw` e o grep do
   `#[error("mod threw")]`. A lista proíbe editar pendências aqui; é do lote Z. O P51-26 só fecha depois do lote C.
8. O diff de `mod.rs` é grande (855 linhas na estatística) porque o corpo de `carregar` foi para `montar` com um nível
   a menos de recuo; com `git diff -w`, as linhas do corpo não mudam além dos quatro `to_path_buf()`.

## Rodada 1

Base `b2dd6d1`. HEAD depois da rodada: `34235fe`. Três commits locais, nenhum empurrado; cada assunto conferido
contra as catorze cadeias do G1 (as linhas entre `CADEIAS` do plano 1B) antes do commit — nenhum as contém. Todos
terminam com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

| Commit | Achado |
|---|---|
| `380a745` | B-1 (Important) e B-2 (Minor) — guarda do escape de `NaoCarregou` e moldura exata no teste do teto |
| `deed8a0` | B-4 (Minor) — id feito só do que o filtro tira diz `mod_id_ilegivel=true` |
| `34235fe` | B-3 (Minor) — `mod failed` quando nada foi lançado, e a doc de `NaoCarregou` sem a promessa do `SyntaxError` |

B-5 não muda código (ver abaixo).

### B-1 — o escape de `NaoCarregou` ganha guarda (`mods/mod.rs`, testes)

- Teste novo `o_texto_lancado_no_topo_com_quebra_de_linha_fica_numa_linha_so`: `carregar("seele/topo-quebra",
  "throw new Error('a\\nWARN seele_server: forjada');", …)`, exige `Falha::NaoCarregou { .. }` (para o teste não
  deixar de guardar o que guarda se a classificação mudar), `contains("forjada")` e nenhum `\n` nem `\r`.
- Verde no código do HEAD. **Reversão**: `{texto:?}` → `{texto}` **só** no `#[error]` de `NaoCarregou`. Vermelho só
  o teste novo (97 verdes, 1 vermelho em `mods::`): «o texto que o MOD lançou no topo quebra a linha do seele.log de
  quem hospeda, e a segunda linha sai com a cara do produto: "mod threw while loading: Error: a\nWARN seele_server:
  forjada at \"<eval> (eval_script:1:10)\""». Restaurado da cópia, `cmp` igual.

### B-2 — a moldura do teste do teto é a do caso

- `o_texto_lancado_cabe_no_teto_depois_de_escapado`: `moldura = "mod threw: ".chars().count() + 2` (13), com o
  comentário dizendo por que o onde não entra (o texto cortado o leva, e a última asserção confere). Antes: 33.
- **Prova de que aperta**: a reversão «corte com 10 de folga» (`cortado_no_teto(texto, TETO_DO_LANCADO_NO_REGISTRO +
  10)` em `Lancado::no_teto`):
  - com a moldura nova: vermelho — «a falha passou do teto da linha: 531 caracteres, para um teto de 512 mais a
    moldura de 13»;
  - com o teste do HEAD (`b2dd6d1`, moldura 33): **verde** — o que o achado dizia.
  Restaurado, `cmp` igual. A reversão C antiga (nunca corta) continua vermelha, e com folga maior.
- Uma nota: com este texto (`\u{100000}`, dez escapados cada), o corte anda de dez em dez; um erro de um a mais no
  teto (a marca não contada) cai no mesmo 508 e não se vê por este caso. O que este teste prova é que o corte não
  passa de 512 + 13; o de um a mais seria outro teste, com caracteres de um escapado só. Não acrescentei.

### B-4 — o id ilegível (`mods/pedidos.rs`)

- `executar` calcula `id_seguro(id)` uma vez; id vazio → `catalogo=true` (como antes); id não vazio cujo
  `id_seguro` sai vazio **ou só espaço** (`trim().is_empty()`) → `warn!(mod_id_ilegivel = true, %error, …)`; senão
  `mod_id = %legivel`. `mod_id_ilegivel=` não casa com `grep "mod_id="` (o `_` vem antes do `=`), e a grafia
  `mod_id=autor/nome` do guia não muda.
- O espaço entrou além do achado: um id `"\u{202E} "` sairia `mod_id=  error=…`, a mesma forma em branco.
- **RED** no código de `380a745`, teste `um_id_so_do_que_o_filtro_tira_diz_que_nao_se_lia_e_nao_um_mod_id_em_branco`
  com o id `"\u{202E}\n \u{2028}"`: «… não diz que o id não se lia, ou escreve um `mod_id=` em branco: WARN
  seele_server::mods::pedidos: MOD request refused mod_id= error=cannot read» (a primeira versão do teste, sem o
  espaço; com o espaço, `mod_id=  error=…`). Exige também uma linha só.
- **Reversões**: sem o ramo (`else if false`) → vermelho, `mod_id=  error=cannot read`; `trim().is_empty()` →
  `is_empty()` → vermelho, a mesma linha. Os outros cinco de `mods::pedidos` verdes nas duas. Restaurado, `cmp`
  igual.

### B-3 — palavras que diziam mais que o código (`mods/mod.rs`, `docs/como-se-faz-um-mod.md`)

Feito por inteiro, não só a doc: o achado deixava a forma ao controlador (prefixo ou variante), e o despacho pediu
os Minor baratos e certos. Escolhi a variante, porque um `Falha::Lancou` que não lançou seria a mesma palavra a mais,
agora no nome do tipo; e conferi antes que ela não muda o que o produto faz.

- **Medido antes** (sonda temporária, apagada; `cmp` igual depois):
  - `pedir` sem `aoPedir`: `mod threw: "Error converting from js 'undefined' into type 'function'"`;
  - `aoPedir = () => 1`: `… 'int' into type 'string'`; `dados.a = 1` (em `pedir` e em `chamar`): o mesmo;
    `dados.a = {}`: `… 'object' into type 'string'`;
  - `Anfitriao::com_tetos(m, 100)` com m de 1 B a 64 KiB: `carregar` dá `mod threw while loading: "Allocation
    failed while creating object"`; com 128 KiB carrega;
  - o glutão (`um_mod_que_aloca_sem_parar…`) dá `Lancou { "InternalError: out of memory", … }` — exceção, e por isso
    continua `Lancou` com a mudança (o teste dele aceita `Lancou | PassouDoTempo`, e segue verde);
  - topo: erro de sintaxe → onde `"eval_script:1:1"`; `throw` → `"<eval> (eval_script:1:10)"`;
    `JSON.parse('{')` → `"SyntaxError: Expected property name or '}' in JSON at position 1 (line 1 column 2)" at
    "<input>:1:1"`.
- `Falha` ganha `FalhouSemLancar { texto }` (`mod failed: {texto:?}`) e `NaoCarregouSemLancar { texto }`
  (`mod failed while loading: {texto:?}`). `Lancado` carrega `do_motor: bool` (verdadeiro só em `Lancado::do_motor`);
  `lancou`/`nao_carregou` viraram `na_chamada`/`ao_carregar`, que escolhem a variante pelo `do_motor`. O texto do
  motor passa pelo mesmo `no_teto` e vai pelo `Debug`, como antes.
- **O que não muda**: o caminho de eventos (`despacho.rs:429`) só poupa `NaoCarregadoAqui`, e por isso desliga
  quem `FalhouSemLancar` como desligava quem «lançou»; `lib.rs:667`, `despacho.rs:155` e o `%error` de `pedidos.rs`
  escrevem o `Display` como antes; `bridge-refused` ao cliente; `Falha` não atravessa o fio. `api/` e `seele-proto`
  sem diff, `cargo xtask check-api` verde. Nenhum `match` exaustivo em `Falha` fora dos testes.
- Doc de `NaoCarregou`: o texto do QuickJS e o onde **costumam** separar os dois casos (com os dois ondes medidos),
  e não é regra: um `JSON.parse('{')` no topo também lança `SyntaxError`, com o onde em `<input>:1:1`. Os docs de
  texto de `Lancou`/`NaoCarregou` perderam «o erro do motor quando nada foi lançado»; «It threw.» virou «Lançou, numa
  chamada.»; `# Errors` de `carregar` e `chamar` e a doc do teto nomeiam as variantes novas.
- Guia: uma frase depois da do `mod threw while loading`: quando nada foi lançado (um `aoPedir` que falta, um que
  devolve outra coisa que texto, um valor que não é texto em `dados`), a linha diz `mod failed: …`, com o exemplo
  `mod failed: "Error converting from js 'int' into type 'string'"` — medido.
- **RED** no código de `deed8a0` (2 vermelhos):
  - `um_aopedir_que_falta_ou_devolve_numero_diz_que_falhou_e_nao_que_lancou` — «sem aoPedir: a falha de um aoPedir
    que o motor não converteu diz que o MOD lançou, e manda procurar um throw que não existe: mod threw: "Error
    converting from js 'undefined' into type 'function'"»;
  - `um_contexto_que_nao_coube_na_memoria_diz_que_falhou_ao_carregar_e_nao_que_lancou` (teto de 16 KiB) — «a falha
    do motor ao montar o contexto de um MOD diz que o MOD lançou ao carregar, antes de uma linha dele rodar: mod
    threw while loading: "Allocation failed while creating object"».
- **Reversões** (cada uma restaurada, `cmp` igual):
  - central, `Lancado::do_motor` com `do_motor: false`: os dois vermelhos (99 verdes);
  - só `na_chamada` ignorando o `do_motor`: só o de `pedir` vermelho;
  - só `ao_carregar` ignorando o `do_motor`: só o de `carregar` vermelho.

### B-5 — a frase do commit `cd71f8e`

Nada no código. **Para o lote Z, ao registrar o P51-27**: antes deste lote, um topo cortado pelo teto virava
`Falha::NaoCarregou` sem texto, cujo `Display` era «mod source did not compile» — e não «lançou». A frase do commit
`cd71f8e` («agora também em `pedir` e no topo de `carregar`, que antes diziam «lançou»») vale para `pedir` e não
para `carregar`. A escrever: o topo cortado pelo teto dizia «mod source did not compile» e passa a dizer `mod went
past its step ceiling`. E, desta rodada: o que falhou sem lançar diz `mod failed: …` / `mod failed while loading:
…`, e não `mod threw`.

### Arquivos mudados na rodada

- `crates/seele-server/src/mods/mod.rs`
- `crates/seele-server/src/mods/pedidos.rs`
- `docs/como-se-faz-um-mod.md`

### Verificação

- `cargo fmt --all -- --check` limpo antes de cada um dos três commits.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` limpo no estado de cada commit (só o
  `ld: duplicate -rpath`, ambiental).
- `cargo test -p seele-server --no-fail-fast` no estado final: lib 549, seeled 3, os sete binários de `tests/` e os
  doctests verdes. `cargo xtask check-api` verde.
- Os commits 1 e 2 foram montados a partir do estado final (o `mod.rs` do commit 1 é o do HEAD anterior com só os
  dois trechos de teste; testado nesse estado: `mods::tests::` 44 verdes), e o estado final foi restaurado e
  conferido com `cmp` antes do commit 3.
- A bateria inteira (workspace, conformidade, check-runtime, Playwright) não rodou aqui: é o passo próprio do fluxo.

### O que não pôde ser compilado ou rodado aqui

Nada: nenhum código com `cfg(windows)` ou `cfg(target_os = "linux")` foi escrito ou tocado.

### Preocupações e o que ficou

1. **B-3 foi além do mínimo**, de propósito: duas variantes novas em `Falha` (enum público do seele-server). Se o
   controlador preferir só o prefixo, a troca é local a `na_chamada`/`ao_carregar` e aos dois `#[error]`. O teste de
   `carregar` depende de 16 KiB não caberem num contexto do QuickJS; medido que até 64 KiB não cabe e 128 KiB cabe,
   e a mensagem do `expect_err` diz o que houve se um dia couber.
2. **B-2, o corte de um a mais** não tem teste afiado (ver a nota em B-2). Deixado: o achado pedia a moldura exata,
   e ela está feita; um teste de caracteres de um escapado é barato, mas não foi pedido.
3. **B-4, o espaço**: `trim()` também trata como ilegível um id só de espaço comum (que o fio aceita). Um id assim
   não é de MOD nenhum que o guia mande procurar, e a linha antes saía em branco; mas é uma decisão além do achado.
4. **Para o lote Z**: além do B-5, `docs/pendencias.md` (P51-27) precisa das frases novas `mod failed: …` e
   `mod failed while loading: …`, e o P51-26 ganha a linha `mod_id_ilegivel=true`.
