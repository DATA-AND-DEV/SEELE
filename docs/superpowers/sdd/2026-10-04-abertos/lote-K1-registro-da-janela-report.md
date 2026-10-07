# Lote K1-registro-da-janela: relatório

Base 39b1e58. Branch conserto/abertos-da-1.0. Quatro commits locais, sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| b655edf | P51-04 | `atenderOMod` registra a recusa por versão e o tipo desconhecido; o `catch` registra `motivoDaFalha` (com JSON quando ela daria «[object Object]»); a bancada carrega `frases.js` |
| 4599e14 | P51-13 | bloco novo «o elo» em `contribuicoes-e-camadas.cjs`, num `vm.createContext` próprio; o produto não muda |
| ef707c7 | P51-06 | `registrarNoAnfitriao` troca substituto solto por U+FFFD em `onde`, `oQue` e `modId`; `chaveDoMod` cortada por ponto de código; casos (a) e (b) |
| d744644 | só texto | corrige três frases minhas (b655edf) e um comentário do `default` |

HEAD: d744644. Arquivos mudados (39b1e58..HEAD), só os quatro da lista:
`apps/seele-app/ui/base.js`, `apps/seele-app/ui/mods-regiao.js`,
`apps/seele-app/bancada/contribuicoes-e-camadas.cjs`, `apps/seele-app/bancada/regiao-do-mod.cjs`.

Nada no Rust, no fio, na API congelada, na CSP nem em `docs/`. Nenhuma dependência nova e nada baixado.

## 1. P51-04: as recusas do roteador chegam ao registro (b655edf)

**Produto (base.js, `atenderOMod`).**
- Recusa de `podePedir`: antes do `responder`, `registrarNoAnfitriao("atender-mod", `${mod.id}: «${m.tipo}» não existe na API ${mod?.api ?? "?"}`, "aviso", mod.id)`.
- `default`: `registrarNoAnfitriao("atender-mod", `${mod.id}: a API de MODs não conhece «${m.tipo}»`, "aviso", mod.id)`.
- `catch`: o registro usa `motivoDaFalha(falha)`. Quando ela devolve «[object Object]», usa `JSON.stringify(falha)` dentro de um `try`, porque um ciclo faria o `stringify` lançar dentro do `catch`. `fraseDeErro` ficou só no `responder`, com o mesmo `typeof` de antes.

**Bancada (contribuicoes-e-camadas.cjs).**
- `frases.js` inteiro entra no contexto compartilhado logo depois dos arquivos do produto, num `runInContext` só dele: o arquivo abre com `"use strict"`, que só vale no começo de um script. Medi antes de decidir o lugar. Com o arquivo carregado no HEAD, todos os blocos continuaram verdes, e um `fraseDeErro` instrumentado foi chamado 3 vezes, todas por `atenderOMod` no R2. No estado final são 4 (o caso novo do `snapshot`), e de novo todas de `atenderOMod`. Nenhum nome do `frases.js` colide com uma das 19 propriedades que a bancada escreve no `contexto`.
- O recorte de `motivoDaFalha` (antes no R4e) subiu para junto do recorte do roteador, porque agora o `catch` a chama. Como na página, `mods-regiao.js` vem antes de `base.js`.
- A troca de `contexto.registrarNoAnfitriao` passou para antes da chamada de API 3 (n:2). Se o roteador lançar, o `catch` do fim encerra a bancada, e por isso a troca não precisa de `finally`.
- Asserções novas:
  - `dita("«contribuir»", "não existe na API 3")`;
  - a linha que contém «nao.existe» existe e não contém «ALGO FALHOU»;
  - depois de `tipo: "inexistente"`: `ok === false` e `dita("não conhece «inexistente»")`;
  - `snapshot` com `contexto.invoke` rejeitando `{ Refused: { reason: "Teste" } }`: `ok === false`, a linha leva `{"Refused":{"reason":"Teste"}}`, e nenhuma linha leva «[object Object]».

