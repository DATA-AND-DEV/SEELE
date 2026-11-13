# A tela do produto sob perda: o controle de taxa de verdade, medido

**Data:** 2026-11-10
**Árvore:** `entrega/1.0`, commit `ef867b0e`, mais o exemplo novo, não commitado (o produto não foi tocado por esta medida)
**O que é:** uma medida, não uma decisão. `docs/medidas/2026-11-10-tela-sob-perda.md` mediu o canal SCTP com um emissor de taxa fixa, e disse no fim que aquele emissor não tinha o balde, a chave em fatias, o teto nem a sonda do produto. Esta medida põe o produto inteiro no lugar do emissor e pergunta o que quem assiste vê.

## Como reproduzir

```sh
# o aparelho de mentira da voz não compila sem `debug_assertions`
# (`seele-audio/src/device/mentira.rs`); a opção liga só isso, na linha de
# comando, e o resto do perfil é o `release`. Nada no repositório muda.
B="cargo run --release --config profile.release.debug-assertions=true -p seele-conformance --example tela_do_produto_sob_perda --"

# a grade: 7 cenários × (produto, fixo) × 3 rodadas de 20 s, ~20 min
$B

# o controle do banco: o emissor da linha «A» da medida anterior, ~9 min
$B --variante cru

# o regime: 40 s de transmissão antes de abrir a janela de 20 s, ~25 min
$B --variante produto --aquecimento 40

# as variações (cada uma com --so "0%" e com --so "1%")
$B --variante produto --escolha-bps 1470000 --so "0%"
$B --variante produto --recusa-sem-chave --so "0%"
$B --variante produto --fracao 1.0 --rodadas 1 --so "0%"

# as duas conferências do cenário sem perda (1 rodada de 10 s cada)
$B --variante produto --rodadas 1 --segundos 10 --so "0%" --atraso-ms 0
$B --variante produto --rodadas 1 --segundos 10 --so "0%" --escolha-bps 4000000

# o registro do produto no STDERR, para investigar uma célula
SEELE_LOG=1 RUST_LOG=warn,seele_core=info,seele_ffi=debug,seele_server=info $B --so "1%" --aquecimento 40
```

Semente `20261110` (rodada *n* usa semente + *n* − 1). O código é `crates/seele-conformance/examples/tela_do_produto_sob_perda.rs`; o emulador é o `Cano` de `crates/seele-enlace/src/teste.rs`, o mesmo da medida anterior.

## A máquina

Apple M5 Pro, 15 núcleos, 24 GB, macOS 27.2 (26B123), rustc 1.97.1, perfil `release` com `debug-assertions` ligado (ver acima). Tudo em 127.0.0.1, num processo só. Uma célula por vez. Carga média (`uptime`, 1 min) no começo e no fim: grade principal 2,17 e 1,74; controle cru 2,67 e 2,09; regime 2,09 e 1,72; variações 1,72 e 1,96, 1,96 e 5,21, 5,21 e 1,55, 1,55 e 1,40, 1,40 e 1,80, 1,80 e 1,94. A partir das variações havia outras sessões abertas na máquina, e a carga chegou a 5,21 no fim da execução da escolha de 1,47 Mbps com 1% de perda.

## O que foi medido

**O banco.** Um `seele_server::Daemon` e duas `seele_ffi::Connection`: quem transmite e quem assiste, sentados na mesma sala. Quem transmite disca o `Cano`, que repassa ao servidor; quem assiste disca o servidor direto, sem cano. O servidor declara 50 Mbps de subida (`caminho_bps`), para a perna de quem hospeda não apertar: o teto que vale é o de quem transmite. Em cada célula o exemplo confere que o vídeo passou pelo cano (os bytes que o cano repassou contra os que o codificador entregou). Uma tentativa do emissor fixo a 1% foi recusada por essa conferência e refeita: o balde tinha descartado antes do fio quase tudo o que o codificador entregou (20,4 MB entregues, 3,2 MB no cano). Foi a única tentativa refeita.

**A rede** (o `Cano`), como na medida anterior: em cada sentido, perda independente por datagrama com semente fixa, 30 ms de atraso fixo, sem jitter, e — nos dois cenários de banda — um gargalo com fila de 250 ms e descarte pela cauda. O aperto passa pelo cano também. Cenários: 0%, 0,3%, 1%, 3% e 5% de perda sem limite de banda; 1,0 Mbps sem perda; 2,0 Mbps com 1%.

**O vídeo** entra pelo caminho da câmera: `Connection::compartilhar_camera` e `Connection::quadro_da_camera`, com os limites que o aplicativo usa (`LIMITES_DA_CAMERA` em `apps/seele-app/ui/camera.js`: 720p, 30 quadros, movimento, **sem teto de banda escolhido**). Os quadros são enchimento, com o número e o instante da captura nos primeiros bytes. Três emissores:

- **produto** — um «codificador de mentira» que faz o que a casca do aplicativo faz (lido em `camera.js`): mira 85% do `Event::CameraBudget` (`FRACAO_DO_TETO_DA_CAMERA`), a 30 quadros/s, ou seja delta = orçamento × 0,85 ÷ 30 ÷ 8 bytes; a chave tem **8 × o delta**; manda chave no começo, a cada `Event::CameraKeyFrameRequested`, depois de um quadro recusado (`CameraQueueFull`) e a cada troca de orçamento (a casca reconfigura o codificador, e o primeiro quadro depois disso é chave); pula 3 quadros depois de cada chave (`QUADROS_APOS_A_CHAVE`); para em `Event::CameraPaused`. O pedido desta medida dizia delta = orçamento ÷ 30 ÷ 8, sem os 85%: a casca usa 85%, então a grade usa 85%, e a variação «fração 1,0» mede a diferença.
- **fixo** — ignora o orçamento: deltas de 6 KiB a 30 quadros/s (≈1,47 Mbps) e chave de 80 KiB, a carga da medida anterior, **pela mesma porta da câmera**. O balde do produto continua valendo.
- **cru** — o emissor da linha «A» da medida anterior, sem núcleo de cliente: um par que fala o fio à mão, escreve um quadro por vez no fluxo confiável e só captura o seguinte quando o anterior acabou de ser escrito. Mesma carga do fixo, mesmo servidor, mesma pessoa assistindo. Sem voz. Existe para dizer se **o banco** reproduz a medida anterior.

**A voz.** Quem transmite tem a voz aberta, com o aparelho de mentira do `seele-audio` (um tom contínuo) no lugar do microfone; daí em diante é o Opus e o canal de voz do produto. Quem assiste a toca no aparelho de mentira dele e fica calado.

**As métricas.** Latência = da entrega do quadro a `quadro_da_camera` até o `Event::ScreenFrame` dele em quem assiste (ms). «Congelado %» = soma dos intervalos de mais de 200 ms sem quadro novo, sobre os 20 s. «Aceitos» = quadros que o núcleo aceitou; «recusados (fila)» = `CameraQueueFull`; «não chegaram» = aceitos que não tinham chegado 3 s depois do fim da janela (descartados pelo teto, ou perdidos com o corte de quem assiste, abaixo). «Orçamento» = o último `CameraBudget`, amostrado a cada 250 ms. «Sinal servidor» = o `signal` do `PersonState` de quem transmite, lido no retrato dele: é o número de que o motor tira a faixa que corta o teto. «Sinal local» = `Snapshot::telemetry.signal`, o que a janela mostra. «rtt» = a ida e volta da telemetria de quem transmite (o `Ping` do `ctl`). «Voz tocada %» = blocos de saída com som em quem assiste, sobre os blocos da janela: diz se a voz tocou, não com quanto atraso.

### O que é o mesmo caminho da tela do sistema, e onde difere (leitura de código)

O mesmo: `Comando::CompartilharTela` semeia a sonda igual para as duas fontes; `Motor::teto_de_video`, `self.faixa`, `medir_o_caminho` e `reconferir_o_teto` (`seele-core/src/enlace.rs`) não distinguem a fonte; e quem escreve no fio é o mesmo laço (`escoar_espelhado` em `bomba.rs`), com a mesma `Transmissao`: o balde, a chave em quatro fatias, o descarte acima do teto e o pedido de chave depois de um descarte.

Difere antes da `Transmissao`. A tela do sistema é codificada pela bomba, que **espera** em vez de codificar quando o canal está cheio ou quando o quadro anterior ainda não foi escrito (`bomba.rs`, «Esperar aqui mantém as referências H.264 intactas»): um engasgo custa quadros não capturados, e a cadeia de deltas continua. A câmera é codificada pela janela e empurrada por `quadro_da_camera`: há uma fila de 8 quadros (`FILA_DE_FORA`), e com ela cheia o quadro é **recusado depois de codificado** — o seguinte tem de ser chave. Um engasgo na câmera custa uma chave; na tela do sistema, não. A variação «recusa sem chave» aproxima o segundo comportamento, e **a tela do sistema em si não foi medida**.

## O resumo

Mediana de 3 rodadas de 20 s. «Primeiros 20 s» é a janela aberta no instante em que a câmera liga, numa sessão que nunca transmitiu; «depois de 40 s» é a mesma transmissão, medida do segundo 40 ao 60.

| cenário | produto, primeiros 20 s: p50 / congelado % / quadros/s | orçamento kbps (mediana → final) | produto, depois de 40 s: p50 / congelado % / quadros/s | orçamento kbps (mediana) | fixo (pelo produto): p50 / congelado % / quadros/s | cru: p50 / congelado % / quadros/s | A (medida anterior) |
|---|--:|--:|--:|--:|--:|--:|--:|
| 0% | 415 / 55,3 / 14,8 | 12480 → 12480 | 3126 / 97,9 / 4,0 | 12480 | 32 / 1,5 / 30,1 | 32 / 1,5 / 30,0 | 32 / 1,6 / 30,0 |
| 0,3% | 33 / 22,6 / 20,8 | 1500 → 832 | 32 / 0,0 / 30,1 | 514 | 60 / 1,5 / 30,1 | 33 / 1,5 / 30,0 | 32 / 1,6 / 30,0 |
| 1% | 33 / 42,6 / 16,1 | 746 → 495 | 32 / 5,5 / 29,8 | 202 | 501 / 55,1 / 13,6 | 265 / 1,8 / 29,2 | 437 / 3,1 / 28,0 |
| 3% | 4466 / 89,1 / 1,5 | 1463 → 379 | 32 / 13,2 / 30,1 | 200 | 2826 / 87,3 / 2,7 | 1314 / 15,7 / 16,6 | 1528 / 17,6 / 14,2 |
| 5% | 6282 / 96,5 / 1,4 | 1463 → 344 | 33 / 29,0 / 29,9 | 200 | 3988 / 87,4 / 2,6 | 2129 / 49,3 / 8,3 | 2518 / 43,5 / 8,4 |
| 1,0 Mbps, 0% | 283 / 63,3 / 8,9 | 1170 → 512 | 49 / 3,4 / 29,2 | 560 | 3826 / 89,6 / 2,8 | 1156 / 21,3 / 17,2 | 1229 / 24,5 / 15,5 |
| 2,0 Mbps, 1% | 42 / 50,9 / 15,0 | 725 → 471 | 36 / 0,0 / 29,8 | 261 | 882 / 54,3 / 12,2 | 612 / 3,4 / 27,4 | 719 / 8,1 / 24,5 |

## A grade: os primeiros 20 s de uma transmissão (mediana de 3 rodadas)

A linha «A (medida anterior)» é copiada de `2026-11-10-tela-sob-perda.md` (a de 0,3% é de uma rodada só, lá).

