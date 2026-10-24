# Lote SOM — o teto de som decodificado por MOD (P49-7c), 23–24/10/2026

Ramo `0.16/lotes-som`, a partir de `750e751b` (D-dono-1). O número é da F5, e a
orquestradora aprovou os 128 MiB.

## A medida

**Os pacotes medidos:** os três oficiais 4.0.0 nas árvores de trabalho dos
repositórios locais (`/Users/dev-alexandre/SEELE-MOD-*/.claude/worktrees/*`,
`mod.json` com `"version": "4.0.0"`, `"api": 6`). As pastas principais dos
repositórios estão na 3.x.

**Quem declara som:** só a MESA, com três arquivos (`som/exploracao.wav`,
`som/misterio.wav`, `som/combate.wav`), cada um com 661 544 bytes: WAV PCM de
16 bits, mono, a 11 025 Hz, com 661 500 bytes de dados (30 s). O ESTILO e o
PERFIS não declaram `arquivos`, e o cliente deles não tem `tocando` nem som.

| Como | Por trilha | As três |
|---|---|---|
| O piso do Rust, `seele_sessao::som::bytes_decodificados_minimos` (um teste temporário que lê os `mod.json` e os arquivos, fora do commit) | 5 759 996 | 17 279 988 |
| O WebAudio de verdade, `OfflineAudioContext(1, 1, 48000).decodeAudioData`, no WebKit e no Chromium do Playwright: `length` 1 440 000, 1 canal, 30 s | 5 760 000 | 17 280 000 |
| A conta da janela (`length × canais × 4`), no `carregarSom` de verdade da bancada | 5 760 000 | 17 280 000 |

A MESA segura de pé, no máximo, uma trilha na página e outra no palco (uma
contribuição): 11 520 000 bytes. As três juntas, 17 280 000, são o teto
generoso da medida.

**O custo de memória por som no motor:** 23 cópias de uma trilha decodificadas
e seguras num array, com o RSS da árvore de processos do navegador medido
antes e depois, três vezes. **No Chromium (o motor do WebView2):** 6 016 490,
6 013 640 e 6 037 860 bytes por som, de 5 760 000 contados: **1,045 vez a
conta da janela.** **No WebKit não deu para medir assim:** os processos de
conteúdo dele não ficam sob o processo que o Playwright abre (o delta deu ~0),
e o app de verdade estava proibido neste lote. Pelo código, depois de
decodificar a janela não guarda os bytes comprimidos (o `data:` sai, e o
`ArrayBuffer` do `decodeAudioData` é uma cópia que fica solta); o pico de
cada carga é o arquivo duas ou três vezes, mais o float32.

Os scripts: `scratchpad/som/medida_do_som_dos_oficiais.rs`,
`medir-no-motor.cjs` e `medir-memoria.cjs`; os logs, `medida.log`,
`medida-motor.log` e `medida-memoria.log`, todos na pasta `som/` do
scratchpad da sessão.

## O número: 128 MiB por MOD

- **Folga para os oficiais:** 134 217 728 bytes são **7,7 vezes** as três
  trilhas da MESA (17 280 000), e 11,6 vezes o que ela segura de pé de
  verdade.
- **O dobro de um bolso:** o maior som que um lugar aceita (64 MiB) cabe
  sempre, com outro do mesmo tamanho ao lado.
- **O que corta:** sem o teto, em tese (12 superfícies + 128 contribuições) ×
  64 MiB, uns 8,75 GiB por MOD. **No teto, 128 MiB de float32**, uns 140 MB
  no Chromium pela medida de 1,045 vez; perto de doze minutos de som mono a
  48 kHz, ou seis em estéreo.

## O conserto

- **Um número, num lugar só:** `LIMITES_DO_MOD.bytesDeSomDecodificado`, em
  `apps/seele-app/ui/mods-regiao.js`. Só a janela o conta: o Rust corta o som
  que sozinho passa de um bolso (`TETO_DO_SOM_DECODIFICADO`) e não sabe o que
  está de pé em cada lugar, então não há cópia nem ponte para ler.
- **A conta é por instância** (`contaDoSomDoMod`, numa `WeakMap` com a
  `instancia` que `donoDaRegiao` dá a todos os lugares do MOD). O MOD
  recarregado é outra instância. Um dono sem `instancia` (o de um
  laboratório) conta por ele mesmo.
- O `carregarSom` confere o teto do MOD depois do teto do bolso. A conta
  diminui quando o som sai do nó (`guardar`) e quando o destino de uma
  contribuição devolve os sons (`devolverOsSons`).
- **A recusa com nome:** pelo `recusarPeloTetoDoSom`, como todo teto do som —
  figura «cheia», `recusada` ao MOD com os números no `porque` («… passa do
  teto de 134217728 bytes de som do MOD»), a linha de `anotarRecusa` (no
  produto, `registrarNoAnfitriao` com o `mod_id`) e a anotação com a chave
  `bytes de som do MOD:134217728`, que a aba DIAGNÓSTICO junta numa vaga só.
- **A frase pública:** o `api/README.md`, as notas (`empacotar/notas/0.16.0.md`)
  e a D-dono-1 da spec. O `api/v6.json` não muda: ele não diz o teto por
  bolso, e está congelado.

## A prova

- **O vermelho** (`86033b3c`, a bancada contra o `mods-regiao.js` de antes):
  quatro sons de 60 397 976 bytes em três regiões e um cartão do mesmo MOD
  ficam `["pronta","pronta","pronta","pronta"]`, quatro tocadores, e a
  bancada não acha teto do MOD (`undefined`).
- **O conserto** (`df4394db`): 60 provas verdes. 2 de pé, 2 recusados com o
  teto do MOD no `porque`, no registro e na chave; outro MOD não sente a
  conta deste; o lugar solto devolve o espaço; tudo solto, o teto volta
  inteiro; as três trilhas da MESA ficam de pé.
- **As reversões**, cada uma com o fonte copiado de volta por `cp` sem `-p` e
  conferido por `cmp` contra a cópia do conserto:
  1. o `mods-regiao.js` inteiro de antes (`cmp` igual ao original): 4 falhas;
  2. só o `if` do teto do MOD desligado: 2 falhas (os quatro de pé);
  3. sem o desconto no `guardar`: 2 falhas (o espaço não volta);
  4. sem o desconto no `devolverOsSons`: 1 falha (`37709b25`: 768 000 no MOD
     para 384 000 de pé). Antes dessa prova, essa reversão passava verde.

## O que baixou

Uma linha no registro do curl (`scratchpad/som/curl-registro.tsv`), no build
frio do `shiguredo_opus` sob a HOME falsa:

```
2026-10-23T22:38:09  -fsSL -o …/target/debug/build/shiguredo_opus-ded9d4b1bfadc216/out/build/opus-1.6.1.tar.gz https://oss-mirrors.shiguredo.jp/opus-1.6.1.tar.gz
```

O CMake veio do `~/.cache/shiguredo_cmake` clonado. Nada mais.

## Ressalvas

- **O custo no WebKit (WKWebView) não foi medido**, só a conta do float32 e o
  Chromium. Medir pede o app de verdade.
- **O teto é por MOD, não por janela:** com vários MODs carregados, os 128 MiB
  se somam por MOD.
- O laboratório com um dono sem `instancia` conta por dono, e não por MOD.