**Desvios da lista, de propósito:**
- `dita("não conhece")` ao pé da letra seria verde no HEAD. A linha do ponto «nao.existe» já diz «a API de MODs não conhece o ponto «nao.existe»», e o `desconhecida` repete a mensagem do `Error`. Por isso a asserção exige `não conhece «inexistente»`. Pelo mesmo cuidado, a de API 3 exige `«contribuir»` e `não existe na API 3`.
- O caso do `snapshot` não estava na lista. Ele prova o fallback de JSON que a lista pede no produto. Sem ele, nenhum teste exercitava esse ramo.

**RED (bancada nova contra o produto de 39b1e58):** 4 falhas.
- `R2 · a versão no registro: a recusa por versão foi dita só ao MOD, …: []`
- `R2 · a recusa no registro: a linha da contribuição recusada levou ao seele.log a frase de tela de \`fraseDeErro\`, …: ["atender-mod","mod/b: «contribuir» falhou — ALGO FALHOU E ESTE APP NÃO SABE EXPLICAR O QUÊ.…`
- `R2 · o tipo desconhecido no registro: a mensagem que a API não conhece foi recusada só para o MOD, …`
- `R2 · a variante da FFI: … [... "mod/b: «snapshot» falhou — SESSÃO RECUSADA" ...]`. O registro antigo trocava o `Refused` pela frase de tela.

**GREEN:** `contribuicoes-e-camadas` passa, as seis do `check-runtime` passam, e `frontend.rs` passa (259).

**Reversões** (script em scratchpad: troca o trecho, roda a bancada, restaura, compara sha256 antes e depois). Todas voltaram IDÊNTICO.

| mutação | vermelho |
|---|---|
| registro volta a `typeof fraseDeErro === "function" ? fraseDeErro(falha) : …` | `R2 · a recusa no registro: … a frase de tela de \`fraseDeErro\`, e não só o motivo` (e `R2 · a variante da FFI`) |
| tirar a linha nova do `podePedir` | `R2 · a versão no registro: a recusa por versão foi dita só ao MOD …` |
| tirar a linha nova do `default` | `R2 · o tipo desconhecido no registro: …` |
| tirar o fallback de JSON | `R2 · a variante da FFI: … "mod/b: «snapshot» falhou — [object Object]"` |
| mutação dupla: a bancada **sem** `frases.js` e o produto com `fraseDeErro` no registro | **verde**. Isso prova que carregar `frases.js` é o que dá dente ao guarda do «ALGO FALHOU» |

## 2. P51-13: o elo da janela até o invoke (4599e14)

Bloco novo, antes do R2, num `vm.createContext` próprio. O `contexto` compartilhado não é tocado.
- **Recorte.** `function registrarNoAnfitriao(` e `function donoDaRegiao(` vêm de `base.js`, cada um até o primeiro `\n}\n`. Um `confere` falha alto se o recorte não achar a declaração, e os casos só rodam se as duas funções existirem.
- **Contexto.**
  - `invoke` de mentira que anota `{cmd, args}` e devolve `Promise.resolve()`;
  - `modsCarregados` (Map) e `geracaoDaSessao: 7`;
  - instância `{ geracao: 7, admite: () => true }`;
  - **`avisarQueAMidiaMudou: () => {}`**, que a lista não citava. `donoDaRegiao` lê esse nome ao montar o objeto (`midiaMudou: avisarQueAMidiaMudou`), e sem ele o recorte lança `ReferenceError`.
- **(1)** Com a instância no mapa, `anotarRecusa('x')` gera exatamente um invoke, e ele é um `registrar_da_janela` com `{nivel:'aviso', onde:'recusa-de-mod', oQue:'mod/a: x', modId:'mod/a'}`. A conferência é campo a campo, sem campo a mais e em qualquer ordem.
- **(2)** Nenhum invoke em três situações:
  - mapa vazio;
  - outra instância no lugar;
  - a mesma instância com `admite: () => false`. Esse terceiro caso é um acréscimo meu.