| cenário | emissor | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | aceitos | recusados (fila) | não chegaram | chaves | pedidos de chave | kbps oferecidos | MB no cano | voz tocada % | cano perda % | cano fila % |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 415 | 2751 | 3007 | 3101 | 55,3 | 383 | 14,8 | 307 | 161 | 0 | 44 | 0 | 9959 | 23,36 | 100,0 | 0,00 | 0,00 |
| 0% | fixo | 32 | 33 | 153 | 293 | 1,5 | 294 | 30,1 | 601 | 0 | 0 | 1 | 0 | 1507 | 4,27 | 100,2 | 0,00 | 0,00 |
| 0% | cru | 32 | 33 | 99 | 296 | 1,5 | 296 | 30,0 | 601 | — | 0 | 1 | — | 1509 | 4,02 | — | 0,00 | 0,00 |
| 0% | A (medida anterior) | 32 | 33 | 167 | 313 | 1,6 | 314 | 30,0 | — | — | — | — | — | — | — | — | — | — |
| 0,3% | produto | 33 | 2214 | 3591 | 3918 | 22,6 | 718 | 20,8 | 432 | 101 | 17 | 25 | 8 | 1808 | 4,47 | 100,2 | 0,32 | 0,00 |
| 0,3% | fixo | 60 | 202 | 261 | 294 | 1,5 | 296 | 30,1 | 601 | 0 | 0 | 1 | 0 | 1507 | 4,24 | 100,2 | 0,36 | 0,00 |
| 0,3% | cru | 33 | 125 | 166 | 297 | 1,5 | 298 | 30,0 | 601 | — | 0 | 1 | — | 1509 | 4,02 | — | 0,37 | 0,00 |
| 0,3% | A (medida anterior) | 32 | 115 | — | — | 1,6 | 315 | 30,0 | — | — | — | — | — | — | — | — | — | — |
| 1% | produto | 33 | 3547 | 4963 | 5411 | 42,6 | 1071 | 16,1 | 343 | 189 | 17 | 31 | 10 | 1219 | 3,21 | 99,5 | 0,95 | 0,00 |
| 1% | fixo | 501 | 1313 | 1355 | 1362 | 55,1 | 5730 | 13,6 | 589 | 12 | 270 | 263 | 258 | 9419 | 2,46 | 99,6 | 0,90 | 0,00 |
| 1% | cru | 265 | 682 | 907 | 937 | 1,8 | 364 | 29,2 | 595 | — | 0 | 1 | — | 1494 | 3,92 | — | 0,92 | 0,00 |
| 1% | A (medida anterior) | 437 | 848 | 1013 | 1046 | 3,1 | 616 | 28,0 | — | — | — | — | — | — | — | — | — | — |
| 3% | produto | 4466 | 15582 | 16267 | 16267 | 89,1 | 2347 | 1,5 | 53 | 479 | 13 | 23 | 6 | 937 | 2,07 | 96,1 | 3,15 | 0,00 |
| 3% | fixo | 2826 | 10855 | 11910 | 11910 | 87,3 | 4807 | 2,7 | 239 | 362 | 187 | 199 | 182 | 6619 | 1,69 | 96,1 | 3,39 | 0,00 |
| 3% | cru | 1314 | 2256 | 2387 | 2445 | 15,7 | 1193 | 16,6 | 352 | — | 0 | 1 | — | 896 | 2,26 | — | 3,12 | 0,00 |
| 3% | A (medida anterior) | 1528 | 2505 | 2657 | 2761 | 17,6 | 1126 | 14,2 | — | — | — | — | — | — | — | — | — | — |
| 5% | produto | 6282 | 11743 | 15464 | 15464 | 96,5 | 3567 | 1,4 | 52 | 489 | 17 | 20 | 5 | 760 | 1,40 | 84,0 | 5,76 | 0,00 |
| 5% | fixo | 3988 | 15034 | 16126 | 16126 | 87,4 | 2110 | 2,6 | 72 | 529 | 12 | 26 | 0 | 965 | 1,32 | 83,2 | 5,44 | 0,00 |
| 5% | cru | 2129 | 5215 | 5415 | 5481 | 49,3 | 3071 | 8,3 | 188 | — | 0 | 1 | — | 493 | 1,19 | — | 5,62 | 0,00 |
| 5% | A (medida anterior) | 2518 | 3629 | 3783 | 3818 | 43,5 | 2068 | 8,4 | — | — | — | — | — | — | — | — | — | — |
| 1,0 Mbps, 0% | produto | 283 | 7556 | 10076 | 10200 | 63,3 | 3553 | 8,9 | 199 | 327 | 20 | 28 | 10 | 871 | 2,11 | 87,7 | 0,00 | 1,77 |
| 1,0 Mbps, 0% | fixo | 3826 | 7429 | 8498 | 8498 | 89,6 | 7454 | 2,8 | 321 | 280 | 265 | 276 | 260 | 9155 | 1,42 | 92,7 | 0,00 | 2,11 |
| 1,0 Mbps, 0% | cru | 1156 | 2450 | 3035 | 3137 | 21,3 | 2111 | 17,2 | 366 | — | 0 | 1 | — | 931 | 2,35 | — | 0,00 | 1,81 |
| 1,0 Mbps, 0% | A (medida anterior) | 1229 | 2764 | 3085 | 3176 | 24,5 | 2166 | 15,5 | — | — | — | — | — | — | — | — | — | — |
| 2,0 Mbps, 1% | produto | 42 | 4314 | 6837 | 7939 | 50,9 | 1354 | 15,0 | 317 | 220 | 14 | 28 | 8 | 1137 | 2,84 | 99,2 | 0,89 | 0,00 |
| 2,0 Mbps, 1% | fixo | 882 | 1939 | 2554 | 2848 | 54,3 | 6575 | 12,2 | 542 | 59 | 299 | 289 | 284 | 10168 | 2,19 | 99,7 | 0,88 | 0,00 |
| 2,0 Mbps, 1% | cru | 612 | 1064 | 1262 | 1301 | 3,4 | 502 | 27,4 | 554 | — | 0 | 1 | — | 1393 | 3,67 | — | 0,87 | 0,00 |
| 2,0 Mbps, 1% | A (medida anterior) | 719 | 1438 | 1715 | 1724 | 8,1 | 1091 | 24,5 | — | — | — | — | — | — | — | — | — | — |

### O que o controle de taxa fez

| cenário | codificador | orç. mín kbps | orç. mediana kbps | orç. final kbps | avisos de orçamento | pausas | parado % | sinal servidor mín | sinal servidor med. | sinal local mín | sinal local med. | fora de Nominal % | rtt p50 ms | rtt máx ms | perda medida máx % | reaberturas em quem assiste |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 5580 | 12480 | 12480 | 4 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 65 | — | 0 |
| 0% | fixo | 5580 | 5580 | 5580 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 65 | — | 0 |
| 0,3% | produto | 832 | 1500 | 832 | 6 | 0 | 0,0 | 100 | 100 | 62 | 91 | 0,0 | 65 | 391 | — | 0 |
| 0,3% | fixo | 996 | 3571 | 996 | 6 | 0 | 0,0 | 100 | 100 | 91 | 97 | 0,0 | 64 | 126 | — | 0 |
| 1% | produto | 495 | 746 | 495 | 9 | 0 | 0,0 | 100 | 100 | 60 | 66 | 0,0 | 341 | 568 | — | 0 |
| 1% | fixo | 378 | 840 | 378 | 10 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 420 | 675 | — | 0 |
| 3% | produto | 379 | 1463 | 379 | 12 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 591 | 1690 | — | 0 |
| 3% | fixo | 250 | 936 | 250 | 12 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 712 | 1564 | — | 0 |
| 5% | produto | 344 | 1463 | 344 | 11 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 960 | 2142 | — | 0 |
| 5% | fixo | 214 | 1463 | 214 | 13 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 865 | 1332 | — | 0 |
| 1,0 Mbps, 0% | produto | 512 | 1170 | 512 | 10 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 608 | 2272 | — | 0 |
| 1,0 Mbps, 0% | fixo | 505 | 611 | 505 | 9 | 0 | 0,0 | 100 | 100 | 60 | 62 | 0,0 | 652 | 2253 | — | 0 |
| 2,0 Mbps, 1% | produto | 471 | 725 | 471 | 9 | 0 | 0,0 | 100 | 100 | 60 | 68 | 0,0 | 319 | 704 | — | 0 |
| 2,0 Mbps, 1% | fixo | 518 | 768 | 518 | 9 | 0 | 0,0 | 100 | 100 | 60 | 78 | 0,0 | 66 | 670 | — | 0 |

### Menor e maior valor entre as 3 rodadas

| cenário | codificador | lat p50 | p95 | congelado % | quadros/s | orç. mediana kbps | recusados (fila) | não chegaram | pedidos de chave |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 404–428 | 2506–2883 | 55,2–56,8 | 14,7–14,8 | 12480–12480 | 161–165 | 0–1 | 0–0 |
| 0% | fixo | 32–32 | 33–33 | 1,5–1,5 | 30,1–30,1 | 5580–5580 | 0–0 | 0–0 | 0–0 |
| 0,3% | produto | 32–124 | 1657–3508 | 15,8–52,8 | 14,4–22,8 | 902–3284 | 75–186 | 14–17 | 6–10 |
| 0,3% | fixo | 33–97 | 198–544 | 1,5–38,3 | 20,4–30,1 | 1190–5580 | 0–0 | 0–193 | 0–118 |
| 1% | produto | 33–59 | 2764–4498 | 27,0–47,4 | 14,2–18,5 | 665–1013 | 142–218 | 14–21 | 6–11 |
| 1% | fixo | 291–1474 | 662–5148 | 21,4–81,2 | 5,0–23,4 | 735–1463 | 0–231 | 132–318 | 88–265 |
| 3% | produto | 322–5047 | 8399–16291 | 72,9–93,0 | 1,4–7,9 | 936–2286 | 356–497 | 9–15 | 0–8 |
| 3% | fixo | 1949–3265 | 8497–11474 | 86,3–89,3 | 2,6–3,7 | 679–1463 | 285–459 | 88–242 | 58–237 |
| 5% | produto | 1078–6832 | 6179–14913 | 75,9–96,6 | 1,1–8,9 | 1463–1463 | 346–514 | 11–21 | 0–7 |
| 5% | fixo | 2826–4862 | 12508–15769 | 85,4–88,4 | 2,5–3,0 | 1170–2286 | 459–536 | 10–91 | 0–86 |
| 1,0 Mbps, 0% | produto | 48–500 | 6342–8739 | 57,0–64,1 | 8,2–11,2 | 1170–1463 | 291–344 | 19–20 | 10–10 |
| 1,0 Mbps, 0% | fixo | 2892–3856 | 6859–8712 | 89,5–95,6 | 2,8–2,8 | 609–622 | 274–293 | 252–272 | 247–266 |
| 2,0 Mbps, 1% | produto | 41–44 | 3952–4842 | 46,4–51,7 | 13,6–15,2 | 685–1172 | 208–242 | 13–17 | 7–8 |
| 2,0 Mbps, 1% | fixo | 780–1967 | 1595–5874 | 53,7–79,9 | 4,2–13,4 | 595–839 | 28–293 | 224–305 | 219–293 |

O controle cru:

| cenário | codificador | lat p50 | p95 | congelado % | quadros/s | orç. mediana kbps | recusados (fila) | não chegaram | pedidos de chave |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | cru | 32–32 | 33–33 | 1,5–1,5 | 29,9–30,0 | — | — | 0–0 | — |
| 0,3% | cru | 32–33 | 95–208 | 1,5–1,8 | 29,9–30,0 | — | — | 0–0 | — |
| 1% | cru | 122–431 | 625–1275 | 1,5–7,5 | 27,2–29,7 | — | — | 0–0 | — |
| 3% | cru | 1190–1366 | 2081–2377 | 13,7–19,4 | 14,6–17,3 | — | — | 0–0 | — |
| 5% | cru | 1937–2305 | 2950–5672 | 29,8–52,7 | 8,2–10,9 | — | — | 0–1 | — |
| 1,0 Mbps, 0% | cru | 1155–1157 | 2450–2613 | 19,5–21,9 | 17,2–17,2 | — | — | 0–0 | — |
| 2,0 Mbps, 1% | cru | 424–655 | 779–1463 | 2,5–8,1 | 25,1–28,6 | — | — | 0–0 | — |

### O orçamento segundo a segundo (kbps; 0 seria parado)

