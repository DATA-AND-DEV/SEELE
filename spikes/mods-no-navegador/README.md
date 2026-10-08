# spike `mods-no-navegador` — onde o código de um MOD roda no celular?

**Descartável.** Responde a pergunta que a decisão D-web-1 de 08/10 abriu, e
morre com a resposta.

## A pergunta

O dono decidiu que **os MODs rodam no celular**
(`docs/superpowers/specs/2026-10-08-0.16-design.md`, §2 F4). No celular, a
identidade da pessoa, uma `CryptoKey` Ed25519 não extraível, mora no IndexedDB
da origem da página. Um MOD que alcance essa origem assina o desafio de qualquer
servidor como a pessoa. **Um MOD não pode rodar com acesso a ela.**

O desktop já passou por isso. O executor saiu de um Worker de `blob:` para o
QuickJS nativo depois que um MOD gravou em IndexedDB uma marca que sobreviveu ao
reinício (`apps/seele-app/src/executor.rs:1-35`).

A spec traz duas candidatas e cinco critérios:
- **A**: o QuickJS compilado para `wasm`, num Worker, com o mesmo motor e o
  mesmo prelúdio do desktop;
- **B**: um `iframe` com `sandbox`, origem opaca e uma CSP sem rede.

Os critérios:
1. nenhum armazenamento que sobreviva à sessão;
2. nenhuma rede;
3. nenhum acesso à origem da página;
4. a mesma API do desktop;
5. o custo medido.

O spike mede também **AB**, as duas juntas: o motor da A num Worker criado de
dentro do `iframe` opaco da B.

## Como roda

```sh
rustup target add wasm32-wasip1
(cd motor && cargo build --release --target wasm32-wasip1)
node teste-motor.mjs              # o motor fora do navegador
python3 prova.py chromium         # Chromium 151 do Playwright
python3 prova.py safari-sim       # Safari do Simulador do iOS (um simulador ligado)
python3 prova.py iphone           # só o custo, no aparelho (o dono)
```

O primeiro build do motor baixa o wasi-sdk 24.0 (ver «O custo de compilar»).

**As peças:**
- **`motor/`** é a candidata A: o `rquickjs` 0.12.2 do desktop (quickjs-ng
  0.15.1), compilado para `wasm32-wasip1`.
  - O `build.rs` extrai o `PRELUDIO` de `apps/seele-app/src/executor.rs`, sem
    cópia.
  - A ponte é a do desktop: `seele.postar` e `seele.console`, mais
    `__seeleCapacidades` e `__seeleTetoDaMensagem`.
  - Os limites são os do desktop: 8 MiB, 500 ms e 200 000 consultas por volta.
- **`pagina/motor-anfitriao.js`** instancia o `.wasm`, dá a ele as três funções
  do módulo `seele` e a WASI mínima, e faz a volta do desktop: avaliar, escoar
  os jobs e recolher os pedidos de relógio.
- **`pagina/candidato-quickjs-worker.js`** é o Worker da A. Ele é servido com a
  CSP `default-src 'none'; script-src 'self' 'wasm-unsafe-eval'`.
- **`pagina/candidato-iframe.html`** é a B. Ela é servida com
  `default-src 'none'; script-src 'sha256-<o script>' 'unsafe-eval'; webrtc 'block'`.
  O prelúdio e o MOD rodam por `eval` indireto, no motor do navegador.
- **`pagina/candidato-ab.html`** é a AB: um `iframe` opaco que cria de um
  `blob:` o Worker da A. A CSP é
  `default-src 'none'; script-src 'sha256-…' 'wasm-unsafe-eval'; worker-src blob:`.
- **`pagina/anfitriao.js`** é a página do app:
  - ela tem a CSP do ADR 0057 (`script-src 'self' 'wasm-unsafe-eval'`, sem
    `unsafe-inline`);
  - ela guarda uma identidade como o celular guardaria (`seele-identidade` no
    IndexedDB, chave não extraível), mais uma marca em `localStorage` e um
    cookie;
  - os três executores têm a forma dos quatro verbos de `ui/mods-runtime.js`.