- **(3)** `registrarNoAnfitriao('o','t','erro','mod/a')` repassa `{nivel:'erro', onde:'o', oQue:'t', modId:'mod/a'}`.
- Acréscimo: `typeof dono.anotarRecusa === "function"` com frase própria, e a chamada com `?.`. Sem isso, um dono sem `anotarRecusa` derrubaria a bancada com `TypeError` em vez de reprovar pela frase.

**Medida da premissa** (o comentário do bloco a afirma, e eu medi de novo):
- Com cada uma das duas mutações da lista e a bancada de 39b1e58/b655edf (sem o bloco), as seis bancadas do `check-runtime` saíram com 0.
- Com cada uma das duas mutações, o `frontend.rs` deu `259 passed`.

**GREEN contra o produto atual.** O item é só de teste, então o bloco passa de cara, e o dente dele está nas reversões:

| mutação | vermelho |
|---|---|
| tirar `modId` do objeto do invoke (`base.js` 449 na base) | (1) `o elo · anotarRecusa: … [{"cmd":"registrar_da_janela","args":{"nivel":"aviso","onde":"recusa-de-mod","oQue":"mod/a: x"}}]` e (3) `o elo · registrarNoAnfitriao: … não repassou … o nível e o id do MOD` |
| inverter `if (!meu()) return;` em `anotarRecusa` (790 na base) | (1) `…: []` e (2) nos três casos: `sem instância carregada, a recusa ainda foi ao registro`, `com outra instância no lugar, …`, `com a sessão que já não é a de pé, …` |
| (acréscimo) tirar `&& instancia.admite(geracaoDaSessao)` de `meu` | `o elo · anotarRecusa fora da vez: com a sessão que já não é a de pé, a recusa ainda foi ao registro` |
| (acréscimo) renomear `anotarRecusa` no dono | `o elo · anotarRecusa: o dono de \`base.js\` não tem \`anotarRecusa\`…`, sem `TypeError` |
| recorte que não acha (`function donoDaRegiaoQueSumiu(` na bancada) | `o elo · o recorte: «function donoDaRegiaoQueSumiu(» mudou de forma, e o recorte até o primeiro «\n}\n» não a achou` |

Rodei M1 a M3 de novo depois da troca para `?.`, com os mesmos vermelhos. Todas as restaurações deram IDÊNTICO.

## 3. P51-06: substituto solto e o corte da chave (ef707c7)

**Produto.**
- `registrarNoAnfitriao`: `const bem = (texto) => String(texto).replace(/[\uD800-\uDFFF]/gu, "�")`, aplicado a `onde` e a `o_que`, e a `modId` só quando não é nulo: `modId == null ? null : bem(modId)`. Sem `toWellFormed`. As chamadas a `bem` ficam dentro do `try` que já existia.
- `mods-regiao.js`, na antiga linha 809: `elem.dataset.chaveDoMod = [...plano.no.chave].slice(0, 120).join("")`, sem trocar substituto.
- Conferi antes de escrever o comentário:
  - `node -e 'JSON.stringify({o:"avatar\uD800"})'` dá `{"o":"avatar\ud800"}`;
  - no serde_json 1.0.151 (o do Cargo.lock), `read.rs` `parse_unicode_escape` com `validate` dá erro no substituto solto;
  - `tauri.conf.json:42` diz `"minimumSystemVersion": "11.0"`.

**Testes.**
- (a) No bloco do elo: `registrarNoAnfitriao("onde\uDC00", "avatar\uD800 😀", "aviso", "mod/a\uD800")` exige `isWellFormed()` em `onde`, `oQue` e `modId`, e `oQue === "avatar� 😀"`. Isso prova que o par do emoji fica intacto, ou seja, a flag `u`. Sem `modId`, exige `args.modId === null`.
- (b) Em `regiao-do-mod.cjs`, a prova nova `aChaveDoCampoNaoPartePar` usa um campo com a chave `"a".repeat(119) + "😀"` e exige `isWellFormed()` e `endsWith("😀")` em `dataset.chaveDoMod`.