| cenário | codificador | rodada | orçamento ao fim de cada segundo | paradas |
|---|---|--:|---|---|
| 0% | produto | 1 | 6843 6843 8457 10572 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 | — |
| 0% | produto | 2 | 6920 6920 8650 10699 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 | — |
| 0% | produto | 3 | 6876 6876 8595 10636 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 | — |
| 0% | fixo | 1 | 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 | — |
| 0% | fixo | 2 | 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 | — |
| 0% | fixo | 3 | 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 | — |
| 0,3% | produto | 1 | 6413 6413 8017 6413 5131 5131 3283 3283 3283 3283 3283 1479 1479 1479 977 977 977 977 622 622 | — |
| 0,3% | produto | 2 | 5580 4464 4464 2856 1553 1553 1409 1409 1409 901 901 901 901 901 901 901 901 901 901 901 | — |
| 0,3% | produto | 3 | 5580 5580 4464 3571 2856 1500 1500 1500 1500 1500 1500 832 832 832 832 832 832 832 832 832 | — |
| 0,3% | fixo | 1 | 5580 5580 5580 5580 5580 5580 5580 5580 5580 5580 4464 4464 3571 2856 2856 2856 2856 2856 2285 2285 | — |
| 0,3% | fixo | 2 | 4464 4464 3571 2856 2856 2856 2285 2285 1190 1190 1190 1190 1190 1190 1190 1190 1190 1007 627 627 | — |
| 0,3% | fixo | 3 | 5580 5580 5580 4464 4464 4464 3571 3571 3571 3571 3571 2856 2856 2285 1828 1828 1828 1828 995 995 | — |
| 1% | produto | 1 | 5580 4464 3571 3571 2285 2285 2285 1086 1012 1012 1012 1012 810 810 810 494 494 494 494 494 | — |
| 1% | produto | 2 | 5580 4464 3571 3571 2856 2856 1828 1038 1038 746 729 729 729 729 729 729 460 460 460 460 | — |
| 1% | produto | 3 | 4464 4464 3571 2856 2856 2285 2285 2285 1828 664 664 664 581 581 581 581 581 581 581 581 | — |
| 1% | fixo | 1 | 5580 4464 3571 3571 2856 2285 1828 1462 1462 1462 1462 1462 1170 1119 1111 719 719 472 377 377 | — |
| 1% | fixo | 2 | 4464 3571 3571 2856 2285 2285 1828 1462 1462 735 735 735 735 735 464 464 464 464 464 464 | — |
| 1% | fixo | 3 | 4464 3571 2856 2856 2285 1828 1313 1050 840 840 840 467 467 467 467 373 373 373 373 373 | — |
| 3% | produto | 1 | 5580 4464 3571 3571 3571 3571 3571 3571 2285 2285 2285 1828 1828 1462 1462 1462 1170 729 729 240 | — |
| 3% | produto | 2 | 5580 4464 3571 3571 3571 2285 1828 1462 1462 1462 1462 1462 1462 1462 1462 1462 936 936 537 537 | — |
| 3% | produto | 3 | 5580 4464 4464 2856 2856 2856 2285 1462 1462 936 936 936 866 554 474 474 474 474 379 379 | — |
| 3% | fixo | 1 | 5580 4464 4464 3571 2285 1828 1828 1828 1462 1462 1462 1462 936 522 522 522 360 248 248 248 | — |
| 3% | fixo | 2 | 5580 4464 3571 3571 3571 2285 2285 1462 1462 936 540 540 504 504 312 312 312 312 249 249 | — |
| 3% | fixo | 3 | 4464 4464 3571 3571 3571 2856 2285 1462 1462 679 679 527 527 527 527 421 421 421 421 337 | — |
| 5% | produto | 1 | 5580 5580 3571 3571 2285 2285 1828 1828 1828 1462 1462 936 936 599 334 334 334 334 334 200 | — |
| 5% | produto | 2 | 5580 4464 4464 4464 4464 2856 1828 1462 1462 1462 1462 1462 1462 1462 1462 1462 1462 1170 1170 1170 | — |
| 5% | produto | 3 | 5580 4464 3571 2856 2856 2856 2285 2285 1828 1462 1170 748 748 344 344 344 344 344 344 344 | — |
| 5% | fixo | 1 | 5580 4464 3571 2856 2856 2856 2856 2856 2856 2856 1462 748 748 748 748 748 362 200 200 200 | — |
| 5% | fixo | 2 | 5580 4464 4464 3571 2856 2285 2285 1828 1828 1462 1462 1170 936 748 748 599 248 248 248 213 | — |
| 5% | fixo | 3 | 5580 4464 4464 4464 4464 2856 1462 1170 1170 1170 1170 1170 1170 1170 1170 936 936 936 513 482 | — |
| 1,0 Mbps, 0% | produto | 1 | 5580 4464 4464 3571 2285 1828 1828 1828 1828 1462 1462 1462 512 512 512 512 512 512 512 512 | — |
| 1,0 Mbps, 0% | produto | 2 | 5580 5580 4464 4464 3571 2285 1828 1462 1462 1170 1170 530 530 530 523 523 523 523 523 523 | — |
| 1,0 Mbps, 0% | produto | 3 | 5580 4464 4464 3571 2285 2285 2285 1828 1828 1462 1170 495 495 495 495 482 482 482 482 482 | — |
| 1,0 Mbps, 0% | fixo | 1 | 4464 4464 3571 2856 2856 2856 2285 2285 1170 611 504 504 504 504 504 504 504 504 504 504 | — |
| 1,0 Mbps, 0% | fixo | 2 | 4464 4464 3571 2856 2285 2285 1828 1462 1170 609 609 511 511 511 511 511 511 511 511 511 | — |
| 1,0 Mbps, 0% | fixo | 3 | 4464 4464 3571 3571 2856 2285 1828 1462 1170 621 621 495 495 495 495 495 495 495 495 495 | — |
| 2,0 Mbps, 1% | produto | 1 | 5580 4464 4464 2856 2856 1828 1828 1156 925 684 684 684 684 684 419 419 419 419 419 419 | — |
| 2,0 Mbps, 1% | produto | 2 | 5580 4464 4464 2856 2856 1828 1828 827 827 827 724 724 724 724 724 470 470 470 470 470 | — |
| 2,0 Mbps, 1% | produto | 3 | 5580 4464 4464 3571 2285 1828 1462 1462 1462 1462 574 574 574 493 493 493 493 493 493 493 | — |
| 2,0 Mbps, 1% | fixo | 1 | 4464 4464 3571 2856 2285 1828 1172 1172 938 768 762 762 762 463 463 463 463 463 370 370 | — |
| 2,0 Mbps, 1% | fixo | 2 | 4464 4464 3571 3571 3571 2285 1828 1462 1462 1462 838 709 517 517 517 517 517 517 517 517 | — |
| 2,0 Mbps, 1% | fixo | 3 | 4464 3571 3571 2856 2285 1828 969 937 937 599 595 595 595 595 595 595 595 595 595 595 | — |

## O regime: a mesma transmissão, do segundo 40 ao 60

| cenário | codificador | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | aceitos | recusados (fila) | não chegaram | chaves | pedidos de chave | kbps oferecidos | MB no cano | voz tocada % | cano perda % | cano fila % |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 3126 | 3458 | 3482 | 3482 | 97,9 | 387 | 4,0 | 80 | 340 | 1 | 61 | 0 | 8775 | 23,62 | 100,6 | 0,00 | 0,00 |
| 0,3% | produto | 32 | 57 | 98 | 130 | 0,0 | 132 | 30,1 | 601 | 0 | 0 | 0 | 0 | 438 | 1,43 | 100,7 | 0,29 | 0,00 |
| 1% | produto | 32 | 111 | 152 | 259 | 5,5 | 1108 | 29,8 | 601 | 0 | 0 | 0 | 0 | 172 | 0,72 | 100,5 | 1,11 | 0,00 |
| 3% | produto | 32 | 642 | 1042 | 1179 | 13,2 | 1077 | 30,1 | 601 | 0 | 0 | 0 | 0 | 170 | 0,65 | 100,5 | 3,06 | 0,00 |
| 5% | produto | 33 | 888 | 1087 | 1176 | 29,0 | 1100 | 29,9 | 601 | 0 | 0 | 0 | 0 | 170 | 0,66 | 100,5 | 5,41 | 0,00 |
| 1,0 Mbps, 0% | produto | 49 | 182 | 204 | 326 | 3,4 | 256 | 29,2 | 583 | 0 | 0 | 6 | 0 | 532 | 1,69 | 100,2 | 0,00 | 0,00 |
| 2,0 Mbps, 1% | produto | 36 | 93 | 134 | 141 | 0,0 | 139 | 29,8 | 595 | 0 | 0 | 2 | 0 | 250 | 0,93 | 100,5 | 1,16 | 0,00 |

| cenário | codificador | orç. mín kbps | orç. mediana kbps | orç. final kbps | avisos de orçamento | pausas | parado % | sinal servidor mín | sinal servidor med. | sinal local mín | sinal local med. | fora de Nominal % | rtt p50 ms | rtt máx ms | perda medida máx % | reaberturas em quem assiste |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 12480 | 12480 | 12480 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 66 | — | 0 |
| 0,3% | produto | 514 | 514 | 857 | 1 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 64 | — | 0 |
| 1% | produto | 202 | 202 | 202 | 0 | 0 | 0,0 | 100 | 100 | 76 | 97 | 0,0 | 62 | 262 | — | 0 |
| 3% | produto | 200 | 200 | 200 | 0 | 0 | 0,0 | 100 | 100 | 82 | 97 | 0,0 | 63 | 205 | — | 0 |
| 5% | produto | 200 | 200 | 200 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 62 | 64 | — | 0 |
| 1,0 Mbps, 0% | produto | 500 | 560 | 610 | 6 | 0 | 0,0 | 100 | 100 | 87 | 96 | 0,0 | 74 | 170 | — | 0 |
| 2,0 Mbps, 1% | produto | 261 | 261 | 261 | 2 | 0 | 0,0 | 100 | 100 | 87 | 97 | 0,0 | 64 | 156 | — | 0 |

| cenário | codificador | lat p50 | p95 | congelado % | quadros/s | orç. mediana kbps | recusados (fila) | não chegaram | pedidos de chave |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 3089–3351 | 3390–3619 | 96,6–98,2 | 3,8–4,0 | 12217–12480 | 338–348 | 0–2 | 0–0 |
| 0,3% | produto | 32–32 | 33–62 | 0,0–0,0 | 29,4–30,1 | 438–537 | 0–0 | 0–0 | 0–0 |
| 1% | produto | 32–32 | 111–144 | 1,3–100,5 | 0,0–30,1 | 200–403 | 0–0 | 0–601 | 0–0 |
| 3% | produto | 32–32 | 246–898 | 2,0–21,6 | 30,1–31,6 | 200–200 | 0–0 | 0–0 | 0–0 |
| 5% | produto | 33–33 | 888–941 | 21,8–100,5 | 0,0–30,1 | 200–200 | 0–0 | 0–601 | 0–0 |
| 1,0 Mbps, 0% | produto | 48–50 | 57–188 | 1,2–3,5 | 29,2–29,8 | 500–610 | 0–0 | 0–0 | 0–0 |
| 2,0 Mbps, 1% | produto | 35–36 | 65–99 | 0,0–0,0 | 29,8–29,9 | 200–277 | 0–0 | 0–0 | 0–0 |

| cenário | codificador | rodada | orçamento ao fim de cada segundo | paradas |
|---|---|--:|---|---|
| 0% | produto | 1 | (aquecimento: 6819 6819 8211 10192 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217) 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 12217 | — |
| 0% | produto | 2 | (aquecimento: 6866 6866 8583 10617 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480) 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 | — |
| 0% | produto | 3 | (aquecimento: 6957 6957 8696 10760 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480) 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 12480 | — |
| 0,3% | produto | 1 | (aquecimento: 6389 6389 7987 7987 6389 6389 4089 4089 3271 3271 2617 1455 1455 1317 1317 1317 1317 690 690 690 690 690 690 690 690 690 690 690 690 690 690 690 690 690 690 690 690 690 438 438) 438 438 438 438 438 438 438 438 438 438 438 438 438 438 438 438 438 438 438 438 | — |
| 0,3% | produto | 2 | (aquecimento: 5580 4464 3571 2856 1482 1482 1482 1455 888 888 888 888 888 888 888 888 888 888 888 888 888 888 888 888 888 537 537 537 537 537 537 537 537 537 537 537 537 537 537 537) 537 537 537 537 537 537 537 537 537 537 537 537 537 537 537 537 895 582 727 909 | — |
| 0,3% | produto | 3 | (aquecimento: 5580 5580 4464 3571 2856 2285 1751 1751 1444 1444 1444 1444 1444 1444 1444 1444 1444 1444 1444 1444 858 858 858 858 858 858 858 858 858 514 514 514 514 514 514 514 514 514 514 514) 514 514 514 514 514 514 514 514 514 514 514 514 514 514 514 514 514 514 514 856 | — |
| 1% | produto | 1 | (aquecimento: 5580 4464 3571 2856 2856 1556 1556 1245 1106 1106 1106 610 610 610 610 610 610 610 610 610 610 610 610 610 610 610 403 403 403 403 403 403 403 403 403 403 403 403 403 403) 403 403 403 403 403 403 403 403 403 403 403 403 403 403 403 403 672 672 481 481 | — |
| 1% | produto | 2 | (aquecimento: 5580 4464 3571 3571 2856 2285 2285 1029 843 843 843 843 843 481 481 481 481 481 481 481 331 331 331 331 331 258 258 258 258 258 258 258 258 258 258 258 258 202 202 202) 202 202 202 202 202 202 202 202 202 202 202 202 202 202 202 202 202 202 202 202 | — |
| 1% | produto | 3 | (aquecimento: 5580 4464 4464 2856 2285 1828 1828 1185 764 764 764 668 668 416 416 416 416 416 416 416 416 416 416 416 416 291 242 200 200 200 200 200 200 200 200 200 200 200 200 200) 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 | — |
| 3% | produto | 1 | (aquecimento: 5580 4464 3571 3571 2856 2285 1828 1462 1462 1170 1170 658 526 453 453 453 393 393 393 393 393 393 393 393 393 264 264 264 264 264 264 264 200 200 200 200 200 200 200 200) 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 | — |
| 3% | produto | 2 | (aquecimento: 5580 4464 3571 2856 2856 2285 2285 2285 1462 1170 1170 936 936 936 936 624 624 624 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200) 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 | — |
| 3% | produto | 3 | (aquecimento: 5580 4464 3571 2856 2285 1828 1828 1462 1462 936 936 936 936 936 601 601 385 385 385 385 385 277 277 277 277 277 277 208 208 208 200 200 200 200 200 200 200 200 200 200) 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 | — |
| 5% | produto | 1 | (aquecimento: 5580 4464 4464 3571 3571 1828 1828 1828 1828 1170 1170 1170 936 936 936 936 748 414 296 296 278 278 278 278 278 278 278 278 200 200 200 200 200 200 200 200 200 200 200 200) 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 | — |
| 5% | produto | 2 | (aquecimento: 5580 4464 3571 2856 2856 2285 2285 2285 2285 2285 2285 1462 1170 936 341 341 341 234 234 234 234 204 204 204 204 204 200 200 200 200 200 200 200 200 200 200 200 200 200 200) 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 | — |
| 5% | produto | 3 | (aquecimento: 5580 5580 4464 4464 4464 4464 4464 2856 2285 2285 2285 1462 1462 936 556 442 442 313 313 274 274 274 274 274 274 274 200 200 200 200 200 200 200 200 200 200 200 200 200 200) 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 | — |
| 1,0 Mbps, 0% | produto | 1 | (aquecimento: 5580 5580 3571 2856 2285 2285 2285 1462 1462 936 617 617 617 617 485 485 485 470 470 470 470 470 470 470 470 470 470 470 470 470 470 470 470 470 470 470 470 470 470 470) 470 470 470 470 470 470 470 784 512 640 640 801 1001 609 609 609 609 609 609 609 | — |
| 1,0 Mbps, 0% | produto | 2 | (aquecimento: 5580 4464 4464 2856 2856 2856 2285 1462 1462 1170 1170 1170 1170 627 606 606 606 606 529 529 499 499 499 499 499 499 499 499 499 499 499 499 499 499 499 499 499 499 499 499) 499 499 499 499 499 499 499 499 499 499 499 833 507 634 792 991 673 673 673 673 | — |
| 1,0 Mbps, 0% | produto | 3 | (aquecimento: 5580 5580 3571 2856 2285 2285 2285 2285 1828 1828 1170 1170 1170 581 581 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504 504) 504 504 504 504 504 504 504 504 504 841 559 559 559 559 559 559 559 559 559 559 | — |
| 2,0 Mbps, 1% | produto | 1 | (aquecimento: 5580 4464 3571 2856 2285 1828 1828 1828 1828 983 718 718 718 718 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380 380) 380 380 277 277 277 277 277 277 277 277 277 277 277 277 277 277 277 277 277 277 | — |
| 2,0 Mbps, 1% | produto | 2 | (aquecimento: 5580 4464 4464 4464 4464 3571 2285 976 976 723 723 723 723 723 459 459 459 459 459 459 459 459 459 459 459 459 459 459 459 459 459 459 459 459 321 321 321 321 321 321) 243 243 243 243 243 200 200 200 200 200 200 200 200 200 200 200 200 200 200 200 | — |
| 2,0 Mbps, 1% | produto | 3 | (aquecimento: 5580 4464 4464 4464 2285 1828 1828 1462 1462 1462 1462 855 655 655 607 607 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509 509) 509 509 509 509 509 340 340 340 340 261 261 261 261 261 261 261 261 261 261 261 | — |