- **`pagina/mod-comum.js`** é um MOD que só usa a API do prelúdio:
  `snapshot`, `request`, `regiao`, `capacidades`, `aoEvento`, `setTimeout`,
  `setInterval` e `console`. Ele recebe uma recusa da página e manda uma
  mensagem de 14 KB, que o prelúdio tem de recusar contando UTF-8. A página
  confere cada resposta.
- **`pagina/mod-hostil.js`** faz 35 tentativas no relatório, mais três navegações e dois testes de recurso:
  - armazenamento, inclusive abrir o banco da página e assinar com a chave da
    pessoa;
  - a página;
  - 17 saídas de rede, cada uma com um caminho próprio até uma canária;
  - três navegações, cada uma num executor novo;
  - um laço de 3 s e 128 MiB de memória.

  Na fase 1 ele grava marcas. Entre as fases **o navegador é fechado**: no
  Chromium, o perfil persistente é reaberto; no Simulador, o Safari é
  encerrado. Na fase 2 ele procura as marcas.
- **`prova.py`** serve tudo e mantém as canárias:
  - HTTP em `127.0.0.1:8096`, outra origem, que aceita qualquer pedido e
    responde com CORS aberto;
  - UDP em `8097`, `8098` e `8099`, uma porta por candidata, distinguindo STUN
    de QUIC.

  **Quem dá o veredito da rede são as canárias**, e não o MOD: um pedido que o
  MOD acha que falhou e que chegou é vazamento.

## O que mediu — 08/10/2026

No Mac (aarch64), no `d4eb6f1`, contra o `executor.rs` da v0.15.1 publicada.
Chromium 151 headless (Playwright) e Safari do Simulador iOS 27.0 (24A434).

### Os critérios

| critério | A · QuickJS num Worker | B · iframe opaco | AB · QuickJS num Worker dentro do iframe |
|---|---|---|---|
| **1. armazenamento** | **passa**: nenhuma das 8 APIs existe (`ReferenceError`), e depois de fechar o navegador nada sobrou | **passa**: IndexedDB, `localStorage`, `sessionStorage`, cookie, Cache, OPFS e service worker lançam `SecurityError` nos dois navegadores. O `window.name` grava, mas o `iframe` novo começa vazio | **passa**, como a A |
| **2. rede** | **passa**: nenhuma API existe; **0** pedidos HTTP e **0** pacotes UDP nas canárias | **reprova**: a CSP barra todo HTTP (0 pedidos), mas **o WebRTC sai**: **8 pacotes STUN** em cada navegador, e o `webrtc 'block'` da CSP3 é **ignorado pelos dois** | **passa**: 0 e 0 |
| **3. a origem da página** | **passa**: o MOD não alcança o Worker. O `.wasm` só importa 3 funções do módulo `seele` e 9 da WASI (relógio, aleatório, ambiente vazio, escrita de log), e essa é a superfície inteira, mesmo com um defeito de memória no motor | **passa**: `parent.*`, `top.location` e `document.domain` lançam `SecurityError`, e a página recebeu **0** mensagens do `BroadcastChannel` | **passa**, e o Worker também está numa origem opaca |
| **4. a mesma API do desktop** | **passa**: mesmo motor, mesmo prelúdio, e todas as conferências da página batem. **O teto de memória só vale com o `RustAllocator`** (ver abaixo). O laço de 3 s é cortado em 500 ms (`interrupted`). A memória para no terceiro bloco de 2 MiB | **reprova**: o prelúdio roda e as conferências batem, mas o **laço de 3 s não é cortado** e a página trava **3000 ms**. **A memória não tem teto**: 128 MiB alocados. E o prelúdio só funciona com um anfitrião que sonde os relógios (ver abaixo) | **passa**, como a A |
| a página trava com o MOD em laço sem fim? | **não**: no pior caso, 11–13 ms entre duas batidas de 10 ms | **sim**: 3000–3001 ms, nos dois navegadores. O `iframe` roda na mesma thread da página, até no Chromium | **não**: 11–13 ms |