**RED:**
- (a) `o elo · o substituto solto: … [{"cmd":"registrar_da_janela","args":{"nivel":"aviso","onde":"onde\udc00","oQue":"avatar\ud800 😀","modId":"mod/a\ud800"}}]`
- (b) `FALHOU — a chave do campo: … o MOD recebe de volta outra chave: "aa\ud83d"`
- O caso «sem MOD» já era verde, porque o padrão `modId = null` já existia. Ele guarda a escolha `modId == null ? null : …`.

**GREEN:** as duas bancadas passam, `regiao-do-mod` com 35 provas, as seis do `check-runtime` passam e `frontend.rs` dá 259.

**Reversões:**

| mutação | vermelho |
|---|---|
| tirar o `replace` (`bem = String(texto)`) | `o elo · o substituto solto: … "onde\udc00" … "avatar\ud800 😀" … "mod/a\ud800"` |
| tirar a flag `u` | `o elo · o substituto solto: … "oQue":"avatar� ��"`: o par do emoji também foi trocado |
| `modId: bem(modId)` sem o teste de nulo | `o elo · sem MOD: … "modId":"null"` |
| voltar ao `slice(0, 120)` em `chaveDoMod` | `FALHOU — a chave do campo: … "aa\ud83d"` |

Todas as restaurações deram IDÊNTICO no sha256.

**Frases que o conserto tornou falsas, corrigidas no mesmo commit:**
- O doc de `dizerRecusaDeMidia` (`mods-regiao.js`) dizia que meia chave fazia «a linha inteira se perder calada no `catch` de `registrarNoAnfitriao`». Agora ela chega com «�».
- O comentário e a mensagem da prova «recusa dita · chave longa» (`regiao-do-mod.cjs`) diziam que a ponte recusa a frase inteira. Agora dizem que a chave chega com «�».

**Também medido:**
- O guarda `todo_argumento_que_o_frontend_manda_existe_no_comando` continua lendo o `invoke` depois que ele foi quebrado em várias linhas. Trocar `oQue:` por `oQueX:` reprova com `invoke("registrar_da_janela") manda \`oQueX\``, e restaurei.
- A bancada de navegador `diagnostico-de-mods.cjs`, que lê `oQue` do `invoke`, passou com o Playwright local.

## 4. Só texto (d744644)

Três frases que eu tinha escrito em b655edf estavam imprecisas:
- «no lugar do motivo». `desconhecida` acrescenta a mensagem do `Error` depois do pedido de desculpas, então o motivo estava lá, atrás da frase de tela. Corrigi no comentário que carrega `frases.js` e na mensagem da asserção («e não só o motivo»).
- A medida «as três chamadas», que mudou para 4 com o caso do `snapshot`. A frase agora não depende de contagem.
- O comentário do `default` de `atenderOMod` («E quem hospeda também») lia como se quem hospeda também ficasse esperando.

Conferi cada frase contra o código. Rodei de novo a reversão do «ALGO FALHOU», e ela reprova com a mensagem nova.

## Comandos rodados (estado final)

- `node apps/seele-app/bancada/contribuicoes-e-camadas.cjs` e `regiao-do-mod.cjs`: verdes.
- `cargo xtask check-runtime`: as seis verdes.
- `cargo test -p seele-app`: todos os binários verdes (188, 22, 2, 5, 259, 9, 4, 3).
- `cargo fmt --all -- --check`: limpo. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: limpo; o único aviso é o ambiental `ld: duplicate -rpath`.
- `PLAYWRIGHT=… node apps/seele-app/bancada/diagnostico-de-mods.cjs`: verde.
- O assunto de cada commit foi conferido contra as catorze cadeias do G1 (plano 1B): nenhuma aparece.

## O que não pôde ser compilado nem rodado aqui

- Nenhum código com `cfg` de outra plataforma foi tocado.
- O WKWebView (macOS 11), o WebView2 e o WebKitGTK não se medem aqui. A regex com a flag `u` é ES2015 e o espalhamento `[...s]` também. A afirmação de que o `serde_json` recusa o substituto vem da leitura do `read.rs` e não de uma ida e volta medida num WebView com o Tauri.
- O resto das bancadas Playwright e a bateria inteira (`cargo test --workspace`) ficam para o passo do lote.