## As variações (primeiros 20 s; mediana de 3 rodadas, salvo onde diz)

- **escolha de 1,47 Mbps** — a pessoa (ou a casca) dá um teto de banda igual à taxa do emissor fixo. O orçamento nasce em 1470 kbps em vez de 5580.
- **recusa sem chave** — depois de um `CameraQueueFull` o codificador de mentira não manda chave: é como se o quadro recusado não tivesse sido codificado. Aproxima o que a bomba da tela do sistema faz.
- **fração 1,0** — o delta é orçamento ÷ 30 ÷ 8, sem os 85% da casca. Uma rodada.

| cenário | variação | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | aceitos | recusados (fila) | não chegaram | chaves | pedidos de chave | kbps oferecidos | MB no cano | voz tocada % | cano perda % | cano fila % |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto (o da grade) | 415 | 2751 | 3007 | 3101 | 55,3 | 383 | 14,8 | 307 | 161 | 0 | 44 | 0 | 9959 | 23,36 | 100,0 | 0,00 | 0,00 |
| 0% | produto, escolha de 1,47 Mbps | 32 | 33 | 34 | 226 | 1,1 | 227 | 29,9 | 598 | 0 | 0 | 1 | 0 | 1260 | 3,60 | 100,4 | 0,00 | 0,00 |
| 0% | produto, recusa sem chave | 723 | 771 | 784 | 792 | 6,0 | 359 | 26,1 | 540 | 46 | 0 | 5 | 0 | 9231 | 24,05 | 100,1 | 0,00 | 0,00 |
| 0% | produto, fração 1,0 (1 rodada) | 315 | 3741 | 4069 | 4282 | 70,6 | 424 | 10,2 | 218 | 239 | 5 | 49 | 1 | 10562 | 23,41 | 100,0 | 0,00 | 0,00 |
| 1% | produto (o da grade) | 33 | 3547 | 4963 | 5411 | 42,6 | 1071 | 16,1 | 343 | 189 | 17 | 31 | 10 | 1219 | 3,21 | 99,5 | 0,95 | 0,00 |
| 1% | produto, escolha de 1,47 Mbps | 58 | 942 | 1129 | 1139 | 3,4 | 480 | 29,3 | 586 | 0 | 0 | 5 | 0 | 849 | 2,50 | 100,0 | 0,89 | 0,00 |
| 1% | produto, recusa sem chave | 65 | 2231 | 2746 | 3032 | 15,6 | 1062 | 22,3 | 446 | 125 | 0 | 10 | 0 | 1012 | 2,92 | 99,8 | 0,95 | 0,00 |
| 1% | produto, fração 1,0 (1 rodada) | 56 | 2557 | 6110 | 6355 | 30,6 | 1285 | 17,6 | 368 | 168 | 16 | 25 | 9 | 1553 | 3,70 | 99,8 | 0,81 | 0,00 |
| 1,0 Mbps, 0% | produto (o da grade) | 283 | 7556 | 10076 | 10200 | 63,3 | 3553 | 8,9 | 199 | 327 | 20 | 28 | 10 | 871 | 2,11 | 87,7 | 0,00 | 1,77 |
| 1,0 Mbps, 0% | produto, escolha de 1,47 Mbps | 457 | 3955 | 4369 | 4434 | 60,7 | 1323 | 14,8 | 326 | 164 | 20 | 41 | 8 | 803 | 2,07 | 95,5 | 0,00 | 1,55 |
| 1,0 Mbps, 0% | produto, recusa sem chave | 425 | 5296 | 6016 | 6249 | 40,1 | 3767 | 16,4 | 328 | 234 | 0 | 12 | 0 | 723 | 2,13 | 92,5 | 0,00 | 1,38 |
| 1,0 Mbps, 0% | produto, fração 1,0 (1 rodada) | 404 | 6750 | 8768 | 8769 | 76,0 | 3758 | 9,2 | 210 | 311 | 27 | 39 | 18 | 974 | 2,17 | 84,2 | 0,00 | 1,79 |
| 2,0 Mbps, 1% | produto (o da grade) | 42 | 4314 | 6837 | 7939 | 50,9 | 1354 | 15,0 | 317 | 220 | 14 | 28 | 8 | 1137 | 2,84 | 99,2 | 0,89 | 0,00 |
| 2,0 Mbps, 1% | produto, escolha de 1,47 Mbps | 119 | 2018 | 2107 | 2229 | 15,3 | 538 | 27,0 | 554 | 8 | 14 | 14 | 2 | 937 | 2,49 | 99,8 | 0,88 | 0,00 |
| 2,0 Mbps, 1% | produto, recusa sem chave | 87 | 2720 | 3455 | 3554 | 27,5 | 1380 | 21,6 | 432 | 136 | 0 | 11 | 0 | 874 | 2,55 | 99,1 | 0,88 | 0,00 |
| 2,0 Mbps, 1% | produto, fração 1,0 (1 rodada) | 69 | 4557 | 7386 | 8013 | 53,6 | 1546 | 13,3 | 290 | 230 | 23 | 35 | 13 | 1390 | 3,18 | 99,2 | 0,82 | 0,19 |

| cenário | variação | orç. mín kbps | orç. mediana kbps | orç. final kbps | avisos de orçamento | pausas | parado % | sinal servidor mín | sinal servidor med. | sinal local mín | sinal local med. | fora de Nominal % | rtt p50 ms | rtt máx ms | perda medida máx % | reaberturas em quem assiste |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto (o da grade) | 5580 | 12480 | 12480 | 4 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 65 | — | 0 |
| 0% | produto, escolha de 1,47 Mbps | 1470 | 1470 | 1470 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 62 | 64 | — | 0 |
| 0% | produto, recusa sem chave | 5580 | 12480 | 12480 | 4 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 64 | — | 0 |
| 0% | produto, fração 1,0 (1 rodada) | 5580 | 12480 | 12480 | 4 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 64 | — | 0 |
| 1% | produto (o da grade) | 495 | 746 | 495 | 9 | 0 | 0,0 | 100 | 100 | 60 | 66 | 0,0 | 341 | 568 | — | 0 |
| 1% | produto, escolha de 1,47 Mbps | 473 | 750 | 473 | 4 | 0 | 0,0 | 100 | 100 | 61 | 91 | 0,0 | 94 | 568 | — | 0 |
| 1% | produto, recusa sem chave | 675 | 719 | 675 | 9 | 0 | 0,0 | 100 | 100 | 60 | 82 | 0,0 | 140 | 537 | — | 0 |
| 1% | produto, fração 1,0 (1 rodada) | 633 | 1013 | 633 | 8 | 0 | 0,0 | 100 | 100 | 60 | 75 | 0,0 | 273 | 533 | — | 0 |
| 1,0 Mbps, 0% | produto (o da grade) | 512 | 1170 | 512 | 10 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 608 | 2272 | — | 0 |
| 1,0 Mbps, 0% | produto, escolha de 1,47 Mbps | 501 | 632 | 501 | 5 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 602 | 1318 | — | 0 |
| 1,0 Mbps, 0% | produto, recusa sem chave | 528 | 634 | 528 | 13 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 608 | 1221 | — | 0 |
| 1,0 Mbps, 0% | produto, fração 1,0 (1 rodada) | 550 | 936 | 550 | 11 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 579 | 1951 | — | 0 |
| 2,0 Mbps, 1% | produto (o da grade) | 471 | 725 | 471 | 9 | 0 | 0,0 | 100 | 100 | 60 | 68 | 0,0 | 319 | 704 | — | 0 |
| 2,0 Mbps, 1% | produto, escolha de 1,47 Mbps | 438 | 699 | 438 | 4 | 0 | 0,0 | 100 | 100 | 60 | 84 | 0,0 | 70 | 564 | — | 0 |
| 2,0 Mbps, 1% | produto, recusa sem chave | 277 | 613 | 277 | 10 | 0 | 0,0 | 100 | 100 | 60 | 82 | 0,0 | 91 | 718 | — | 0 |
| 2,0 Mbps, 1% | produto, fração 1,0 (1 rodada) | 383 | 979 | 383 | 11 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 390 | 1077 | — | 0 |

### Duas conferências do cenário sem perda (1 rodada de 10 s cada)

| o que mudou | orçamento ao fim de cada segundo (kbps) | lat p50 | quadros/s | congelado % |
|---|---|--:|--:|--:|
| o cano sem atraso (`--atraso-ms 0`) | 5580 11160 12480 12480 12480 12480 12480 12480 12480 12480 | 2 | 29,2 | 0,0 |
| escolha de 4 Mbps (`--escolha-bps 4000000`), 30 ms por sentido | 4000 em todos | 32 | 29,8 | 3,4 |

## O que os números mostram