A identidade da página continuou intacta e não extraível em todas as rodadas.
Nenhuma canária HTTP tocou em rodada nenhuma.

### O custo

| | A | B | AB |
|---|---|---|---|
| o motor | **942 514 B** (361 068 B com gzip), compilado uma vez em 1–6 ms | nenhum | o mesmo, compilado em cada `iframe` |
| subir um MOD até o motor responder, Chromium / Simulador | 20–26 ms / 17–55 ms | 6 ms / 21–37 ms | 14–21 ms / 31–59 ms |
| dez MODs, um depois do outro, até o «pronto» de cada um | 720–759 ms / 840–1092 ms | 662–686 ms / 777–862 ms | 752–763 ms / 989–1246 ms |
| a volta página → MOD → página, p50 / p95 | < 0,1 / 0,1–1 ms | < 0,1 / 0,1 ms | < 0,1 / 0,2–1 ms |
| 1 milhão de iterações de aritmética, dentro do MOD | 20–26 ms / 17–66 ms | 3–4 ms / 3–9 ms | 21–25 ms / 18–52 ms |
| 200 árvores de 50 nós em JSON | 10–16 ms / 9–16 ms | 0–2 ms / 0–18 ms | 11–12 ms / 9–15 ms |
| o heap do motor com o MOD comum | 117 728 B (4 páginas de `wasm`: 1,4 MiB) | não mensurável pela página | o mesmo da A |
| memória residente de dez MODs, Chromium / Simulador | +20 a +25 MB / +58 a +87 MB | +5 MB / +0,3 a +1,3 MB | +46 a +53 MB / +78 a +86 MB |

Notas sobre a tabela:
- A primeira medida de CPU de cada rodada no Safari é a mais lenta (49–66 ms).
  As seguintes ficam em 17–25 ms.
- A memória residente é a soma do `ps` sobre os processos do navegador, antes e
  depois. É grosseira, e no Simulador variou muito entre rodadas. No Simulador,
  ela **não voltou** logo depois de encerrar os Workers. Ninguém mediu num
  iPhone.
- **O AB compila o motor em cada `iframe`**, e por isso custa quase o dobro da
  A em memória no Chromium. No produto, um `iframe` só hospedaria todos os
  Workers e compilaria uma vez.
- O modo `iphone` do `prova.py` foi conferido no Simulador pelo IP da LAN, por
  HTTPS e passando pelo aviso de certificado. Os números dele estão nas faixas
  da tabela.

### Três achados que valem para o produto

**1. No `wasm32-wasip1`, o teto de memória do QuickJS não existe.** No `__wasi__`,
o `js__malloc_usable_size` do quickjs-ng devolve 0
(`rquickjs-sys-0.12.2/quickjs/cutils.h:601-612`). O QuickJS só soma os 8 bytes
de cabeçalho de cada bloco, e o `set_memory_limit(8 MiB)` nunca é atingido.
- **Antes:** o MOD alocou 64 blocos de 2 MiB, 128 MiB, com o teto de 8 MiB
  ligado, e o motor dizia ter usado 12 928 B.
- **Depois do conserto** (`Runtime::new_with_alloc(RustAllocator)`, feature
  `allocator`, que guarda o tamanho de cada bloco): o MOD para no terceiro
  bloco com `out of memory`, e o motor diz 117 728 B.

No desktop o teto vale, porque o macOS, o Windows e o Linux respondem o tamanho.
É o «existir não é funcionar» do `CLAUDE.md`. **O executor do navegador precisa
de um guarda** que aloque além do teto no `.wasm` e espere a recusa, provado
trocando o alocador de volta.

**2. O prelúdio do desktop pressupõe que todo código do MOD roda numa volta do
anfitrião.**
- No QuickJS é verdade: o anfitrião avalia, escoa os jobs e recolhe os pedidos
  de relógio.
- Na B, o código do MOD também roda em callbacks nativos do navegador
  (IndexedDB, `fetch`, eventos), fora de qualquer volta. Sem uma sondagem a
  cada 10 ms, um `setTimeout` pedido ali nunca dispara, e o MOD trava.