## Autorrevisão

- Escopo: só os quatro arquivos da lista. Nenhuma linha no Rust do registro (lote C), nenhum evento novo na API, nenhum `toWellFormed`, nenhum recorte de `base.js` rodado no contexto compartilhado (os recortes do R2 já existiam e rodam lá como antes; o bloco novo tem contexto próprio). Nenhuma pendência e nenhum índice editados.
- Cada asserção nova tem mensagem em português que diz o que quebra.
- Os acréscimos à lista são todos guardas extras provados por reversão:
  - o caso do `snapshot` com JSON;
  - o caso `admite` falso;
  - o `typeof anotarRecusa`;
  - `onde` e `modId` com substituto;
  - o par preservado.
- Apertos de asserção para não ficarem verdes no HEAD:
  - `não conhece «inexistente»`;
  - `«contribuir»` + `não existe na API 3`.
- Revisei os comentários dos quatro commits contra o código. Quatro frases estavam imprecisas ou ficaram falsas: três minhas (corrigidas em d744644) e uma pré-existente, que o próprio conserto tornou falsa (corrigida em ef707c7).

## Preocupações

1. **O MOD continua recebendo a frase de tela.** O `responder` do `catch` ainda manda ao MOD `fraseDeErro(falha)`, que para um `Error` da janela é «ALGO FALHOU E ESTE APP NÃO SABE EXPLICAR O QUÊ.\nSe for relatar, copie o texto abaixo:\nError: 3 nó(s) não couberam…». A lista mandou manter assim («`fraseDeErro` fica só no `responder`»), e o comportamento vem de antes. Mas quem escreve MOD lê esse pedido de desculpas no `erro` de toda recusa que passa pelo `catch`. Fica como pergunta para um lote futuro, ou para o dono.
2. **O `catch` depende de `motivoDaFalha` ser global.** Ela vem de `mods-regiao.js`, e não há `typeof` de proteção. É de propósito: foi um `typeof` que deixou a bancada medir outro ramo. Hoje só o `index.html` carrega `base.js`, depois de `mods-regiao.js`. Uma página futura que carregue `base.js` sem `mods-regiao.js` faria o `catch` lançar `ReferenceError`, e o MOD ficaria sem resposta.
3. **Um comentário do `frontend.rs` ficou superado** (`frontend.rs` não está na lista do K1). O doc de `o_dono_da_regiao_leva_cada_recusa_de_midia_ao_registro` (`frontend.rs` ~8148-8167) diz que as seis bancadas do `check-runtime` «ficam verdes com o dono sem `anotarRecusa` (medido em 01/10/2026)». Desde 4599e14, a `contribuicoes-e-camadas` reprova nesse caso; medi renomeando `anotarRecusa`. A frase é datada, e quem mexer no `frontend.rs` (C ou Z) pode atualizá-la.
4. **As duas linhas novas podem ser provocadas em laço por um MOD**, como a do `catch` já podia. O teto de vazão delas é o P51-08, do lote C, como a lista já diz.
5. **Mudança de comportamento hipotética.** `registrarNoAnfitriao` agora passa `onde` e `modId` por `String()`. Antes um `modId` que não fosse texto ia cru e o serde o recusava. Todas as chamadas atuais passam texto, então isso só pesa num chamador futuro.
6. **Observação fora do escopo.** `mods-regiao.js` ainda corta outros textos por índice, e não mexi em nenhum, porque a lista limita o item à linha 809. São dois tipos:
   - **Só tela.** `textoDoRotulo` (385, que vai para `aria-label` e `title`), `nomeAcessivel` e as opções. Um par partido ali aparece como «�».
   - **Vai a um `invoke`.** A `finalidade` de `pedidoDeArquivo` (402, cortada em 200) é mandada ao `escolher_para_o_mod`. Pela mesma leitura do `serde_json`, um par partido ali faria o pedido inteiro ser recusado. Não medi.