1. **O banco reproduz a medida anterior.** O emissor cru, contra o servidor e o cliente de verdade, dá os números da linha «A»: 32 ms e 30 quadros/s sem perda; 265 ms a 1% (lá 437; a faixa entre rodadas aqui é 122–431); 1314 ms e 16,6 quadros/s a 3% (lá 1528 e 14,2); 2129 ms e 8,3 a 5% (lá 2518 e 8,4); 1156 ms e 17,2 a 1,0 Mbps (lá 1229 e 15,5); 612 ms e 27,4 a 2,0 Mbps com 1% (lá 719 e 24,5). O servidor no meio e a pessoa assistindo não mudam a ordem de grandeza de nada.
2. **O emissor fixo pela porta da câmera não reproduz a linha «A», e a razão é o produto.** Sem perda ele é igual (32 ms, 30,1 quadros/s). Com perda o orçamento desce abaixo de 1,47 Mbps em poucos segundos (item 4), e o balde passa a descartar o que o codificador insiste em mandar: a 1% foram 258 pedidos de chave em 20 s e 270 quadros aceitos que não chegaram; com o teto abaixo de ~650 kbps a chave de 80 KiB não cabe no balde de um segundo e é descartada toda vez. Não é um controle da rede: é o que acontece com um codificador que não obedece.
3. **A faixa de sinal não se moveu, em nenhuma célula.** Nas 91 células de «produto» e «fixo» (grade, regime e variações), o `signal` que o servidor devolveu a quem transmite foi 100 em todas as amostras, a faixa foi `Nominal` 100% do tempo, e não houve nenhum `CameraPaused`. A telemetria de perda (`loss_fraction`) nunca veio medida. O sinal **local**, o que a janela mostra, desceu até 60. As duas contas são diferentes, e só a do servidor entra no teto.
4. **O orçamento reage à perda aleatória, e reage descendo até o piso.** Em toda célula com perda o orçamento saiu de 5580 kbps e só desceu: aos 20 s estava (mediana do valor final) em 832 kbps a 0,3%, 495 a 1%, 379 a 3% e 344 a 5%, num caminho **sem limite de banda**. Entre 40 e 60 s estava (mediana) em 514 kbps a 0,3% e em 200–202 kbps — o piso — a 1%, 3% e 5%. A 2,0 Mbps com 1% chegou a 261 kbps, contra os 1200 que 60% daquele caminho dariam.
5. **Na banda estreita sem perda ele converge para perto do lugar certo, depois de 11 a 15 s.** A 1,0 Mbps o orçamento parte de 5580, chega à vizinhança dos 600 kbps (60% do caminho) entre o 11º e o 15º segundo, e fica em 470–610 kbps. Entre os 48 e os 52 s a sonda pergunta para cima (até 841–1001 kbps) e em 1 a 6 s volta para 559–673.
6. **Os primeiros 20 s são piores que a linha «A» em tempo congelado, em todos os cenários.** Produto: 22,6% do tempo congelado a 0,3%, 42,6% a 1%, 89,1% a 3%, 96,5% a 5%, 63,3% a 1,0 Mbps, 50,9% a 2,0 Mbps com 1%. Cru: 1,5%, 1,8%, 15,7%, 49,3%, 21,3%, 3,4%. A mediana da latência do produto é menor até 1% (33 ms contra 265 ms a 1%) e a cauda é maior (p95 de 3547 ms contra 682 ms): o que chega, chega rápido, e entre uma chegada e outra há buracos de segundos.
7. **Depois que o orçamento assenta, a latência some — a 200 kbps.** Do segundo 40 ao 60, nos seis cenários com perda ou gargalo: p50 de 32–49 ms e ~30 quadros/s; congelado 0,0% a 0,3% e a 2,0 Mbps com 1%, 3,4% a 1,0 Mbps, 5,5% a 1% (faixa 1,3–100,5), 13,2% a 3%, 29,0% a 5%. Nenhum quadro recusado, nenhuma chave pedida. O preço é o orçamento: 200 kbps para 720p a 30 quadros, e o quadro de mentira não diz como isso fica na tela (ver «NÃO mostram»).
8. **Sem perda nenhuma, a 60 ms de ida e volta, o produto se estraga sozinho.** O orçamento sobe a 12480 kbps em 5 s e fica. O codificador obediente oferece ~10 Mbps; o cano viu passar 23,4 MB em 20 s (≈9,3 Mbps), e o resto virou fila: 161 quadros recusados e 44 chaves nos primeiros 20 s (p50 de 415 ms, 14,8 quadros/s, 55,3% congelado), e do segundo 40 ao 60, 340 recusados, p50 de 3126 ms, 4,0 quadros/s, 97,9% congelado. A sonda não viu dor: o rtt da telemetria ficou em 63 ms. Com o cano sem atraso, o mesmo orçamento passa (2 ms, 29,2 quadros/s, uma rodada); com uma escolha de 4 Mbps a 60 ms de ida e volta, também (32 ms, 29,8 quadros/s). O emissor fixo e o cru, a 1,47 Mbps, não têm problema nenhum nesse cenário.
9. **A recusa seguida de chave é parte grande do estrago, e não é todo ele.** Sem a chave depois da recusa: 15,6% congelado a 1% (42,6% com), 27,5% a 2,0 Mbps com 1% (50,9%), 40,1% a 1,0 Mbps (63,3%), 6,0% sem perda (55,3%). Os quadros recusados continuam lá (125 a 1%, 234 a 1,0 Mbps): a rede não leva o orçamento dos primeiros segundos.
10. **Com um teto escolhido igual à taxa da linha «A», o produto empata ou ganha dela nos primeiros 20 s, menos na banda estreita.** Escolha de 1,47 Mbps: sem perda, 32 ms e 1,1% congelado; a 1%, p50 de 58 ms, 3,4% congelado, 29,3 quadros/s (cru: 265 ms, 1,8%, 29,2); a 2,0 Mbps com 1%, 119 ms e 15,3% (cru: 612 ms e 3,4%); a 1,0 Mbps, 457 ms e 60,7% (cru: 1156 ms e 21,3%).
11. **A fração de 85% ou de 100% não muda a ordem de grandeza** (uma rodada): a 1%, 30,6% congelado e 17,6 quadros/s com 1,0, contra 42,6% e 16,1 com 0,85 na grade; sem perda, 70,6% contra 55,3%.
12. **O servidor corta quem assiste, e a imagem não volta.** Em 2 das 21 rodadas do regime (1% e 5%) quem assiste não recebeu **nenhum** quadro na janela, com o tráfego no cano igual ao das rodadas vivas e 601 quadros aceitos. Numa conferência com o registro ligado o caso se repetiu (1 de 10 rodadas), e o registro diz o que foi: `espectador cortado no meio do fluxo da tela` no servidor, 13 s depois de a câmera ligar, e `a transmissão alheia terminou torta … o par cortou o fluxo (código 1)` em quem assiste. Dali até o fim (47 s) quem assiste viu `ScreenClosed` e mais nada, e quem transmite não ficou sabendo: continuou com os quadros aceitos e o orçamento andando. Na grade dos primeiros 20 s nenhuma rodada de «produto» tem essa assinatura (o maior «não chegaram» é 21); na variação da escolha a 1,0 Mbps uma rodada tem a mesma assinatura (325 não chegaram, 98,5% congelado), sem registro que a confirme.
13. **A voz.** Tocou 99–100% dos blocos em quem assiste até 1% de perda e no regime inteiro. Nos primeiros 20 s caiu a 96,1% a 3%, 84,0% a 5% e 87,7% a 1,0 Mbps (produto; o fixo dá 96,1%, 83,2% e 92,7%). O atraso da voz não foi medido aqui.

### Lido no código, que explica parte disso (não é medida isolada)

- **Por que a faixa não se move.** O servidor só alimenta o sinal quando tem ida e volta **e** perda medidas (`seele-server/src/session.rs`, `match (rtt_ms, loss_fraction)`); a perda vem de `Leitura::fracao_de_perda_na_saida`, que o `seele-enlace` documenta como vinda dos relatórios RTCP — «sem mídia RTP fica em `None`» —, e o produto não manda RTP. Sem as duas, o sinal é o «ainda sem medida» do `Signal`, que vale 100. O corte pela metade em `Degraded` e a parada em `Critical` de `TetoDeVideo::teto` dependem dessa faixa. A medida confirma o resultado (item 3); o caminho por dentro é leitura.
- **Por que o orçamento só desce com perda aleatória.** A única dor que a sonda mede desde a 0.17 é a ida e volta do `Ping` dela no `ctl`, um canal confiável (`caminho.rs`, `IdaEVolta`; perda e congestionamento são `None`). Um `Ping` ou `Pong` perdido volta por retransmissão, mais de 40 ms (`FILA_TOLERADA`) acima da linha de base: a janela «doeu». Dor grava `limite_bps`, e daí a estimativa não sobe sozinha; só a sondagem, depois de 30 s de calmaria, pergunta para cima. A perda sorteada não é congestionamento e é lida como se fosse.
- **Por que 5580 kbps na partida.** `Comando::CompartilharTela` troca a sonda que nunca mediu por `Sonda::partindo_de(limites.caminho_inicial_bps())`: para 720p, 30 quadros e movimento são 9,3 Mbps de caminho, e 60% disso são 5580 kbps. Vale para a primeira transmissão de uma sessão; a segunda parte do que a primeira mediu (não medido aqui).
- **Por que 12,48 Mbps não passam a 60 ms.** `LIMIAR_DE_VOLUME` (`seele-enlace/src/ponta.rs`): os canais de volume só escrevem no SCTP com menos de 64 KiB esperando na associação. É um candidato a explicar o teto de ≈9,3 Mbps que o cano viu a 62 ms de ida e volta, e a ausência dele sem atraso; **não foi isolado**.
- **Por que o corte de quem assiste.** `VoiceRoom::escrever` (`seele-server/src/voice_room.rs`) dá a cada espectador um canal de 64 pedaços (`PEDACOS_DEPTH`) e, com ele cheio, tira o espectador. O que encheu o canal de quem assistia por um caminho limpo não foi determinado.

## O que os números NÃO mostram

- **O codificador é de mentira, e é obediente demais.** Os quadros têm exatamente o tamanho que o orçamento manda. Um codificador de verdade não enche 10,6 Mbps com uma câmera de 720p parada, e pode estourar o orçamento numa cena movimentada. O item 8 é o que acontece **se** a casca entregar o que o `CameraBudget` autoriza; quanto um `VideoEncoder` de verdade entrega com 10,6 Mbps de alvo não foi medido.
- **A chave de 8 × o delta é suposição.** A medida anterior usava 13 × (80 KiB para 6 KiB). Não foi variado (`--fator-da-chave` existe).
- **Não há imagem.** «30 quadros/s a 200 kbps» é uma frase sobre bytes. Pelas constantes do próprio produto (`bits_por_quadro(P720)` = 93 000 bits, 0,10 bit por pixel), 720p a 30 quadros pede 2,8 Mbps para texto nítido; 200 kbps são 7% disso. Como fica a imagem de uma câmera a 200 kbps não foi olhado por ninguém aqui.
- **É o caminho da câmera, não o da tela do sistema.** O teto, a sonda, a faixa e a `Transmissao` são os mesmos (leitura de código, acima), e a bomba da tela do sistema reage a um engasgo de outro jeito. A variação «recusa sem chave» é uma aproximação, não a bomba.
- **Os primeiros 20 s de uma primeira transmissão.** Toda célula é uma sessão nova, com a sonda partindo de 9,3 Mbps. O regime foi medido só dos 40 aos 60 s. Uma segunda transmissão na mesma sessão, uma lembrança de caminho gravada em disco e uma conversa de dez minutos (com as sondagens a cada 30 s ou mais) não foram medidas.
- **Um só atraso.** 30 ms por sentido em todas as células, menos uma conferência. O item 8 depende da ida e volta, e não foi medido a 20 nem a 150 ms.
- **Loopback, num processo só**, perda independente e não em rajada, sem jitter, sem tráfego concorrente, um espectador, caminho de quem assiste limpo: as mesmas ressalvas da medida anterior. O sinal do servidor não se moveu **sem jitter**; o jitter não entra na conta do servidor (leitura), mas não foi posto à prova.
- **`debug-assertions` ligado.** O binário medido é o perfil `release` com as asserções de depuração, por causa do aparelho de mentira. Não é o binário que vai para quem usa. A conferência sem atraso (12,48 Mbps, 29,2 quadros/s, 2 ms) diz que o banco não é o gargalo; o custo das asserções não foi medido.
- **A voz foi medida por cima.** Só se o bloco tocou. Atraso, perda de pacote e jitter da voz não saem daqui; a medida anterior tem os da voz no canal.
- **O corte de quem assiste não está contado na grade.** A coluna «não chegaram» mostra a assinatura, mas as execuções das tabelas não registraram o `ScreenClosed` de quem assiste; o exemplo passou a escrevê-lo numa linha de nota depois delas. Das três rodadas com a assinatura no regime, uma tem o registro do servidor; as outras duas são inferência. Quanto tempo quem assiste fica sem imagem além dos 47 s observados, e se alguma ação dele a traz de volta, não foi medido.
- **3 rodadas de 20 s.** As faixas entre rodadas são largas com perda (p50 do produto a 3%: 322 a 5047 ms). A variação da fração tem uma rodada só, e as duas conferências, uma de 10 s.
- **A máquina não estava sozinha** nas variações (carga até 5,21).
- **Nenhum conserto foi tentado.** Um teto padrão para a câmera, uma sonda que distinga perda de fila, um sinal do servidor alimentado, ou uma fila de câmera que não custe chave não foram medidos: a escolha de 1,47 Mbps é o único desses que tem número, e é uma opção que o produto já tem.

## Apêndice: todas as rodadas

### A grade (primeiros 20 s): o que quem assiste vê

