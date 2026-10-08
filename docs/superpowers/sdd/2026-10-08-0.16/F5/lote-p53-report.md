# Relatório do lote P53 (F5, 0.16)

Instalar um MOD de uma pasta com atalho: a tela diz qual é o atalho, e o `seele.log` ganha a linha. Pendência 53.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a8255b8e14cec9b31`
- Branch: `0.16/lotes-p53`, aberto em `0f94be6`.
- Status: DONE_WITH_CONCERNS. As preocupações estão no fim, e nenhuma bloqueia.

## Os itens e os commits que os fecham

| Item do brief | Commit |
|---|---|
| Variante própria para o atalho: `FalhaAoInstalarMod::TemAtalho(String)`, com o caminho. `copiar_arvore` devolve `FalhaNaCopia { Atalho(PathBuf), Disco(io::Error) }`, e `NaoCopiei` fica para as outras falhas. | `c6ca70e4` |
| A linha no registro: `instalar_de` escreve `tracing::warn!` em toda recusa, com a pasta e a recusa inteira (o caminho do atalho junto). | `c6ca70e4` |
| As frases de `TemAtalho`, `NaoCopiei`, `ManifestoRecusado` e `SemManifesto`. | `6038abbe` |
| O guarda: `variants_of` aceita `Nome(…),` e `Nome { … },`, e o guarda das recusas lê também `src/mods.rs` e inclui `FalhaAoInstalarMod`. Um teste novo prende o leitor. | `6038abbe` |

A pendência 53 fecha com os dois: `c6ca70e4` e `6038abbe`. A rodada 1 de conserto, pedida pela revisão, acrescentou `1a7011e7`, `8b6ea89a` e `14253634`. Ela está na seção «Rodada 1 de conserto», no fim. As seções de antes descrevem o que já está no branch, com as mudanças da rodada indicadas onde elas valem.

## O que foi feito

### `apps/seele-app/src/mods.rs` (`c6ca70e4`)

- `FalhaNaCopia`, privado ao módulo, com `From<std::io::Error>`, para os `?` de `copiar_arvore` continuarem como estavam. O atalho deixa de ser um `io::Error` inventado com o caminho dentro do texto.
- `instalar_de` passou a ser uma casca de `publicar_de`, que é o corpo de antes. A casca escreve a linha em toda saída `Err`, num lugar só, para nenhuma recusa nova escapar do registro. A linha medida no teste:

  ```
  WARN seele_app::mods: não instalei o MOD desta pasta origem=/var/folders/…/origem recusa=TemAtalho("/var/folders/…/origem/node_modules/.bin/playwright-core")
  ```

  Desde a rodada 1, a pasta vai por `caminho_no_registro`, entre aspas e com escape: `origem="/var/folders/…/origem"`.

  O `?recusa` é o `Debug` da variante, então o nome no registro é o mesmo que o serde manda à janela, e não há uma segunda lista de nomes para divergir.
- A recusa continua: nada passou a aceitar atalho.

### `apps/seele-app/ui/frases.js` (`6038abbe`)

O que a tela mostra agora, medido rodando o `fraseDeErro` do arquivo no node:

| Erro | Antes | Agora |
|---|---|---|
| `{"TemAtalho": "…/node_modules/.bin/playwright-core"}` | «ALGO FALHOU E ESTE APP NÃO SABE EXPLICAR O QUÊ», com o JSON cru | `ESTA PASTA TEM UM ATALHO, E O SEELE NÃO INSTALA MOD COM ATALHO: …/node_modules/.bin/playwright-core.`<br>`Tire o atalho e instale de novo. Num MOD em desenvolvimento, os atalhos costumam estar em «node_modules».` (segunda linha das rodadas 1 e 2; antes era `Instale a pasta do pacote, sem «node_modules».`) |
| `{"ManifestoRecusado": "api-too-old"}` | «ALGO FALHOU…», com o JSON cru | `ESTE SEELE RECUSOU O «mod.json» DESTA PASTA: api-too-old.`<br>`Nada foi instalado.` |
| `{"NaoCopiei": "Permission denied (os error 13)"}` | «ALGO FALHOU…», com o JSON cru | `NÃO CONSEGUI COPIAR ESTE MOD PARA O SEELE: Permission denied (os error 13).`<br>`Nada foi instalado. Confira o espaço e as permissões do disco, e tente de novo.` |
| `"SemManifesto"` | `SemManifesto`, o nome cru da variante | `ESTA PASTA NÃO TEM «mod.json».`<br>`Escolha a pasta do MOD, e não a pasta que a contém.` |

- As três com campo são braços de `fraseDeErro`, como o `erro.NomeRecusado`, logo antes dele. `SemManifesto` não tem campo: o serde a manda como texto, e o texto vai direto a `FRASES`. Por isso ela está em `FRASES`, depois do bloco do catálogo de MODs.
- Todas têm duas linhas. Sem o detalhe interpolado, têm 171 (eram 112 antes da rodada 1, e 174 na rodada 1), 66, 124 e 82 caracteres. O `LIMITE_DE_FRASE` é 180. A de `SemManifesto`, por estar em dicionário, é medida pelos guardas de tamanho e de terceira linha, e passou.

### `apps/seele-app/tests/frontend.rs` (`6038abbe`)

- `variants_of` lê linha a linha e conta a profundidade de `(`, `{` e `[`. Na profundidade zero, uma linha que começa com um nome seguido de `,`, `(` ou `{` declara uma variante. Os campos de uma variante `struct` ficam na profundidade um, e não contam.
- `every_refusal_the_bridge_writes_itself_has_a_sentence_in_the_page` confere pares (arquivo, enum): os três de `src/main.rs` de antes, e `("src/mods.rs", "FalhaAoInstalarMod")`. Ele junta todas as que faltam e reprova uma vez, com a lista inteira. A mensagem é em português.
- Teste novo, `variants_of_le_toda_forma_de_variante_e_nao_cala_a_que_tem_campo`:
  - uma fonte sintética com unitária, tupla de um e de dois campos, `struct` de várias linhas com atributo e comentário, `struct` de uma linha, e um comentário com `Falsa,` dentro. Exige a lista exata;
  - nas fontes de verdade: `FalhaAoHospedar` traz `NomeRecusado` e `EstaVersaoRevogada`, e `FalhaAoInstalarMod` é exatamente `[SemManifesto, ManifestoRecusado, TemAtalho, NaoCopiei]`.

### O teste da A10 mudou de injeção

O brief pede que `uma_copia_que_falha_no_meio_nao_deixa_pacote_pela_metade` continue `NaoCopiei`. Mas ele injetava a falha **com um atalho** (`extra/atalho → /etc/hosts`), e o atalho agora é `TemAtalho`. Para o teste continuar `NaoCopiei`, mudei a injeção: um arquivo `extra/trancado.txt` com modo `0o000`.

- A leitura do manifesto não tropeça nele, porque `seele_core::mods::collect` pula o que não abre.
- `copiar_arvore` tropeça no `std::fs::copy`, depois de já ter criado a estufa.
- Uma pré-condição reprova com frase se o arquivo ainda se lê, que é o que acontece rodando como root: «o arquivo trancado ainda se lê (este teste roda como root?)…». Sem essa pré-condição, o teste reprovaria do mesmo jeito, só com uma mensagem pior.

`uma_copia_que_falha_nao_toca_no_que_ja_estava_publicado` continua com o atalho, porque só exige `is_err()`. `um_atalho_dentro_do_pacote_nao_e_instalado` passou também a conferir que nada foi publicado.

## TDD

### RED, `mods.rs` (só a variante, sem ninguém que a produza)

```
thread 'mods::instalar::um_atalho_dentro_do_pacote_nao_e_instalado' panicked at apps/seele-app/src/mods.rs:642:22:
um atalho dentro do pacote não foi recusado como atalho, e a tela não tem como dizer qual é ele: Err(NaoCopiei("atalho dentro do pacote: /var/folders/…/origem/atalho.txt"))
thread 'mods::instalar::a_recusa_do_atalho_fica_no_registro_com_o_caminho' panicked at …:
o atalho não foi recusado como atalho: Err(NaoCopiei("atalho dentro do pacote: /var/folders/…/node_modules/.bin/playwright-core"))
thread 'mods::instalar::toda_recusa_de_instalar_fica_no_registro_com_a_pasta' panicked at …:
uma recusa de instalar não deixou linha WARN com a pasta no `seele.log`: Err(SemManifesto)
test result: FAILED. 12 passed; 3 failed
```

Com o erro tipado e ainda sem o `warn!`, o teste do atalho passou, e os dois de rastro reprovaram pela linha que faltava (o rastro vazio):

```
a recusa do atalho não deixou linha WARN com o caminho dele no `seele.log`, e quem relata «não instala» chega sem dado nenhum:
uma recusa de instalar não deixou linha WARN com a pasta no `seele.log`: Err(SemManifesto)
test result: FAILED. 13 passed; 2 failed
```

### RED, `frontend.rs`

Com o guarda reescrito e o leitor antigo:

```
variants_of_le_toda_forma_de_variante_e_nao_cala_a_que_tem_campo ... FAILED
  left: ["Unitaria", "Ultima"]
 right: ["Unitaria", "ComCampo", "ComDois", "Struct", "NaMesmaLinha", "Ultima"]
every_refusal_the_bridge_writes_itself_has_a_sentence_in_the_page ... FAILED
estas recusas chegam à página sem frase escrita para elas, …: ["FalhaAoInstalarMod::SemManifesto"]
```

O leitor antigo enxergava só a unitária, `SemManifesto`. Com o leitor novo e ainda sem as frases:

```
estas recusas chegam à página sem frase escrita para elas, …: ["FalhaAoInstalarMod::SemManifesto", "FalhaAoInstalarMod::ManifestoRecusado", "FalhaAoInstalarMod::TemAtalho", "FalhaAoInstalarMod::NaoCopiei"]
```

### GREEN

- `cargo test -p seele-app --bin seele-app mods`: 40 passaram (eram 38; entraram os dois testes de rastro).
- `cargo test -p seele-app --test frontend`: 263 passaram, entre eles `every_way_the_update_can_fail_asks_something_different_of_the_reader` (`FalhaAoAtualizar` segue com 8), `every_reason_a_session_can_end_with_has_a_sentence_in_the_page`, `quem_nao_prova_a_chave_tem_frase_propria_e_nao_a_de_versao_incompativel` e `a_volta_recusada_so_diz_o_que_vale_para_toda_origem_e_todo_motivo`.
- `cargo xtask check-runtime`: saiu 0, com as 7 bancadas dele.
- `cargo fmt --all -- --check` e `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`: limpos antes de cada commit. O fmt reflowou uma lista em `frontend.rs` antes do segundo commit, e o `frontend` rodou de novo depois disso: 263 verdes.

## As reversões

Cada uma desfez o conserto num arquivo guardado, rodou, e voltou da cópia guardada, conferida com `cmp`.

| Reversão | O que reprovou | Restaurado |
|---|---|---|
| 1. Tirar o `warn!` de `instalar_de` | `a_recusa_do_atalho_fica_no_registro_com_o_caminho` e `toda_recusa_de_instalar_fica_no_registro_com_a_pasta`, com as frases do RED acima | `mods.rs`, `cmp` igual, sha1 `33c43afd…` |
| 2. Mapear `FalhaNaCopia::Atalho` de volta para `NaoCopiei("atalho dentro do pacote: …")` | `um_atalho_dentro_do_pacote_nao_e_instalado` («…não foi recusado como atalho…: Err(NaoCopiei(…))») e o teste de rastro do atalho | `mods.rs`, `cmp` igual, sha1 `33c43afd…` |
| 3. Tirar o braço `erro.TemAtalho` de `frases.js` | `every_refusal_…`: `["FalhaAoInstalarMod::TemAtalho"]` | `frases.js`, `cmp` igual, sha1 `27543e0f…` |
| 4. Devolver a `variants_of` o corpo antigo, que só lia `Nome,`, **e** tirar o braço `erro.TemAtalho` | `every_refusal_…` **passou calado**, e `variants_of_le_toda_forma_…` reprovou (`left: ["Unitaria", "Ultima"]`) | `frontend.rs` e `frases.js`, `cmp` iguais |

A 4 é a prova pedida pelo brief: sem a leitura de variante com campo, o guarda das recusas fica verde mesmo sem a frase do atalho, e só o teste do leitor reprova. As reversões de `frontend.rs` foram feitas antes do `cargo fmt`. O arquivo commitado difere da cópia guardada só pelo reflow daquela lista.

## As variantes que o guarda passou a enxergar

`variants_of` serve a quatro guardas. O que cada um passou a ver:

| Enum (arquivo) | Antes | Agora | Têm nome em `ui/*.js`? |
|---|---|---|---|
| `FalhaAoHospedar` (`main.rs`) | `JaHospedando`, `PortaOcupada`, `NaoSubiu`, `ListaDeRevogacaoRecusada` | mais `NomeRecusado(…)` e `EstaVersaoRevogada { defeito, corrigida_em }` (a variante `struct` da F6, em várias linhas) | sim, as duas: `erro.NomeRecusado` e `erro.EstaVersaoRevogada`, em `fraseDeErro` |
| `FalhaAoInstalarMod` (`mods.rs`) | fora do guarda | `SemManifesto`, `ManifestoRecusado`, `TemAtalho`, `NaoCopiei` | sim, com as frases deste lote |
| `FalhaAoEscolher`, `FalhaNaPortaria` (`main.rs`) | iguais, só unitárias | iguais | sim |
| `FalhaAoAtualizar` (`main.rs`), no guarda irmão | 8, só unitárias, `AbertaPeloLancador` entre elas | as mesmas 8 | sim, e o guarda continua verde |
| `EndReason` (`seele-ffi/src/types.rs`), no guarda de `MOTIVOS` | só unitárias | as mesmas | sim |

Nenhuma variante que o guarda passou a enxergar ficou sem nome na página. Não houve frase inventada para variante de outro lote.

## Arquivos mudados

- `apps/seele-app/src/mods.rs`
- `apps/seele-app/ui/frases.js`
- `apps/seele-app/tests/frontend.rs`
- `docs/superpowers/sdd/2026-10-08-0.16/F5/lote-p53-report.md` (este)

Nada no fio, nos `api/v*.json`, no `api/README.md` nem nos arquivos da F6.

## Para a v6

Nada. A janela passa a receber `{"TemAtalho": "…"}`, que é interno à casca e não é promessa de API.

## Preocupações

1. **O brief e o código discordavam no teste da A10.** O brief diz que `uma_copia_que_falha_no_meio_nao_deixa_pacote_pela_metade` «continua `NaoCopiei`», mas o teste injetava a falha com um atalho. Segui o brief e troquei a injeção por um arquivo sem permissão de leitura (ver acima). Isso depende de o teste não rodar como root. Os runners do GitHub e este Mac não rodam. Se rodar como root, o teste reprova com frase que diz isso, em vez de passar sem provar nada.
2. **«node_modules» entre aspas angulares, e não entre crases.** O brief escreve `` `node_modules` `` em Markdown. Nenhuma frase da tela usa crase, e a convenção do produto para nome de arquivo é a aspa angular («http://», «mod.json»). O resto da frase do brief está como ele a escreveu.
3. **O brief dizia que as três caíam em `desconhecida`.** Medido no node: as três com campo, sim. `SemManifesto` chega como texto e saía como o nome cru da variante, `SemManifesto`. As quatro agora têm frase.
4. **Quatro pontos ficaram de fora por decisão do controlador**, como pendências para o lote Z registrar. Estão na seção «Pendências para o lote Z», mais abaixo.
5. (Movido para «Pendências para o lote Z».)
6. (Movido para «Pendências para o lote Z».)
7. **O que não rodou.** O Windows não tem atalho sem privilégio nem `PermissionsExt`, então `um_atalho_dentro_do_pacote_nao_e_instalado`, `a_recusa_do_atalho_fica_no_registro_com_o_caminho` e o teste da A10 são `cfg(unix)`. `toda_recusa_de_instalar_fica_no_registro_com_a_pasta` roda em todo sistema. `um_atalho_…` era um `#[test]` com um bloco `cfg(unix)` dentro, que no Windows passava vazio. Agora é `#[cfg(unix)] #[test]`, e no Windows ele não aparece em vez de aparecer verde sem ter conferido nada. Nada disto rodou em Linux nem em Windows aqui. O código é só `std`.
8. **Limites de `variants_of`.** Ele conta `(`, `{` e `[` também dentro de texto entre aspas num atributo. Uma aspa com um parêntese sem par numa linha de atributo o desalinharia, e não há nenhuma hoje. Desde a rodada 1, isso deixa de ser calado: a profundidade tem de voltar a zero no fim do enum, e um desalinhamento reprova o guarda que lê aquele enum. Variante com discriminante explícito (`Nome = 1,`) não é lida, como antes.
9. **O caminho do atalho na linha WARN não passa pelo teto do registro.** A recusa vai pelo `Debug` da variante, que escapa `\n` e `\r`, então ela não forja linha. Mas ela não é cortada em `TETO_DA_FRASE_NO_REGISTRO`, como a pasta é. O tamanho fica limitado pelo do caminho no sistema (1024 bytes no macOS, 4096 no Linux). A revisão não pediu este corte.

## Pendências para o lote Z

Ficam fora deste lote por decisão do controlador, na rodada 1.

1. **A frase de `ManifestoRecusado` mostra o código cru da recusa** (`api-too-old`, `malformed`, `schema-too-new`…) e não manda fazer nada, porque o conserto depende do motivo: para `api-too-new` é atualizar o SEELE, e para `malformed` é quem fez o MOD. Ficaria melhor com um dicionário dos códigos de `refusal_name` (`crates/seele-core/src/mods.rs`), cada um com a frase dele.
2. **O WARN também dispara pelo catálogo, com a pasta de passagem temporária.** `instalar_do_catalogo` (`apps/seele-app/src/catalogo.rs`) escreve os arquivos baixados numa pasta de passagem em `temp_dir()` e chama `instalar_de` sobre ela. Uma recusa ali escreve a linha WARN com `origem=` apontando para essa pasta, que é apagada logo depois. A linha não diz qual MOD do catálogo era, e a pasta que ela nomeia já não existe quando alguém a lê.
3. **`FalhaAoApagarPacote`, em `mods.rs`, está sem frase.** `HashTorto` e `NaoEstaAqui` saem como o nome cru, e `NaoApaguei(String)` sai como «ALGO FALHOU» com o JSON (`apagarUmPacote`, em `camada-mods.js`, usa `fraseDeErro`). O guarda das recusas não a inclui, porque o brief nomeia só `FalhaAoInstalarMod`. Incluí-la reprova o guarda até alguém escrever as três frases.
4. **Duas recusas vizinhas continuam sem linha no registro:**
   - em `instalar_mod` (`main.rs`), um `escolha.into_path()` que falha devolve `SemManifesto` sem passar por `instalar_de`, e por isso sem linha. Na prática, o diálogo de pasta do sistema sempre devolve caminho;
   - `levar_pacotes_antigos_para_o_cache` (`mods.rs`) pula calado um pacote do layout antigo cuja cópia falha, o atalho incluído (`copiar_arvore(...).is_err()` → `continue`).

## Rodada 1 de conserto

A revisão deu «precisa de conserto» por um Importante e três menores.

| Conserto | Commit |
|---|---|
| Importante 1: a linha WARN forjava linha no `seele.log` | `1a7011e7` |
| Menor 3: o doc de `publicar_de` | `1a7011e7` |
| Menor 1: o teste do leitor não prendia a profundidade | `8b6ea89a` |
| Menor 2: a segunda linha da frase de `TemAtalho` | `14253634` |

### Importante 1: um nome de pasta com quebra de linha forjava linha no `seele.log`

O campo era `origem = %origem.display()`, e o formatador não escapa `\n` nem `\r` num campo `%`. O nome da pasta vem de quem empacotou o MOD. Agora o campo é `origem = %crate::caminho_no_registro(&origem.to_string_lossy())`, a mesma função que o resto da casca usa (`main.rs`): entre aspas, com escape, e cortado no teto do registro. A função é privada na raiz do crate, e `mods` é filho dela, então já a enxergava. `main.rs` não mudou.

Teste novo, `uma_pasta_com_quebra_de_linha_no_nome_nao_forja_linha_no_registro` (`#[cfg(unix)]`). Ele usa uma pasta chamada `mod\n2026-10-08T00:00:00.000000Z  INFO seele_app: tudo certo`, sem `mod.json`, e exige três coisas: que a recusa seja `SemManifesto`, que nenhuma linha do rastro comece com o texto forjado, e que haja uma linha WARN com a pasta na forma escapada.

**RED**, com o código de antes:

```
thread 'mods::instalar::uma_pasta_com_quebra_de_linha_no_nome_nao_forja_linha_no_registro' panicked at apps/seele-app/src/mods.rs:794:9:
o nome da pasta escreveu no `seele.log` uma linha com a cara do produto, e quem lê o registro não tem como saber que ela é forjada:
 WARN seele_app::mods: não instalei o MOD desta pasta origem=/var/folders/…/seele-instalar-mod-…-forjada/mod
2026-10-08T00:00:00.000000Z  INFO seele_app: tudo certo recusa=SemManifesto
test result: FAILED. 15 passed; 1 failed
```

**GREEN.** A mesma pasta, agora numa linha só:

```
 WARN seele_app::mods: não instalei o MOD desta pasta origem="/var/folders/…/mod\n2026-10-08T00:00:00.000000Z  INFO seele_app: tudo certo" recusa=SemManifesto
```

`cargo test -p seele-app --bin seele-app mods`: 41 verdes.

**O Windows.** `toda_recusa_de_instalar_fica_no_registro_com_a_pasta` roda no `windows-2022` e comparava com `origem.display()`. Lá, o escape do `Debug` dobra a barra invertida, e a comparação reprovaria mesmo com o registro certo. Ela passou a comparar com `crate::caminho_no_registro(&origem.to_string_lossy())`, a mesma forma que a linha escreve. Isto não rodou em Windows aqui. A comparação é a da própria função, e não uma cópia da regra.

**Reversão.** Com o campo de volta a `%origem.display()`, dois testes reprovaram: o da quebra de linha, com a linha forjada no rastro, e o de toda recusa, que agora exige a forma escapada (`uma recusa de instalar não deixou linha WARN com a pasta no seele.log: Err(SemManifesto)`). O arquivo voltou da cópia guardada, com `cmp` igual.

### Menor 1: o teste do leitor não prendia a profundidade

A revisão estava certa, e medi antes de mexer. Com o laço da profundidade apagado de `variants_of`, `variants_of_le_toda_forma_de_variante_e_nao_cala_a_que_tem_campo` e os três guardas que usam o leitor **passaram**, 4 de 4. Os campos de uma `struct` já caíam pelo `:` depois do nome, e nada na fonte sintética precisava da contagem.

O conserto:

- a fonte sintética ganhou `Longa(\n    String,\n),`, uma tupla de várias linhas, onde o `String,` de dentro tem a cara de uma unitária;
- `variants_of` termina com `assert_eq!(profundidade, 0, …)`, e a mensagem diz que um parêntese ou uma chave sem par desalinhou a leitura. A variável passou de `depth` a `profundidade`;
- o doc diz o mecanismo certo: os campos de `struct` caem pelo `:`, e a profundidade é para a tupla de várias linhas e para o tipo de um campo que quebra de linha.

**Reversão.** Com o laço da profundidade apagado de novo, o teste do leitor reprova, e os três guardas seguem verdes:

```
  left: ["Unitaria", "ComCampo", "ComDois", "Longa", "String", "Struct", "NaMesmaLinha", "Ultima"]
 right: ["Unitaria", "ComCampo", "ComDois", "Longa", "Struct", "NaMesmaLinha", "Ultima"]
test variants_of_le_toda_forma_de_variante_e_nao_cala_a_que_tem_campo ... FAILED
```

**E o `assert_eq!` do fim também dispara.** Com um parêntese sem par no atributo da fonte sintética (`#[serde(rename = "x(")]`), o teste reprova em `variants_of` com `left: 1, right: 0`. Nos dois casos o arquivo voltou da cópia guardada, com `cmp` igual.

`cargo test -p seele-app --test frontend`: 263 verdes. `FalhaAoAtualizar` continua com 8.

### Menor 2: a segunda linha da frase de `TemAtalho`

Pela decisão do controlador, a segunda linha passou a ser «Tire o atalho da pasta e instale de novo. Num MOD em desenvolvimento, eles costumam estar em «node_modules».». A primeira ficou como estava. Medida no node, sem o caminho interpolado, a frase tem 174 caracteres e duas linhas. O `LIMITE_DE_FRASE` é 180. Esta frase é montada em `fraseDeErro` e não está em dicionário, então nenhum guarda de tamanho a mede: a medida é a do node. Ela fica a 6 caracteres do teto.

### Menor 3: o doc de `publicar_de`

Ele dizia «cada `return` daqui é uma recusa», e existe o `return Ok(Publicado { … })` do pacote que já está no cache. Agora diz que **toda saída `Err`** é uma recusa, e que a outra saída antecipada é `Ok`.

### O que rodou na rodada

- `cargo test -p seele-app --bin seele-app mods`: 41 verdes.
- `cargo test -p seele-app --test frontend`: 263 verdes.
- `cargo xtask check-runtime`: saiu 0, com as 7 bancadas.
- `cargo fmt --all -- --check` e `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`: limpos.

## Rodada 2: o texto da frase do atalho

Na segunda linha de `TemAtalho`, «eles» não tinha antecedente plural, porque «o atalho» é singular. Ela passou a ser, como o controlador escreveu: «Tire o atalho e instale de novo. Num MOD em desenvolvimento, os atalhos costumam estar em «node_modules».». Medida no node, sem o caminho, a frase tem 171 caracteres e duas linhas. `cargo test -p seele-app --test frontend` deu 263 verdes, e `cargo xtask check-runtime` saiu 0, com as 7 bancadas.