## Rodada 1

HEAD continua d744644. Esta rodada não tem commit: nenhum achado pediu mudança nos quatro arquivos do lote. Os três Minor ficaram registrados, cada um com o motivo abaixo. O Important da bateria foi medido e não é do lote. A árvore terminou limpa (`git status` vazio). O `macos.rs` mexido para a medida voltou byte a byte, com o sha256 `9c4e5e07…c31d5` conferido por `shasum -c`.

### BAT-0-1 (Important): medido, não é do lote → NEEDS_CONTEXT

**O lote não toca nada que esse teste compila.** De 3685a64 (base do branch) até d744644, `crates/seele-video`, `Cargo.lock`, `Cargo.toml`, `.cargo` e `rust-toolchain.toml` são iguais (`git diff --quiet` deu 0). O `seele-video` também não depende de crate nenhum do workspace (seu `Cargo.toml` não tem `path =`). O diff do K1 (39b1e58..d744644) só toca os quatro arquivos JS/cjs.

**Reproduzido.** Rodei sozinho, três vezes, `cargo test -p seele-video --lib captura::macos::testes::a_captura_entrega_quadros_ao_longo_do_tempo`, e as três reprovaram com `entregou 17 quadros … escritos pelo sistema: 17`.

**A medida que separa as hipóteses.** Pus em `macos.rs` um módulo de teste temporário que roda a mesma captura (P720, Q30, 3 s, o mesmo laço de 5 ms) em **cada** monitor de `fontes()`. Ele conta `escritos`, `sem_conteudo`, `ilegiveis` e `descartados`, e os escritos por segundo. Depois tirei o módulo e restaurei o arquivo (sha256 conferido). A ordem é a de `fontes()`, e o teste usa o primeiro `Fonte::Monitor` da lista:

| monitor (ordem de `fontes()`) | pegos | escritos | sem_conteudo | escritos por segundo |
|---|---|---|---|---|
| id=2, 1080×1920 (o externo vertical, 144 Hz): **o que o teste usa** | 17 | 17 | 73 | [17, 0, 0] |
| id=5, 2560×1440 (o principal, 240 Hz, onde há atividade) | 88 | 89 | 0 | [30, 29, 29] |
| id=1, 1512×982 (o embutido) | 26 | 26 | 63 | [26, 0, 0] |

Ambiente medido: M5 Pro, três monitores ligados, tela acesa e desbloqueada, e `UserIsActive` com o mouse.

**O que a medida diz.**
- **A captura não travou.** No monitor que o teste usa, `escritos + sem_conteudo` dá 17 + 73 = 90, que são exatamente 30 por segundo durante 3 s. O ScreenCaptureKit entregou todas as amostras. Do segundo 1 em diante, todas vieram sem conteúdo, e o código as conta em `sem_conteudo` e não as põe na vaga, como manda o doc de `Vaga::sem_conteudo`.
- **A premissa da mensagem do teste é falsa nesta máquina.** A mensagem diz «A tela parada não explica: o ScreenCaptureKit reentrega o mesmo quadro». O que se mediu é o contrário: numa tela parada o ScreenCaptureKit não reentrega o quadro, ele manda amostras sem pixels. O próprio doc de `Vaga::sem_conteudo` já diz isso.
- **Corrige a hipótese da bateria.** O sistema não escreve «uns 5–6 quadros por segundo durante 3 s». Ele escreve 17 no primeiro segundo e zero depois, e manda as outras 73 amostras como `sem_conteudo`.
- **O resultado depende do monitor que vem primeiro e de a tela dele mudar.** No monitor com atividade a mesma captura dá 88. A base (17, medida pela bateria) e o HEAD (17, 17, 17) reprovam igual.