| cenário | codificador | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | aceitos | recusados (fila) | não chegaram | chaves | pedidos de chave | kbps oferecidos | MB no cano | voz tocada % | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 1 | 404 | 2883 | 3348 | 3365 | 56,8 | 382 | 14,7 | 305 | 161 | 1 | 45 | 0 | 9983 | 23,29 | 100,0 | 0,00 | 0,00 |
| 0% | produto | 2 | 428 | 2751 | 3007 | 3062 | 55,2 | 383 | 14,8 | 308 | 161 | 0 | 44 | 0 | 9959 | 23,39 | 100,0 | 0,00 | 0,00 |
| 0% | produto | 3 | 415 | 2506 | 2968 | 3101 | 55,3 | 384 | 14,8 | 307 | 165 | 0 | 43 | 0 | 9809 | 23,36 | 100,0 | 0,00 | 0,00 |
| 0% | fixo | 1 | 32 | 33 | 153 | 291 | 1,5 | 292 | 30,1 | 601 | 0 | 0 | 1 | 0 | 1507 | 4,27 | 100,1 | 0,00 | 0,00 |
| 0% | fixo | 2 | 32 | 33 | 153 | 293 | 1,5 | 294 | 30,1 | 601 | 0 | 0 | 1 | 0 | 1507 | 4,27 | 100,2 | 0,00 | 0,00 |
| 0% | fixo | 3 | 32 | 33 | 162 | 297 | 1,5 | 298 | 30,1 | 601 | 0 | 0 | 1 | 0 | 1507 | 4,27 | 100,2 | 0,00 | 0,00 |
| 0,3% | produto | 1 | 124 | 3508 | 4114 | 4187 | 52,8 | 718 | 14,4 | 303 | 186 | 14 | 41 | 8 | 2791 | 7,26 | 100,3 | 0,32 | 0,00 |
| 0,3% | produto | 2 | 32 | 2214 | 3591 | 3918 | 22,6 | 1073 | 20,8 | 432 | 101 | 17 | 25 | 6 | 1545 | 3,82 | 100,0 | 0,32 | 0,00 |
| 0,3% | produto | 3 | 33 | 1657 | 2875 | 3153 | 15,8 | 591 | 22,8 | 472 | 75 | 17 | 25 | 10 | 1808 | 4,47 | 100,2 | 0,41 | 0,00 |
| 0,3% | fixo | 1 | 33 | 202 | 261 | 294 | 1,5 | 296 | 30,1 | 601 | 0 | 0 | 1 | 0 | 1507 | 4,25 | 100,2 | 0,23 | 0,00 |
| 0,3% | fixo | 2 | 97 | 544 | 687 | 719 | 38,3 | 1843 | 20,4 | 601 | 0 | 193 | 118 | 118 | 5054 | 3,61 | 99,8 | 0,36 | 0,00 |
| 0,3% | fixo | 3 | 60 | 198 | 261 | 293 | 1,5 | 295 | 30,1 | 601 | 0 | 0 | 1 | 0 | 1507 | 4,24 | 100,2 | 0,38 | 0,00 |
| 1% | produto | 1 | 33 | 2764 | 4522 | 4629 | 27,0 | 1040 | 18,5 | 387 | 142 | 17 | 31 | 10 | 1383 | 3,38 | 99,5 | 0,82 | 0,00 |
| 1% | produto | 2 | 33 | 3547 | 6814 | 7495 | 42,6 | 1406 | 16,1 | 343 | 189 | 21 | 29 | 11 | 1211 | 2,80 | 99,5 | 0,95 | 0,00 |
| 1% | produto | 3 | 59 | 4498 | 4963 | 5411 | 47,4 | 1071 | 14,2 | 297 | 218 | 14 | 31 | 6 | 1219 | 3,21 | 99,7 | 0,99 | 0,00 |
| 1% | fixo | 1 | 291 | 662 | 696 | 711 | 21,4 | 2486 | 23,4 | 601 | 0 | 132 | 88 | 88 | 4144 | 3,52 | 100,2 | 0,84 | 0,00 |
| 1% | fixo | 2 | 1474 | 5148 | 5988 | 6105 | 81,2 | 5730 | 5,0 | 370 | 231 | 270 | 287 | 265 | 9608 | 2,46 | 99,3 | 0,90 | 0,00 |
| 1% | fixo | 3 | 501 | 1313 | 1355 | 1362 | 55,1 | 7580 | 13,6 | 589 | 12 | 318 | 263 | 258 | 9419 | 2,38 | 99,6 | 0,95 | 0,00 |
| 3% | produto | 1 | 4466 | 16291 | 16546 | 16546 | 89,1 | 2347 | 1,5 | 53 | 479 | 9 | 23 | 6 | 961 | 2,07 | 96,1 | 3,15 | 0,00 |
| 3% | produto | 2 | 5047 | 15582 | 16267 | 16267 | 93,0 | 4049 | 1,4 | 41 | 497 | 13 | 21 | 0 | 819 | 1,70 | 80,9 | 3,80 | 0,00 |
| 3% | produto | 3 | 322 | 8399 | 11479 | 12367 | 72,9 | 2231 | 7,9 | 173 | 356 | 15 | 25 | 8 | 937 | 2,19 | 97,2 | 2,24 | 0,00 |
| 3% | fixo | 1 | 3265 | 10855 | 11910 | 11910 | 89,3 | 1913 | 2,7 | 142 | 459 | 88 | 78 | 58 | 2713 | 1,75 | 96,1 | 3,39 | 0,00 |
| 3% | fixo | 2 | 2826 | 11474 | 12108 | 12108 | 86,3 | 4807 | 2,6 | 239 | 362 | 187 | 199 | 182 | 6619 | 1,43 | 96,1 | 3,75 | 0,00 |
| 3% | fixo | 3 | 1949 | 8497 | 10039 | 10039 | 87,3 | 6762 | 3,7 | 316 | 285 | 242 | 255 | 237 | 8506 | 1,69 | 98,6 | 2,23 | 0,00 |
| 5% | produto | 1 | 6282 | 11743 | 15464 | 15464 | 96,5 | 4120 | 1,4 | 52 | 489 | 21 | 20 | 5 | 671 | 1,40 | 84,0 | 5,76 | 0,00 |
| 5% | produto | 2 | 6832 | 14913 | 16781 | 16781 | 96,6 | 3567 | 1,1 | 36 | 514 | 11 | 17 | 0 | 760 | 1,20 | 75,2 | 5,91 | 0,00 |
| 5% | produto | 3 | 1078 | 6179 | 12562 | 13658 | 75,9 | 2373 | 8,9 | 195 | 346 | 17 | 22 | 7 | 856 | 1,64 | 94,0 | 3,84 | 0,00 |
| 5% | fixo | 1 | 4862 | 15034 | 16126 | 16126 | 88,4 | 2110 | 2,6 | 65 | 536 | 12 | 18 | 0 | 705 | 1,14 | 83,2 | 5,44 | 0,00 |
| 5% | fixo | 2 | 2826 | 15769 | 16554 | 16554 | 85,4 | 2595 | 2,5 | 142 | 459 | 91 | 101 | 86 | 3410 | 1,32 | 82,7 | 5,89 | 0,00 |
| 5% | fixo | 3 | 3988 | 12508 | 12766 | 12766 | 87,4 | 1846 | 3,0 | 72 | 529 | 10 | 26 | 0 | 965 | 1,57 | 97,5 | 4,26 | 0,00 |
| 1,0 Mbps, 0% | produto | 1 | 500 | 8739 | 10076 | 10200 | 64,1 | 3423 | 8,9 | 199 | 327 | 20 | 28 | 10 | 871 | 2,11 | 90,3 | 0,00 | 1,77 |
| 1,0 Mbps, 0% | produto | 2 | 283 | 7556 | 10111 | 11058 | 63,3 | 3553 | 8,2 | 185 | 344 | 20 | 28 | 10 | 980 | 2,13 | 83,3 | 0,00 | 1,72 |
| 1,0 Mbps, 0% | produto | 3 | 48 | 6342 | 8456 | 8685 | 57,0 | 3661 | 11,2 | 242 | 291 | 19 | 27 | 10 | 826 | 1,98 | 87,7 | 0,00 | 1,83 |
| 1,0 Mbps, 0% | fixo | 1 | 3856 | 7429 | 8498 | 8498 | 89,6 | 6988 | 2,8 | 308 | 293 | 252 | 263 | 247 | 8729 | 1,42 | 88,0 | 0,00 | 2,11 |
| 1,0 Mbps, 0% | fixo | 2 | 3826 | 6859 | 8188 | 8188 | 89,5 | 7639 | 2,8 | 327 | 274 | 272 | 281 | 266 | 9321 | 1,36 | 92,7 | 0,00 | 2,12 |
| 1,0 Mbps, 0% | fixo | 3 | 2892 | 8712 | 9771 | 9771 | 95,6 | 7454 | 2,8 | 321 | 280 | 265 | 276 | 260 | 9155 | 1,54 | 98,9 | 0,00 | 2,11 |
| 2,0 Mbps, 1% | produto | 1 | 41 | 4314 | 6202 | 6628 | 46,4 | 1354 | 15,2 | 319 | 208 | 14 | 28 | 8 | 1137 | 2,87 | 99,2 | 0,83 | 0,17 |
| 2,0 Mbps, 1% | produto | 2 | 44 | 3952 | 6837 | 8495 | 50,9 | 1410 | 15,0 | 317 | 220 | 17 | 24 | 7 | 1169 | 2,57 | 98,8 | 0,89 | 0,00 |
| 2,0 Mbps, 1% | produto | 3 | 42 | 4842 | 7726 | 7939 | 51,7 | 1284 | 13,6 | 284 | 242 | 13 | 28 | 8 | 1117 | 2,84 | 99,5 | 0,99 | 0,00 |
| 2,0 Mbps, 1% | fixo | 1 | 780 | 1595 | 1783 | 2065 | 53,7 | 6340 | 13,4 | 573 | 28 | 305 | 289 | 284 | 10168 | 2,44 | 99,8 | 0,83 | 0,00 |
| 2,0 Mbps, 1% | fixo | 2 | 1967 | 5874 | 6251 | 6251 | 79,9 | 6575 | 4,2 | 308 | 293 | 224 | 242 | 219 | 8092 | 2,19 | 99,7 | 0,99 | 0,00 |
| 2,0 Mbps, 1% | fixo | 3 | 882 | 1939 | 2554 | 2848 | 54,3 | 9155 | 12,2 | 542 | 59 | 299 | 300 | 293 | 10425 | 2,03 | 99,5 | 0,88 | 0,00 |

### A grade (primeiros 20 s): o que o controle de taxa fez

| cenário | codificador | rodada | orç. mín kbps | orç. mediana kbps | orç. final kbps | avisos de orçamento | pausas | parado % | sinal servidor mín | sinal servidor med. | sinal local mín | sinal local med. | fora de Nominal % | rtt p50 ms | rtt máx ms | perda medida máx % | reaberturas em quem assiste |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 1 | 5580 | 12480 | 12480 | 4 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 62 | 64 | — | 0 |
| 0% | produto | 2 | 5580 | 12480 | 12480 | 4 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 65 | — | 0 |
| 0% | produto | 3 | 5580 | 12480 | 12480 | 4 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 64 | 65 | — | 0 |
| 0% | fixo | 1 | 5580 | 5580 | 5580 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 64 | — | 0 |
| 0% | fixo | 2 | 5580 | 5580 | 5580 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 65 | — | 0 |
| 0% | fixo | 3 | 5580 | 5580 | 5580 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 65 | — | 0 |
| 0,3% | produto | 1 | 623 | 3284 | 623 | 10 | 0 | 0,0 | 100 | 100 | 84 | 92 | 0,0 | 65 | 181 | — | 0 |
| 0,3% | produto | 2 | 902 | 902 | 902 | 6 | 0 | 0,0 | 100 | 100 | 60 | 82 | 0,0 | 64 | 444 | — | 0 |
| 0,3% | produto | 3 | 832 | 1500 | 832 | 5 | 0 | 0,0 | 100 | 100 | 62 | 91 | 0,0 | 122 | 391 | — | 0 |
| 0,3% | fixo | 1 | 2286 | 5580 | 2286 | 4 | 0 | 0,0 | 100 | 100 | 91 | 97 | 0,0 | 64 | 126 | — | 0 |
| 0,3% | fixo | 2 | 627 | 1190 | 627 | 7 | 0 | 0,0 | 100 | 100 | 77 | 95 | 0,0 | 63 | 246 | — | 0 |
| 0,3% | fixo | 3 | 996 | 3571 | 996 | 6 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 64 | 65 | — | 0 |
| 1% | produto | 1 | 495 | 1013 | 495 | 9 | 0 | 0,0 | 100 | 100 | 60 | 66 | 0,0 | 341 | 568 | — | 0 |
| 1% | produto | 2 | 461 | 746 | 461 | 9 | 0 | 0,0 | 100 | 100 | 60 | 77 | 0,0 | 142 | 450 | — | 0 |
| 1% | produto | 3 | 582 | 665 | 582 | 9 | 0 | 0,0 | 100 | 100 | 60 | 62 | 0,0 | 375 | 627 | — | 0 |
| 1% | fixo | 1 | 378 | 1463 | 378 | 12 | 0 | 0,0 | 100 | 100 | 60 | 62 | 0,0 | 384 | 1034 | — | 0 |
| 1% | fixo | 2 | 465 | 735 | 465 | 9 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 420 | 485 | — | 0 |
| 1% | fixo | 3 | 374 | 840 | 374 | 10 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 507 | 675 | — | 0 |
| 3% | produto | 1 | 241 | 2286 | 241 | 12 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 849 | 1690 | — | 0 |
| 3% | produto | 2 | 538 | 1463 | 538 | 9 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 524 | 4610 | — | 0 |
| 3% | produto | 3 | 379 | 936 | 379 | 13 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 591 | 1253 | — | 0 |
| 3% | fixo | 1 | 249 | 1463 | 249 | 12 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 850 | 1690 | — | 0 |
| 3% | fixo | 2 | 250 | 936 | 250 | 13 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 712 | 1564 | — | 0 |
| 3% | fixo | 3 | 337 | 679 | 337 | 12 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 457 | 616 | — | 0 |
| 5% | produto | 1 | 200 | 1463 | 200 | 14 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 1154 | 1445 | — | 0 |
| 5% | produto | 2 | 1170 | 1463 | 1170 | 7 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 944 | 2972 | — | 0 |
| 5% | produto | 3 | 344 | 1463 | 344 | 11 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 960 | 2142 | — | 0 |
| 5% | fixo | 1 | 200 | 2286 | 200 | 13 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 1780 | 3037 | — | 0 |
| 5% | fixo | 2 | 214 | 1463 | 214 | 14 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 838 | 1241 | — | 0 |
| 5% | fixo | 3 | 483 | 1170 | 483 | 10 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 865 | 1332 | — | 0 |
| 1,0 Mbps, 0% | produto | 1 | 512 | 1463 | 512 | 9 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 629 | 2414 | — | 0 |
| 1,0 Mbps, 0% | produto | 2 | 524 | 1170 | 524 | 10 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 608 | 1941 | — | 0 |
| 1,0 Mbps, 0% | produto | 3 | 483 | 1170 | 483 | 10 | 0 | 0,0 | 100 | 100 | 60 | 62 | 0,0 | 592 | 2272 | — | 0 |
| 1,0 Mbps, 0% | fixo | 1 | 505 | 611 | 505 | 9 | 0 | 0,0 | 100 | 100 | 60 | 62 | 0,0 | 681 | 2290 | — | 0 |
| 1,0 Mbps, 0% | fixo | 2 | 511 | 609 | 511 | 9 | 0 | 0,0 | 100 | 100 | 60 | 62 | 0,0 | 652 | 2253 | — | 0 |
| 1,0 Mbps, 0% | fixo | 3 | 495 | 622 | 495 | 9 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 617 | 859 | — | 0 |
| 2,0 Mbps, 1% | produto | 1 | 420 | 685 | 420 | 9 | 0 | 0,0 | 100 | 100 | 60 | 82 | 0,0 | 70 | 704 | — | 0 |
| 2,0 Mbps, 1% | produto | 2 | 471 | 725 | 471 | 9 | 0 | 0,0 | 100 | 100 | 60 | 61 | 0,0 | 499 | 620 | — | 0 |
| 2,0 Mbps, 1% | produto | 3 | 493 | 1172 | 493 | 10 | 0 | 0,0 | 100 | 100 | 61 | 68 | 0,0 | 319 | 735 | — | 0 |
| 2,0 Mbps, 1% | fixo | 1 | 371 | 768 | 371 | 11 | 0 | 0,0 | 100 | 100 | 60 | 78 | 0,0 | 66 | 800 | — | 0 |
| 2,0 Mbps, 1% | fixo | 2 | 518 | 839 | 518 | 9 | 0 | 0,0 | 100 | 100 | 60 | 60 | 0,0 | 572 | 670 | — | 0 |
| 2,0 Mbps, 1% | fixo | 3 | 595 | 595 | 595 | 9 | 0 | 0,0 | 100 | 100 | 60 | 81 | 0,0 | 65 | 512 | — | 0 |