- Na B, as APIs do navegador **existem e lançam**, o que é outra falha de
  `ReferenceError`. Até ler `globalThis.localStorage` lança.
- É uma segunda semântica, e é o que a spec temia ao chamar a B de «um segundo
  executor».

**3. O build do motor baixa 462 MB sem conferir nada.** O `build.rs` do
`rquickjs-sys` baixa o wasi-sdk 24.0 de `github.com/WebAssembly/wasi-sdk` com
`curl` e o descompacta **sem checksum**. A única alternativa é a variável
`WASI_SDK`, que aponta para um SDK já instalado. No produto, o wasi-sdk tem de
entrar por um passo do `xtask` que confira o SHA-256 e passe `WASI_SDK`. O
`cargo deny` não vê o que um `build.rs` baixa. Para o `wasm32-unknown-unknown`,
o `rquickjs-sys` nem compila: falta o `stdlib.h`, e não há ligações prontas
para esse alvo.

## A recomendação: AB

**O motor do desktop (QuickJS, `rquickjs`, o mesmo prelúdio) num Worker criado
de dentro de um `iframe` opaco.** As razões, em ordem:

1. **A B reprova em dois critérios, e nenhum dos dois tem conserto dentro dela:**
   - **rede**: o WebRTC passa pela CSP nos dois navegadores, e a única diretiva
     que existe para isso é ignorada pelos dois. Apagar o `RTCPeerConnection`
     do `iframe` não fecha a porta: pela leitura, um `iframe` `about:blank`
     criado pelo MOD herda a mesma origem opaca e traz um `RTCPeerConnection`
     novo. Isso não foi medido;
   - **a mesma API do desktop**: sem preempção, um MOD em laço trava a página,
     e com ela o envio de voz, que roda na thread da página. Sem teto, um MOD
     come a memória do celular. O desktop promete os dois tetos (ADR 0045).
2. **A A passa nos cinco critérios** por construção: o código do MOD só alcança
   12 funções importadas, e nenhuma delas é rede, armazenamento ou a página.
3. **A AB acrescenta uma segunda parede, por pouco custo.**
   - Na A, o Worker que hospeda o motor é da origem da página e alcança o
     IndexedDB onde mora a chave. O MOD não alcança o Worker. Mas uma falha no
     isolamento do `wasm` do próprio navegador cairia ali.
   - Na AB, cairia numa origem opaca, sem armazenamento.
   - A diferença de custo medida é de 10 a 30 ms na subida e a compilação por
     `iframe`, que um `iframe` único elimina.
4. **A B é mais barata**: CPU de 5 a 7 vezes mais rápida (JIT) e quase nada de
   memória. Isso não compra os critérios que ela reprova.

**Se o custo da AB no iPhone for alto demais** (a medição do dono), a A é a
saída que ainda passa nos cinco critérios. A B não é saída.

**O que a recomendação pede ao produto** (Tarefa 3.2 do plano da F4):
- o `RustAllocator`, com o guarda do teto;
- o wasi-sdk verificado pelo `xtask`;
- um `iframe` opaco só, com `worker-src blob:` e `'wasm-unsafe-eval'` na CSP
  dele, hospedando um Worker por MOD;
- o prelúdio extraído do `executor.rs` no build (como o `build.rs` daqui), com
  um guarda;
- o MOD hostil deste spike como teste de produto, com as canárias.

## O que não mede

- **O iPhone de verdade.** O `prova.py iphone` existe para isso, e os passos
  estão em `docs/superpowers/sdd/2026-10-08-0.16/F4/medicoes-do-dono.md`. A
  memória residente não tem como ser medida pela página no iPhone.
- **Os três MODs oficiais.** A medição de custo da API 6 no celular (memória e
  tempo de quadro, com os três ligados) espera o porte e é a Tarefa 3.7 do
  plano.
- **O Android.**
- **Um ataque ao motor.** O isolamento por importação é uma propriedade do
  `wasm`. Ninguém tentou um defeito de memória do QuickJS aqui.
- **O `about:blank` dentro da B**: é a leitura acima, não uma medida.