**Por que NEEDS_CONTEXT e não conserto.** O defeito está no teste do `seele-video`, fora dos arquivos do K1 e de qualquer lote da onda 1. O conserto natural é o teste cobrar que as amostras continuem chegando, contando `escritos + sem_conteudo` (ou um mínimo por segundo), com a mensagem corrigida. Mas isso muda o que o guarda prova: o relato «exibe apenas 1 frame» precisa continuar reprovando. Por isso a decisão é de quem coordena: em qual lote entra, ou se vai como pendência.

### K1-m1 (Minor): não consertado, fora da lista de arquivos

- **Achado conferido.** O doc está em `apps/seele-app/tests/frontend.rs:8165-8167`, e a frase é a citada. Medi o mesmo na rodada 0 (preocupação 3).
- **Por que fica.** `frontend.rs` não está entre os arquivos do K1, e o próprio conserto do achado manda o acréscimo a quem mexer depois nele.
- **Correção da rota.** O achado diz «lote C ou Z», mas pela `lotes.md` nem o C nem o Z têm `frontend.rs` na lista de arquivos. Os lotes que ainda vão mexer nele são H, K3, J, G e I. O primeiro na ordem é o H (onda 2, ordem 7). Quem coordena escolhe, ou acrescenta o arquivo ao Z.

### K1-m2 (Minor): não consertado, fora do escopo; agora medido

- **Por que fica.** O título do próprio achado diz «fora do escopo», e o conserto dele é entrar no inventário como item de um lote futuro. A lista do K1 limita o P51-06 à linha 809. A `finalidade` vai ao `escolher_para_o_mod`, e não ao registro, que é o caminho deste lote. Não editei o `inventario.json`, porque é de quem coordena.
- **Medido, já que a rodada 0 tinha dito «não medi».**
  - O caminho: `mods-regiao.js:402` (`finalidade.slice(0, 200)`) → `base.js:905` `invoke("escolher_para_o_mod", { … finalidade … })` → `main.rs:5834` `finalidade: Option<String>`.
  - Com `"a".repeat(199) + "😀 depois"`, o node dá `isWellFormed() === false`. O corpo de `JSON.stringify` leva `\ud83d` solto.
  - Esse corpo, lido pelo `serde_json` 1.0.151 (o do `Cargo.lock`, do cache local, `cargo run --offline` num crate de scratchpad), é recusado: `unexpected end of hex escape at line 1 column 246`.
  - Medi o `serde_json` e não a ida e volta pelo Tauri num WebView.
- **Lugar natural.** O K2 (onda 2, ordem 8) já tem `mods-regiao.js` e `regiao-do-mod.cjs` na lista. O conserto é `[...finalidade].slice(0, 200).join("")`, com uma prova pelo molde da `aChaveDoCampoNaoPartePar`.
- **Mais um ponto do mesmo tipo.** `atualizarArquivo` (`mods-regiao.js:1149`) corta a mesma `finalidade` por índice, mas ela só vai ao nome acessível, então é «só tela».

### K1-m3 (Minor): não consertado, é decisão do dono

- **Achado conferido.** Em `base.js:2240-2242`, o `responder` do `catch` de `atenderOMod` ainda manda `fraseDeErro(falha)` ao MOD. A lista mandou manter assim, e o comportamento vem de antes do lote.
- **Por que fica.** Mudar o texto que o MOD recebe no `erro` muda o que a API entrega a quem escreve MOD. Antes de mexer, é preciso conferir se isso toca a API congelada (`api/v3..v5.json`), e isso é decisão do dono ou de um lote que a peça. É a mesma pergunta da preocupação 1 da rodada 0.

### Comandos desta rodada

- `cargo test -p seele-video --lib captura::macos::testes::a_captura_entrega_quadros_ao_longo_do_tempo`, três vezes: vermelho, 17/17/17.
- O módulo temporário `medida_temporaria_k1` (já removido): a tabela acima.
- `node` + `cargo run --offline` do `serde-medida` em scratchpad: a recusa citada.
- `shasum -a 256 -c` do `macos.rs`: OK. `git status --short`: vazio.
- Sem fmt nem clippy, porque nenhum arquivo do repositório mudou.