### O controle cru

| cenário | codificador | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | aceitos | recusados (fila) | não chegaram | chaves | pedidos de chave | kbps oferecidos | MB no cano | voz tocada % | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | cru | 1 | 32 | 33 | 99 | 299 | 1,5 | 300 | 30,0 | 601 | — | 0 | 1 | — | 1509 | 4,02 | — | 0,00 | 0,00 |
| 0% | cru | 2 | 32 | 33 | 129 | 296 | 1,5 | 296 | 29,9 | 601 | — | 0 | 1 | — | 1509 | 4,02 | — | 0,00 | 0,00 |
| 0% | cru | 3 | 32 | 33 | 95 | 295 | 1,5 | 296 | 30,0 | 601 | — | 0 | 1 | — | 1509 | 4,02 | — | 0,00 | 0,00 |
| 0,3% | cru | 1 | 32 | 95 | 133 | 297 | 1,5 | 298 | 30,0 | 601 | — | 0 | 1 | — | 1509 | 4,02 | — | 0,19 | 0,00 |
| 0,3% | cru | 2 | 33 | 208 | 316 | 386 | 1,8 | 355 | 30,0 | 601 | — | 0 | 1 | — | 1509 | 4,02 | — | 0,37 | 0,00 |
| 0,3% | cru | 3 | 33 | 125 | 166 | 295 | 1,5 | 296 | 29,9 | 601 | — | 0 | 1 | — | 1509 | 4,02 | — | 0,42 | 0,00 |
| 1% | cru | 1 | 122 | 682 | 907 | 937 | 1,5 | 299 | 29,7 | 595 | — | 0 | 1 | — | 1494 | 3,98 | — | 0,77 | 0,00 |
| 1% | cru | 2 | 265 | 625 | 798 | 811 | 1,8 | 364 | 29,2 | 598 | — | 0 | 1 | — | 1501 | 3,92 | — | 0,92 | 0,00 |
| 1% | cru | 3 | 431 | 1275 | 1516 | 1547 | 7,5 | 1068 | 27,2 | 565 | — | 0 | 1 | — | 1420 | 3,66 | — | 1,07 | 0,00 |
| 3% | cru | 1 | 1314 | 2256 | 2387 | 2445 | 13,7 | 1193 | 16,6 | 352 | — | 0 | 1 | — | 896 | 2,26 | — | 3,12 | 0,00 |
| 3% | cru | 2 | 1366 | 2377 | 2449 | 2488 | 19,4 | 1197 | 14,6 | 318 | — | 0 | 1 | — | 812 | 2,03 | — | 3,55 | 0,00 |
| 3% | cru | 3 | 1190 | 2081 | 2315 | 2349 | 15,7 | 1133 | 17,3 | 359 | — | 0 | 1 | — | 913 | 2,36 | — | 2,45 | 0,00 |
| 5% | cru | 1 | 2305 | 5672 | 6284 | 6411 | 49,3 | 3071 | 8,3 | 188 | — | 0 | 1 | — | 493 | 1,19 | — | 5,71 | 0,00 |
| 5% | cru | 2 | 2129 | 5215 | 5415 | 5481 | 52,7 | 3073 | 8,2 | 186 | — | 1 | 1 | — | 488 | 1,17 | — | 5,62 | 0,00 |
| 5% | cru | 3 | 1937 | 2950 | 3469 | 3602 | 29,8 | 2080 | 10,9 | 239 | — | 0 | 1 | — | 618 | 1,53 | — | 4,69 | 0,00 |
| 1,0 Mbps, 0% | cru | 1 | 1157 | 2450 | 3032 | 3119 | 19,5 | 2111 | 17,2 | 365 | — | 0 | 1 | — | 928 | 2,35 | — | 0,00 | 1,81 |
| 1,0 Mbps, 0% | cru | 2 | 1156 | 2613 | 3143 | 3243 | 21,3 | 1439 | 17,2 | 366 | — | 0 | 1 | — | 931 | 2,35 | — | 0,00 | 1,60 |
| 1,0 Mbps, 0% | cru | 3 | 1155 | 2450 | 3035 | 3137 | 21,9 | 2121 | 17,2 | 366 | — | 0 | 1 | — | 931 | 2,35 | — | 0,00 | 1,81 |
| 2,0 Mbps, 1% | cru | 1 | 424 | 779 | 835 | 922 | 3,4 | 476 | 28,6 | 592 | — | 0 | 1 | — | 1486 | 3,83 | — | 0,80 | 0,00 |
| 2,0 Mbps, 1% | cru | 2 | 655 | 1064 | 1262 | 1301 | 2,5 | 502 | 27,4 | 554 | — | 0 | 1 | — | 1393 | 3,67 | — | 0,87 | 0,00 |
| 2,0 Mbps, 1% | cru | 3 | 612 | 1463 | 1727 | 1796 | 8,1 | 1072 | 25,1 | 523 | — | 0 | 1 | — | 1317 | 3,41 | — | 1,02 | 0,00 |

### O regime (do segundo 40 ao 60): o que quem assiste vê

| cenário | codificador | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | aceitos | recusados (fila) | não chegaram | chaves | pedidos de chave | kbps oferecidos | MB no cano | voz tocada % | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 1 | 3089 | 3619 | 3714 | 3714 | 96,6 | 333 | 4,0 | 80 | 338 | 2 | 61 | 0 | 8775 | 23,33 | 100,7 | 0,00 | 0,00 |
| 0% | produto | 2 | 3351 | 3458 | 3482 | 3482 | 98,2 | 387 | 3,8 | 76 | 348 | 0 | 59 | 0 | 8646 | 23,62 | 100,6 | 0,00 | 0,00 |
| 0% | produto | 3 | 3126 | 3390 | 3450 | 3450 | 97,9 | 388 | 4,0 | 80 | 340 | 1 | 61 | 0 | 8964 | 23,69 | 100,6 | 0,00 | 0,00 |
| 0,3% | produto | 1 | 32 | 33 | 34 | 111 | 0,0 | 112 | 30,1 | 601 | 0 | 0 | 0 | 0 | 373 | 1,27 | 100,7 | 0,10 | 0,00 |
| 0,3% | produto | 2 | 32 | 62 | 120 | 173 | 0,0 | 176 | 29,4 | 589 | 0 | 0 | 4 | 0 | 507 | 1,61 | 100,7 | 0,29 | 0,00 |
| 0,3% | produto | 3 | 32 | 57 | 98 | 130 | 0,0 | 132 | 30,1 | 601 | 0 | 0 | 0 | 0 | 438 | 1,43 | 100,7 | 0,31 | 0,00 |
| 1% | produto | 1 | 32 | 111 | 152 | 259 | 1,3 | 261 | 29,8 | 595 | 0 | 0 | 2 | 0 | 369 | 1,26 | 100,7 | 0,96 | 0,00 |
| 1% | produto | 2 | 32 | 144 | 908 | 1107 | 5,5 | 1108 | 30,1 | 601 | 0 | 0 | 0 | 0 | 172 | 0,72 | 100,5 | 1,21 | 0,00 |
| 1% | produto | 3 | — | — | — | — | 100,5 | 20107 | 0,0 | 601 | 0 | 601 | 0 | 0 | 170 | 0,72 | 100,5 | 1,11 | 0,00 |
| 3% | produto | 1 | 32 | 898 | 1098 | 1296 | 21,6 | 1099 | 30,1 | 601 | 0 | 0 | 0 | 0 | 170 | 0,65 | 100,5 | 3,48 | 0,00 |
| 3% | produto | 2 | 32 | 642 | 1042 | 1179 | 13,2 | 1077 | 30,1 | 601 | 0 | 0 | 0 | 0 | 170 | 0,65 | 100,5 | 3,06 | 0,00 |
| 3% | produto | 3 | 32 | 246 | 913 | 1111 | 2,0 | 390 | 31,6 | 601 | 0 | 0 | 0 | 0 | 170 | 0,69 | 100,5 | 3,06 | 0,00 |
| 5% | produto | 1 | 33 | 941 | 1096 | 1176 | 29,0 | 1100 | 29,9 | 601 | 0 | 0 | 0 | 0 | 170 | 0,67 | 100,5 | 5,80 | 0,00 |
| 5% | produto | 2 | 33 | 888 | 1087 | 1187 | 21,8 | 1098 | 30,1 | 601 | 0 | 0 | 0 | 0 | 170 | 0,66 | 100,2 | 5,41 | 0,00 |
| 5% | produto | 3 | — | — | — | — | 100,5 | 20106 | 0,0 | 601 | 0 | 601 | 0 | 0 | 170 | 0,66 | 100,5 | 4,98 | 0,00 |
| 1,0 Mbps, 0% | produto | 1 | 50 | 188 | 204 | 326 | 3,4 | 256 | 29,2 | 583 | 0 | 0 | 6 | 0 | 532 | 1,69 | 100,0 | 0,00 | 0,00 |
| 1,0 Mbps, 0% | produto | 2 | 48 | 182 | 209 | 330 | 3,5 | 259 | 29,2 | 583 | 0 | 0 | 6 | 0 | 533 | 1,69 | 100,2 | 0,00 | 0,00 |
| 1,0 Mbps, 0% | produto | 3 | 49 | 57 | 128 | 245 | 1,2 | 232 | 29,8 | 595 | 0 | 0 | 2 | 0 | 475 | 1,53 | 100,3 | 0,00 | 0,00 |
| 2,0 Mbps, 1% | produto | 1 | 36 | 99 | 134 | 137 | 0,0 | 135 | 29,9 | 598 | 0 | 0 | 1 | 0 | 250 | 0,93 | 100,5 | 1,16 | 0,00 |
| 2,0 Mbps, 1% | produto | 2 | 35 | 65 | 134 | 176 | 0,0 | 175 | 29,8 | 595 | 0 | 0 | 2 | 0 | 184 | 0,76 | 100,6 | 1,21 | 0,00 |
| 2,0 Mbps, 1% | produto | 3 | 36 | 93 | 135 | 141 | 0,0 | 139 | 29,8 | 595 | 0 | 0 | 2 | 0 | 295 | 1,05 | 100,5 | 1,04 | 0,00 |

### O regime (do segundo 40 ao 60): o que o controle de taxa fez

| cenário | codificador | rodada | orç. mín kbps | orç. mediana kbps | orç. final kbps | avisos de orçamento | pausas | parado % | sinal servidor mín | sinal servidor med. | sinal local mín | sinal local med. | fora de Nominal % | rtt p50 ms | rtt máx ms | perda medida máx % | reaberturas em quem assiste |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | produto | 1 | 12217 | 12217 | 12217 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 66 | — | 0 |
| 0% | produto | 2 | 12480 | 12480 | 12480 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 64 | — | 0 |
| 0% | produto | 3 | 12480 | 12480 | 12480 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 66 | — | 0 |
| 0,3% | produto | 1 | 438 | 438 | 438 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 63 | 64 | — | 0 |
| 0,3% | produto | 2 | 537 | 537 | 910 | 4 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 64 | 64 | — | 0 |
| 0,3% | produto | 3 | 514 | 514 | 857 | 1 | 0 | 0,0 | 100 | 100 | 97 | 98 | 0,0 | 62 | 65 | — | 0 |
| 1% | produto | 1 | 403 | 403 | 482 | 2 | 0 | 0,0 | 100 | 100 | 70 | 97 | 0,0 | 64 | 354 | — | 0 |
| 1% | produto | 2 | 202 | 202 | 202 | 0 | 0 | 0,0 | 100 | 100 | 97 | 98 | 0,0 | 62 | 63 | — | 0 |
| 1% | produto | 3 | 200 | 200 | 200 | 0 | 0 | 0,0 | 100 | 100 | 76 | 97 | 0,0 | 62 | 262 | — | 0 |
| 3% | produto | 1 | 200 | 200 | 200 | 0 | 0 | 0,0 | 100 | 100 | 61 | 97 | 0,0 | 63 | 465 | — | 0 |
| 3% | produto | 2 | 200 | 200 | 200 | 0 | 0 | 0,0 | 100 | 100 | 82 | 97 | 0,0 | 64 | 205 | — | 0 |
| 3% | produto | 3 | 200 | 200 | 200 | 0 | 0 | 0,0 | 100 | 100 | 97 | 98 | 0,0 | 63 | 64 | — | 0 |
| 5% | produto | 1 | 200 | 200 | 200 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 62 | 64 | — | 0 |
| 5% | produto | 2 | 200 | 200 | 200 | 0 | 0 | 0,0 | 100 | 100 | 84 | 96 | 0,0 | 63 | 157 | — | 0 |
| 5% | produto | 3 | 200 | 200 | 200 | 0 | 0 | 0,0 | 100 | 100 | 97 | 97 | 0,0 | 62 | 64 | — | 0 |
| 1,0 Mbps, 0% | produto | 1 | 471 | 610 | 610 | 6 | 0 | 0,0 | 100 | 100 | 81 | 97 | 0,0 | 67 | 214 | — | 0 |
| 1,0 Mbps, 0% | produto | 2 | 500 | 500 | 673 | 6 | 0 | 0,0 | 100 | 100 | 87 | 96 | 0,0 | 74 | 170 | — | 0 |
| 1,0 Mbps, 0% | produto | 3 | 505 | 560 | 560 | 2 | 0 | 0,0 | 100 | 100 | 96 | 96 | 0,0 | 74 | 78 | — | 0 |
| 2,0 Mbps, 1% | produto | 1 | 277 | 277 | 277 | 1 | 0 | 0,0 | 100 | 100 | 73 | 97 | 0,0 | 63 | 283 | — | 0 |
| 2,0 Mbps, 1% | produto | 2 | 200 | 200 | 200 | 2 | 0 | 0,0 | 100 | 100 | 87 | 97 | 0,0 | 64 | 156 | — | 0 |
| 2,0 Mbps, 1% | produto | 3 | 261 | 261 | 261 | 2 | 0 | 0,0 | 100 | 100 | 96 | 97 | 0,0 | 64 | 80 | — | 0 |

## Adendo: a mesma medida depois de três consertos

Medido no mesmo dia, na mesma máquina, com a árvore que tem os três consertos que a medida acima pediu (carga média 2,84 no começo, 1,94 entre a grade e o regime, 1,02 no fim; uma célula por vez). Só a variante «produto», 3 rodadas de 20 s.

```sh
$B --variante produto
$B --variante produto --aquecimento 40
# a casca de antes do conserto 2, para comparar
$B --variante produto --sem-contrapressao
```

Os três consertos:

1. **A câmera tem teto de banda** (`seele-ffi`, `teto_de_banda_da_camera`): o que a tabela do núcleo diz que a resolução precisa a 30 quadros, 2,79 Mbps em 720p. Antes o orçamento seguia a sonda até 12,48 Mbps.
2. **A casca deixa de codificar com a fila do núcleo pela metade** (`camera.js`, `filaDoNucleoCheia`; o `quadro_da_camera` passou a devolver quantos quadros esperam), e um quadro que não foi codificado não pede quadro-chave. Antes ela só sabia da fila pela recusa, que custa uma chave. O codificador de mentira do exemplo espelha isso; `--sem-contrapressao` é a casca de antes.
3. **O espectador cortado pelo servidor volta no próximo quadro-chave** (`voice_room.rs`, `VOLTA_DO_CORTADO`), uma vez a cada 10 s. Antes o corte era até o fim da transmissão.

A variação «recusa sem chave» da medida acima **não é um conserto**: com H.264 de verdade o quadro codificado que se perde era referência do seguinte. O que ela media era o custo da chave, e o conserto 2 tira a recusa em vez de tirar a chave.

### Primeiros 20 s de uma primeira transmissão (mediana de 3 rodadas)

| cenário | antes: lat p50 / congelado % / quadros/s | depois | depois: p95 | depois: recusados | orçamento mediana kbps (antes → depois) |
|---|--:|--:|--:|--:|--:|
| 0% | 415 / 55,3 / 14,8 | 32 / 1,5 / 29,9 | 33 | 0 | 12480 → 2790 |
| 0,3% | 33 / 22,6 / 20,8 | 34 / 1,5 / 29,3 | 710 | 0 | 1500 → 618–2286 |
| 1% | 33 / 42,6 / 16,1 | 93 / 9,0 / 25,7 | 1520 | 0 | 746 → 608–940 |
| 3% | 4466 / 89,1 / 1,5 | 1613 / 34,8 / 19,4 | 3488 | 10 | 1463 → 506–936 |
| 5% | 6282 / 96,5 / 1,4 | 2888 / 71,7 / 7,2 | 8595 | 218 | 1463 → 936–1170 |
| 1,0 Mbps, 0% | 283 / 63,3 / 8,9 | 48 / 55,9 / 13,3 | 4534 | 116 | 1170 → 484–1170 |
| 2,0 Mbps, 1% | 42 / 50,9 / 15,0 | 89 / 11,1 / 24,6 | 1825 | 0 | 725 → 560–689 |

A coluna do orçamento «depois» é a faixa da mediana entre as três rodadas.

### Do segundo 40 ao 60 (mediana de 3 rodadas)

| cenário | antes: lat p50 / congelado % / quadros/s | depois | depois: p95 | orçamento mediana kbps, depois |
|---|--:|--:|--:|--:|
| 0% | 3126 / 97,9 / 4,0 | 33 / 0,0 / 30,1 | 34 | 2790 |
| 0,3% | 32 / 0,0 / 30,1 | 32 / 1,4 / 29,4 | 73 | 248–553 |
| 1% | 32 / 5,5 / 29,8 | 32 / 1,1 / 29,9 | 90 | 200–279 |
| 3% | 32 / 13,2 / 30,1 | 32 / 20,7 / 30,1 | 2197 | 200 |
| 5% | 33 / 29,0 / 29,9 | 118 / 31,2 / 30,1 | 2182 | 200 |
| 1,0 Mbps, 0% | 49 / 3,4 / 29,2 | 48 / 3,5 / 29,1 | 187 | 480–608 |
| 2,0 Mbps, 1% | 36 / 0,0 / 29,8 | 35 / 1,0 / 29,8 | 101 | 200–455 |

### O que o adendo mostra

1. **Sem perda a câmera deixou de se estragar sozinha**: de 55% congelado nos primeiros 20 s e 98% em regime para 1,5% e 0%, a 30 quadros/s e 32 ms.
2. **Até 1% de perda a partida melhorou muito e ainda não é boa**: o tempo congelado caiu de 23–51% para 1,5–11%, sem nenhum quadro recusado. A cauda continua longa (p95 de 0,7 a 1,8 s): é o SCTP retransmitindo em ordem.
3. **Em regime, até 1% de perda e nas duas bandas estreitas a tela passa** no critério da medida (p50 < 300 ms e < 5% congelado) — ao preço de sempre: o orçamento desce a 200–600 kbps e não volta a subir.
4. **A 3% e 5% não passa, nem em regime** (21% e 31% congelado, p95 de 2,2 s), e a partida a 1,0 Mbps continua ruim (56% congelado nos primeiros 20 s, enquanto a sonda desce de 5,58 Mbps). Nenhum dos três consertos mexe na sonda nem na partida.
5. **O corte do espectador aconteceu duas vezes nas 21 rodadas de regime, e nas duas a imagem voltou**: quem assistia teve duas aberturas e uma fechada, e recebeu quadros até o fim da janela. Antes do conserto, as duas rodadas com corte terminaram com zero quadros.

### O que o adendo NÃO mostra

- Tudo o que a medida acima já não mostrava: codificador de verdade, qualidade da imagem, a tela do sistema, perda em rajada, jitter, atraso da voz.
- **O «antes» e o «depois» não rodaram intercalados**: são duas execuções no mesmo dia, com 3 rodadas cada, e as faixas entre rodadas são largas a partir de 3%.
- O conserto 2 foi medido com o codificador de mentira, que espelha a casca. O `VideoEncoder` de verdade e o tempo do IPC do Tauri não entraram.
- Quanto de cada melhora vem de cada conserto: os três foram medidos juntos.
- O que continua de pé — a faixa de sinal que nunca se mexe, a sonda que lê perda sorteada como congestionamento, e a partida em 5,58 Mbps — não foi tocado.

## Segundo adendo: a sonda e a partida

Medido no mesmo dia, em cima dos três consertos do adendo anterior (carga média 1,76 no começo, 1,49 entre a grade e o regime, 1,57 no fim). Só a variante «produto», 3 rodadas de 20 s.

### O que fazia o orçamento só descer

A sonda foi instrumentada para dizer, janela a janela, o que viu (a instrumentação não ficou no código). Uma rodada de 60 s a 1% de perda e outra a 3%:

- **O começo é fila de verdade**: cinco a oito janelas seguidas com **todas** as respostas do `Ping` acima da tolerância (140 a 510 ms contra 62 ms de base).
- **Depois disso, o que derruba a estimativa são eventos de uma janela só**, entre janelas de 62–64 ms: um `Ping` em voo há 125, 131 ou 400 ms no fechamento da janela, ou uma resposta única de 109 ms. A 1% foram quatro em 46 s, e a estimativa foi de 1,71 Mbps a 405 kbps. A 3% foram dois em 28 s, e o teto parou em 588 kbps com a ida e volta em 62 ms durante os 30 s seguintes.
- É o `Ping` da própria sonda, num canal confiável, sendo o pacote que o caminho perdeu.
- **A partida**: a estimativa nascia em 9,3 Mbps com o teto preso em 2,79 Mbps pela escolha. As quatro primeiras janelas de fila recuavam a estimativa 20% cada uma sem mover o teto.

### Os dois consertos

4. **Um `Ping` atrasado sozinho é suspeita, e não fila** (`caminho.rs`, `DorDoPing`). A janela conta a **menor** resposta que recebeu. Duas respostas ou mais, todas atrasadas: fila, na hora. Uma resposta só atrasada, ou um `Ping` em voo além da tolerância: a sonda não sobe nem desce, e a janela seguinte decide.
5. **A sonda parte do caminho que a banda escolhida usa** (`video.rs`, `semente_da_sonda_bps`): 4,65 Mbps para a câmera, em vez de 9,3.

### Primeiros 20 s (mediana de 3 rodadas; «antes» é o adendo anterior)

| cenário | antes: lat p50 / congelado % / quadros/s | depois | depois: p95 | orçamento mediana kbps (antes → depois) |
|---|--:|--:|--:|--:|
| 0% | 32 / 1,5 / 29,9 | 32 / 1,5 / 29,9 | 33 | 2790 → 2790 |
| 0,3% | 34 / 1,5 / 29,3 | 36 / 4,7 / 29,3 | 845 | 618–2286 → 1369–1416 |
| 1% | 93 / 9,0 / 25,7 | 62 / 5,0 / 27,9 | 1090 | 608–940 → 576–740 |
| 3% | 1613 / 34,8 / 19,4 | 378 / 34,0 / 20,1 | 3691 | 506–936 → 265–493 |
| 5% | 2888 / 71,7 / 7,2 | 2756 / 69,0 / 8,7 | 7178 | 936–1170 → 385–731 |
| 1,0 Mbps, 0% | 48 / 55,9 / 13,3 | 48 / 29,9 / 20,4 | 3477 | 484–1170 → 490–523 |
| 2,0 Mbps, 1% | 89 / 11,1 / 24,6 | 86 / 8,9 / 26,4 | 1611 | 560–689 → 602–739 |

### Do segundo 40 ao 60 (mediana de 3 rodadas)

| cenário | antes: lat p50 / congelado % / quadros/s | depois | depois: p95 | orçamento mediana kbps (antes → depois) |
|---|--:|--:|--:|--:|
| 0% | 33 / 0,0 / 30,1 | 32 / 0,0 / 30,1 | 33 | 2790 → 2790 |
| 0,3% | 32 / 1,4 / 29,4 | 32 / 0,0 / 29,9 | 117 | 248–553 → 519–1458 |
| 1% | 32 / 1,1 / 29,9 | 32 / 0,0 / 29,5 | 216 | 200–279 → 585–640 |
| 3% | 32 / 20,7 / 30,1 | 32 / 24,8 / 29,9 | 2076 | 200 → 234–268 |
| 5% | 118 / 31,2 / 30,1 | 129 / 46,7 / 30,6 | 1028 | 200 → 200 |
| 1,0 Mbps, 0% | 48 / 3,5 / 29,1 | 51 / 6,5 / 29,1 | 365 | 480–608 → 529–626 |
| 2,0 Mbps, 1% | 35 / 1,0 / 29,8 | 41 / 3,2 / 29,4 | 312 | 200–455 → 466–562 |

### O que o segundo adendo mostra

1. **Até 1% de perda o orçamento em regime dobrou ou triplicou sem custo em atraso**: de 200–279 para 585–640 kbps a 1%, e de 248–553 para 519–1458 kbps a 0,3%, com 0% do tempo congelado nas duas.
2. **A partida num gargalo de 1 Mbps melhorou e continua ruim**: de 56% para 30% do tempo congelado nos primeiros 20 s. A estimativa ainda sobe uma janela antes da primeira dor, e a primeira dor ainda move pouco.
3. **A 3% e a 5% nada mudou, e o orçamento não é mais o que segura**: com o teto no piso ou perto dele (200–268 kbps) a tela continua 25% e 47% do tempo congelada, com cauda de 1 a 2 s. Nesses dois cenários a medida anterior do canal já dizia o mesmo com outro emissor: sobre SCTP, o que limita é a retransmissão em ordem e a janela de congestionamento.
4. **As bandas estreitas em regime ficaram dentro do ruído**: 3,5% → 6,5% a 1,0 Mbps (faixa de 1,2 a 13,1 entre as rodadas) e 1,0% → 3,2% a 2,0 Mbps com 1%. Três rodadas não separam isso de piora; se há, é o preço de reagir um segundo mais tarde a uma fila que começa numa janela de uma resposta só.
5. **O corte do espectador aconteceu em 4 das 42 rodadas, e nas 4 a imagem voltou.**

### O que o segundo adendo NÃO mostra

- O que os anteriores já não mostravam (codificador de verdade, qualidade da imagem, tela do sistema, rajada, jitter, voz).
- A instrumentação janela a janela foi **uma** rodada de 60 s por cenário, em dois cenários. A contagem dos eventos de uma janela só não tem faixa.
- Os dois consertos foram medidos juntos, e o «antes» é a execução do adendo anterior, não uma execução intercalada.
- A regra da suspeita num caminho com jitter de verdade (Wi-Fi), onde uma resposta atrasada sozinha é comum sem perda nenhuma. Pela regra ela deixa de derrubar a estimativa; se isso é bom ou ruim lá não foi medido.
- A 3% e 5% o que resta é do transporte. RTP não foi medido.
