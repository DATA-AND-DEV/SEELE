# A tela sob perda, medida sobre o SCTP do enlace

**Data:** 2026-11-10
**Árvore:** `entrega/1.0`, commit `978253c9` mais mudanças não commitadas (o produto não foi tocado por esta medida)
**O que é:** uma medida, não uma decisão. A tela foi posta em canal confiável por uma medida de agosto feita sobre QUIC (`docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`, §3.1). O transporte hoje é SCTP sobre DTLS (`str0m` 0.24.1, `sctp-proto` 0.10.7), e ninguém tinha remedido.

## Como reproduzir

```sh
# a grade inteira: 7 cenários × 5 transportes × 3 rodadas de 20 s, ~8 min
cargo run --release -p seele-enlace --example tela_sob_perda

# a mesma grade com os fragmentos a 2 Mbps em vez de 4 Mbps
cargo run --release -p seele-enlace --example tela_sob_perda -- --ritmo-bps 2000000

# conferência: uma célula de cada vez, em vez das cinco de um cenário juntas
cargo run --release -p seele-enlace --example tela_sob_perda -- --em-serie --rodadas 1 --so "1%"

# o emulador
cargo test -p seele-enlace --test cano
```

Semente `20261110` (rodada *n* usa semente + *n* − 1; as cinco células de um cenário usam a mesma). O código é `crates/seele-enlace/examples/tela_sob_perda.rs`; o emulador é o `Cano` de `crates/seele-enlace/src/teste.rs`.

## A máquina

Apple M5 Pro, 15 núcleos, 24 GB, macOS 27.2 (26B123), rustc 1.97.1, perfil `release`. Tudo em 127.0.0.1, num processo só. Carga média da máquina (`uptime`) no começo e no fim da grade principal: 6,71 e 8,39 — outras compilações rodavam na mesma máquina, inclusive, nos três primeiros minutos (a primeira rodada), as deste próprio trabalho. A conferência em série, feita com a máquina mais quieta (carga 2,3), deu os mesmos números (abaixo).

## O que foi medido

**A rede** (o `Cano`): um repasse UDP entre quem liga e o anfitrião. Em cada sentido: perda independente por datagrama com semente fixa, 30 ms de atraso fixo, sem jitter, e — nos dois cenários de banda — um gargalo com fila de 250 ms (31 250 bytes a 1,0 Mbps, 62 500 a 2,0 Mbps) e descarte pela cauda. O aperto (ICE, DTLS, SCTP) passa pelo cano também.

**A carga**: vídeo sintético de 30 quadros/s, deltas de 6 KiB (≈1,47 Mbps), uma chave de 80 KiB no começo e a cada pedido; e uma voz de 50 pacotes/s de 120 bytes no canal `voz` de verdade (`Conexao::send_datagram`). Os dois de quem liga para o anfitrião.

**Os transportes do vídeo**:

- **A** — o de hoje: canal `u`, confiável e ordenado, `open_uni` + `write_all` de um quadro por vez. O emissor captura quando o quadro anterior acabou de ser escrito; se o crédito segura a escrita, os quadros do intervalo não são gerados (coluna «gerados»).
- **B** — canal `j` (sem ordem, 0 retransmissões), quadro em fragmentos de até 1100 bytes (21 de cabeçalho; delta = 6 fragmentos, chave = 76). Quadro com fragmento faltando está perdido; delta só é exibível com a cadeia inteira desde a chave. Quebrada a cadeia, o receptor pede chave pelo `ctl` e repete a cada 300 ms. Os fragmentos saem num ritmo de 4 Mbps (rajada de até 20 ms).
- **B2** — deltas como em B; a chave vai por um canal `u` confiável, em fatias de 4 KiB no mesmo ritmo.
- **teto do j** — só deltas no `j`, e todo quadro completo conta como exibível: sem cadeia, sem chave, sem pedido. Não é um desenho; é o que o canal `j` entrega com esta carga, e portanto o máximo de qualquer desenho sobre ele.
- **voz** — só a voz, sem vídeo: a referência.

**As métricas**: latência de quadro = da captura até o quadro completo e exibível no receptor (ms). «Congelado %» = soma dos intervalos de mais de 200 ms sem quadro novo exibível, sobre os 20 s. «Maior intervalo» = o maior intervalo sem quadro novo (ms). «MB UDP» = bytes UDP que saíram de quem liga. «j desc. emissor» = fragmentos que o próprio `j` descartou antes do fio (`Contadores::tunel_descartado`: o SCTP já tinha `LIMIAR_DE_VOLUME` = 64 KiB esperando). Voz: perda (não chegou até 1 s depois do fim) e atraso (ms). «cano perda/fila %» = o que o cano tirou na ida, por sorteio e por fila cheia.

## A tabela: mediana de 3 rodadas de 20 s, ritmo de 4 Mbps

| cenário | transporte | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | chaves | pedidos | MB UDP | j desc. emissor | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | voz | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | A | 32 | 33 | 167 | 313 | 1,6 | 314 | 30,0 | 600 | 1 | 0 | 4,21 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | B | 33 | 34 | 75 | 179 | 2,6 | 512 | 29,5 | 601 | 2 | 1 | 4,53 | 36 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | B2 | 32 | 34 | 187 | 315 | 2,9 | 315 | 29,6 | 601 | 2 | 1 | 4,52 | 30 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | teto do j | 33 | 34 | 48 | 124 | 0,0 | 95 | 30,0 | 601 | 0 | 0 | 4,40 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 1% | voz | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,90 | 31 | 32 | 1,02 | 0,00 |
| 1% | A | 437 | 848 | 1013 | 1046 | 3,1 | 616 | 28,0 | 581 | 1 | 0 | 3,97 | 0 | 0,80 | 49 | 90 | 0,96 | 0,00 |
| 1% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 29 | 58 | 3,40 | 2849 | 1,00 | 417 | 741 | 0,98 | 0,00 |
| 1% | B2 | 773 | 1413 | 1761 | 1761 | 99,7 | 1060 | 2,4 | 601 | 34 | 59 | 4,22 | 2394 | 0,70 | 58 | 112 | 0,91 | 0,00 |
| 1% | teto do j | 319 | 643 | 773 | 837 | 12,2 | 442 | 15,6 | 601 | 0 | 0 | 3,35 | 869 | 1,10 | 373 | 724 | 0,96 | 0,00 |
| 3% | voz | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 3,00 | 31 | 32 | 3,16 | 0,00 |
| 3% | A | 1528 | 2505 | 2657 | 2761 | 17,6 | 1126 | 14,2 | 305 | 1 | 0 | 2,19 | 0 | 2,90 | 60 | 133 | 2,90 | 0,00 |
| 3% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 18 | 60 | 1,92 | 3328 | 2,30 | 819 | 1576 | 3,01 | 0,00 |
| 3% | B2 | 2048 | 3571 | 3571 | 3571 | 100,0 | 2138 | 0,7 | 601 | 17 | 65 | 2,10 | 3045 | 3,40 | 66 | 212 | 2,95 | 0,00 |
| 3% | teto do j | 637 | 1076 | 1894 | 1894 | 78,9 | 2067 | 3,8 | 601 | 0 | 0 | 1,83 | 2189 | 1,90 | 823 | 1749 | 3,08 | 0,00 |
| 5% | voz | — | — | — | — | — | — | — | — | — | — | 0,20 | — | 5,20 | 31 | 32 | 5,51 | 0,00 |
| 5% | A | 2518 | 3629 | 3783 | 3818 | 43,5 | 2068 | 8,4 | 192 | 1 | 0 | 1,45 | 0 | 5,89 | 69 | 284 | 5,55 | 0,00 |
| 5% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 14 | 63 | 1,45 | 3422 | 4,60 | 1033 | 3734 | 5,05 | 0,00 |
| 5% | B2 | 2490 | 4280 | 4280 | 4280 | 100,0 | 3471 | 0,5 | 600 | 12 | 67 | 1,49 | 3255 | 6,10 | 63 | 329 | 5,56 | 0,00 |
| 5% | teto do j | 776 | 1694 | 1787 | 1787 | 92,3 | 8527 | 1,3 | 601 | 0 | 0 | 1,32 | 2620 | 4,50 | 1083 | 2122 | 5,03 | 0,00 |
| 10% | voz | — | — | — | — | — | — | — | — | — | — | 0,21 | — | 9,50 | 31 | 32 | 10,48 | 0,00 |
| 10% | A | 6088 | 11615 | 11626 | 11626 | 81,7 | 3422 | 2,7 | 78 | 1 | 0 | 0,68 | 0 | 8,60 | 148 | 2392 | 10,58 | 0,00 |
| 10% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 7 | 61 | 0,78 | 3522 | 14,40 | 2288 | 5540 | 10,36 | 0,00 |
| 10% | B2 | 7475 | 9832 | 9832 | 9832 | 100,0 | 8344 | 0,2 | 601 | 6 | 67 | 0,74 | 3475 | 10,10 | 148 | 2737 | 10,33 | 0,00 |
| 10% | teto do j | 1155 | 2270 | 2270 | 2270 | 100,0 | 17397 | 0,2 | 601 | 0 | 0 | 0,65 | 3132 | 12,40 | 3478 | 5792 | 10,42 | 0,00 |
| 1,0 Mbps, 0% | voz | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,00 | 33 | 34 | 0,00 | 0,00 |
| 1,0 Mbps, 0% | A | 1229 | 2764 | 3085 | 3176 | 24,5 | 2166 | 15,5 | 331 | 1 | 0 | 2,35 | 0 | 0,40 | 208 | 305 | 0,00 | 1,58 |
| 1,0 Mbps, 0% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 19 | 62 | 2,43 | 3454 | 1,90 | 643 | 3161 | 0,00 | 0,49 |
| 1,0 Mbps, 0% | B2 | 1802 | 2421 | 2421 | 2421 | 100,0 | 2233 | 0,9 | 601 | 18 | 66 | 2,46 | 2821 | 0,80 | 201 | 313 | 0,00 | 1,05 |
| 1,0 Mbps, 0% | teto do j | 360 | 1725 | 1725 | 1725 | 94,1 | 10669 | 0,9 | 601 | 0 | 0 | 2,48 | 2048 | 2,20 | 593 | 2264 | 0,00 | 0,49 |
| 2,0 Mbps, 1% | voz | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,90 | 32 | 33 | 1,02 | 0,00 |
| 2,0 Mbps, 1% | A | 719 | 1438 | 1715 | 1724 | 8,1 | 1091 | 24,5 | 504 | 1 | 0 | 3,50 | 0 | 0,90 | 59 | 111 | 0,92 | 0,00 |
| 2,0 Mbps, 1% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 27 | 61 | 3,15 | 2925 | 0,50 | 470 | 793 | 0,96 | 0,00 |
| 2,0 Mbps, 1% | B2 | 1096 | 2095 | 2229 | 2229 | 100,0 | 1778 | 1,4 | 601 | 27 | 62 | 3,21 | 2741 | 0,60 | 56 | 130 | 0,96 | 0,00 |
| 2,0 Mbps, 1% | teto do j | 353 | 721 | 822 | 843 | 41,8 | 1208 | 11,2 | 601 | 0 | 0 | 3,19 | 1036 | 1,00 | 442 | 778 | 0,93 | 0,00 |

### Menor e maior valor entre as 3 rodadas

| cenário | transporte | lat p50 | lat p95 | congelado % | quadros/s | pedidos | voz perda % | voz p95 |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| 0% | voz | — | — | — | — | — | 0,00–0,00 | 32–32 |
| 0% | A | 31–32 | 32–33 | 1,6–1,6 | 29,9–30,0 | 0–0 | 0,00–0,00 | 32–32 |
| 0% | B | 32–33 | 33–34 | 2,6–2,6 | 29,5–29,5 | 1–1 | 0,00–0,00 | 32–32 |
| 0% | B2 | 32–33 | 33–34 | 2,9–2,9 | 29,6–29,6 | 1–1 | 0,00–0,00 | 32–32 |
| 0% | teto do j | 32–33 | 33–34 | 0,0–0,0 | 30,0–30,0 | 0–0 | 0,00–0,00 | 32–32 |
| 1% | voz | — | — | — | — | — | 0,70–1,10 | 32–32 |
| 1% | A | 296–654 | 639–1180 | 2,6–9,7 | 25,1–29,6 | 0–0 | 0,60–1,40 | 90–100 |
| 1% | B | — | — | 100,0–100,0 | 0,0–0,0 | 55–60 | 0,80–1,40 | 730–986 |
| 1% | B2 | 656–1051 | 1368–2194 | 99,2–100,0 | 1,8–3,0 | 57–62 | 0,70–0,90 | 106–147 |
| 1% | teto do j | 312–373 | 579–690 | 11,9–16,0 | 15,2–18,1 | 0–0 | 0,90–1,20 | 649–739 |
| 3% | voz | — | — | — | — | — | 2,90–3,20 | 32–32 |
| 3% | A | 1260–1544 | 2221–2680 | 14,1–26,6 | 13,8–16,2 | 0–0 | 1,80–3,60 | 127–137 |
| 3% | B | — | — | 100,0–100,0 | 0,0–0,0 | 54–65 | 2,00–4,30 | 1528–2249 |
| 3% | B2 | 1880–2161 | 2776–5153 | 100,0–100,0 | 0,7–0,8 | 65–67 | 3,10–3,40 | 198–1394 |
| 3% | teto do j | 577–702 | 888–1084 | 68,1–81,3 | 3,1–4,8 | 0–0 | 1,70–3,70 | 932–2442 |
| 5% | voz | — | — | — | — | — | 5,10–5,40 | 32–32 |
| 5% | A | 2004–2570 | 2916–5037 | 26,9–54,2 | 7,3–10,1 | 0–0 | 4,60–6,00 | 218–1584 |
| 5% | B | — | — | 100,0–100,0 | 0,0–0,0 | 56–63 | 3,40–22,78 | 2673–4682 |
| 5% | B2 | 2436–2918 | 3633–6007 | 99,1–100,0 | 0,5–0,6 | 65–68 | 5,80–6,29 | 206–400 |
| 5% | teto do j | 603–1176 | 1493–2005 | 88,8–97,8 | 0,8–1,6 | 0–0 | 3,80–4,80 | 1699–3566 |
| 10% | voz | — | — | — | — | — | 9,10–10,90 | 32–32 |
| 10% | A | 3759–6325 | 6193–12798 | 66,2–81,9 | 2,7–4,5 | 0–0 | 6,80–11,59 | 1990–3039 |
| 10% | B | — | — | 100,0–100,0 | 0,0–0,0 | 54–63 | 9,70–18,80 | 4601–6953 |
| 10% | B2 | 5009–8615 | 7871–12778 | 99,2–100,0 | 0,1–0,2 | 66–67 | 10,00–11,90 | 2547–4364 |
| 10% | teto do j | 430–1464 | 856–10161 | 98,1–100,0 | 0,1–0,2 | 0–0 | 10,70–36,70 | 3968–10606 |
| 1,0 Mbps, 0% | voz | — | — | — | — | — | 0,00–0,00 | 33–34 |
| 1,0 Mbps, 0% | A | 1228–1229 | 2712–3181 | 21,5–24,6 | 15,1–15,6 | 0–0 | 0,20–0,40 | 291–395 |
| 1,0 Mbps, 0% | B | — | — | 100,0–100,0 | 0,0–0,0 | 61–64 | 1,60–2,70 | 2330–3502 |
| 1,0 Mbps, 0% | B2 | 1788–1811 | 2293–2759 | 100,0–100,0 | 0,9–0,9 | 66–66 | 0,50–1,10 | 308–367 |
| 1,0 Mbps, 0% | teto do j | 310–361 | 731–1767 | 94,0–95,5 | 0,8–1,0 | 0–0 | 1,90–2,30 | 1668–2584 |
| 2,0 Mbps, 1% | voz | — | — | — | — | — | 0,70–1,10 | 33–33 |
| 2,0 Mbps, 1% | A | 670–828 | 1149–1986 | 3,6–12,1 | 21,7–25,6 | 0–0 | 0,80–0,90 | 96–119 |
| 2,0 Mbps, 1% | B | — | — | 100,0–100,0 | 0,0–0,0 | 58–63 | 0,30–0,70 | 720–902 |
| 2,0 Mbps, 1% | B2 | 941–1315 | 1622–2104 | 99,0–100,0 | 1,1–1,4 | 58–63 | 0,50–1,70 | 119–178 |
| 2,0 Mbps, 1% | teto do j | 284–406 | 651–779 | 40,4–50,9 | 7,6–12,0 | 0–0 | 0,90–1,30 | 718–840 |

### Os transportes sobre o `j` com ritmo de 2 Mbps (mediana de 3 rodadas)

| cenário | transporte | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | chaves | pedidos | MB UDP | j desc. emissor | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | B | 37 | 109 | 307 | 347 | 1,7 | 347 | 29,9 | 600 | 1 | 0 | 4,48 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | B2 | 38 | 39 | 316 | 383 | 5,2 | 667 | 29,2 | 601 | 2 | 2 | 4,53 | 20 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | teto do j | 37 | 38 | 56 | 126 | 0,0 | 96 | 29,9 | 600 | 0 | 0 | 4,39 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 1% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 18 | 56 | 3,44 | 1930 | 0,60 | 412 | 721 | 0,95 | 0,00 |
| 1% | B2 | 944 | 1571 | 1936 | 1936 | 100,0 | 2066 | 2,9 | 600 | 28 | 58 | 3,67 | 2303 | 1,30 | 55 | 128 | 0,88 | 0,00 |
| 1% | teto do j | 316 | 634 | 717 | 781 | 12,9 | 500 | 17,5 | 601 | 0 | 0 | 3,48 | 748 | 0,50 | 366 | 673 | 0,96 | 0,00 |
| 3% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 17 | 55 | 1,96 | 3159 | 2,80 | 827 | 1414 | 3,00 | 0,00 |
| 3% | B2 | 2138 | 4439 | 4439 | 4439 | 100,0 | 3458 | 0,7 | 601 | 15 | 65 | 1,95 | 2985 | 3,80 | 70 | 559 | 2,99 | 0,00 |
| 3% | teto do j | 644 | 976 | 1842 | 1842 | 78,1 | 2187 | 3,6 | 600 | 0 | 0 | 1,86 | 2143 | 3,20 | 825 | 1656 | 3,10 | 0,00 |
| 5% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 13 | 55 | 1,37 | 3366 | 5,60 | 1087 | 2435 | 5,28 | 0,00 |
| 5% | B2 | 2836 | 5640 | 5640 | 5640 | 100,0 | 3700 | 0,6 | 601 | 12 | 63 | 1,38 | 3314 | 3,90 | 75 | 1359 | 5,62 | 0,00 |
| 5% | teto do j | 822 | 1513 | 1513 | 1513 | 92,5 | 4302 | 1,4 | 600 | 0 | 0 | 1,44 | 2494 | 5,10 | 1040 | 1863 | 5,14 | 0,00 |
| 10% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 8 | 55 | 0,99 | 3444 | 10,70 | 1619 | 5256 | 10,30 | 0,00 |
| 10% | B2 | 7080 | 8353 | 8353 | 8353 | 100,0 | 7080 | 0,2 | 601 | 7 | 67 | 0,79 | 3475 | 11,70 | 92 | 1647 | 10,37 | 0,00 |
| 10% | teto do j | 2432 | 2923 | 2923 | 2923 | 99,1 | 16710 | 0,1 | 600 | 0 | 0 | 0,80 | 3028 | 20,20 | 2220 | 4424 | 10,52 | 0,00 |
| 1,0 Mbps, 0% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 17 | 56 | 2,25 | 2878 | 1,30 | 593 | 2391 | 0,00 | 1,14 |
| 1,0 Mbps, 0% | B2 | 1820 | 2967 | 2967 | 2967 | 100,0 | 2277 | 0,9 | 600 | 17 | 66 | 2,39 | 2804 | 0,70 | 215 | 328 | 0,00 | 1,09 |
| 1,0 Mbps, 0% | teto do j | 310 | 1984 | 1984 | 1984 | 95,5 | 16150 | 0,8 | 600 | 0 | 0 | 2,30 | 1791 | 1,20 | 585 | 1918 | 0,00 | 1,01 |
| 2,0 Mbps, 1% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 18 | 55 | 3,16 | 2183 | 1,00 | 484 | 803 | 0,94 | 0,00 |
| 2,0 Mbps, 1% | B2 | 1146 | 1693 | 2042 | 2042 | 100,0 | 1515 | 1,6 | 601 | 27 | 52 | 3,29 | 2745 | 1,10 | 54 | 119 | 0,96 | 0,00 |
| 2,0 Mbps, 1% | teto do j | 353 | 671 | 797 | 901 | 43,5 | 1596 | 11,1 | 600 | 0 | 0 | 3,11 | 1093 | 0,70 | 463 | 819 | 0,94 | 0,00 |

### Conferência em série (1 rodada, uma célula por vez)

| cenário | transporte | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | chaves | pedidos | MB UDP | j desc. emissor | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 1% | voz | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,70 | 33 | 34 | 0,75 | 0,00 |
| 1% | A | 461 | 805 | 850 | 873 | 3,2 | 633 | 28,8 | 590 | 1 | 0 | 4,06 | 0 | 1,60 | 52 | 94 | 0,79 | 0,00 |
| 1% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 32 | 59 | 3,65 | 2771 | 0,60 | 373 | 695 | 0,95 | 0,00 |
| 1% | B2 | 724 | 1457 | 2006 | 2006 | 99,8 | 1880 | 2,4 | 601 | 34 | 62 | 4,22 | 2309 | 0,50 | 56 | 129 | 0,83 | 0,00 |
| 1% | teto do j | 312 | 593 | 663 | 967 | 10,4 | 499 | 17,8 | 601 | 0 | 0 | 3,54 | 701 | 0,70 | 334 | 635 | 0,95 | 0,00 |
| 2,0 Mbps, 1% | voz | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,70 | 33 | 34 | 0,75 | 0,00 |
| 2,0 Mbps, 1% | A | 625 | 1118 | 1220 | 1233 | 3,6 | 728 | 25,6 | 534 | 1 | 0 | 3,65 | 0 | 0,60 | 60 | 100 | 0,81 | 0,00 |
| 2,0 Mbps, 1% | B | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 30 | 62 | 3,20 | 3092 | 0,80 | 442 | 772 | 0,93 | 0,00 |
| 2,0 Mbps, 1% | B2 | 1154 | 1720 | 2206 | 2206 | 100,0 | 1446 | 1,4 | 601 | 28 | 60 | 3,39 | 2618 | 1,40 | 56 | 176 | 0,78 | 0,00 |
| 2,0 Mbps, 1% | teto do j | 370 | 681 | 795 | 858 | 36,9 | 1041 | 11,5 | 601 | 0 | 0 | 3,24 | 995 | 0,50 | 410 | 729 | 0,92 | 0,00 |

### Fora da grade: 0,1% e 0,3% de perda (1 rodada de 20 s, A e B)

Medido uma vez, com `--perda 0.001` e `--perda 0.003`, antes de a chave do B2 ir em fatias (o A e o B não mudaram depois).

| perda | transporte | lat p50 | p95 | congelado % | maior intervalo ms | quadros/s | chaves | pedidos | j desc. emissor | voz p95 |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0,1% | A | 31 | 35 | 1,6 | 314 | 30,0 | 1 | 0 | 0 | 32 |
| 0,1% | B | 32 | 260 | 62,3 | 3070 | 13,9 | 38 | 38 | 683 | 245 |
| 0,3% | A | 32 | 115 | 1,6 | 315 | 30,0 | 1 | 0 | 0 | 59 |
| 0,3% | B | 32 | 136 | 83,3 | 15391 | 5,4 | 35 | 46 | 1603 | 454 |

## O que os números mostram

1. **O `j` não escapa do controle de congestionamento do SCTP.** «0 retransmissões» tira a retransmissão, não a janela. A linha «teto do j» — sem nenhum protocolo de vídeo por cima — entrega 15,6 quadros/s a 1% de perda, 3,8 a 3%, 1,3 a 5% e 0,2 a 10%. O canal confiável, na mesma rede, entrega 28,0, 14,2, 8,4 e 2,7. Em nenhuma célula com perda ou com gargalo o `j` entregou mais quadros que o `u`.
2. **O que o `j` não manda, ele descarta no emissor, sem avisar quem escreve.** A 1% de perda o `j` descartou 869 dos ~3600 fragmentos antes do fio; a 3%, 2189. O `mandar` devolve `Ok` nesses casos; só o contador sabe.
3. **O `j` também não é rápido sob perda.** A latência p50 do «teto do j» a 1% é 319 ms (A: 437 ms); a 3%, 637 ms (A: 1528 ms). É menor que a do canal confiável, e está longe dos 32 ms da rede sem perda: os fragmentos esperam no buffer do SCTP até o limiar de 64 KiB.
4. **B, como especificado, não exibiu nenhum quadro em nenhuma célula com perda ou gargalo**, com 4 e com 2 Mbps de ritmo. Uma chave são 76 fragmentos seguidos num canal que descarta no emissor; cada chave pedida aumenta a carga, que aumenta o descarte. A 0,1% de perda (uma rodada) ainda exibiu 13,9 quadros/s com 62% do tempo congelado e 38 chaves em 20 s.
5. **B2 exibe, pouco**: 2,4 quadros/s a 1%, 0,7 a 3%. A chave chega (é confiável), e a cadeia de deltas quebra logo depois.
6. **Sem perda e sem gargalo os três empatam**: 30 quadros/s, p50 de 32–33 ms. O p99 e o «maior intervalo» de ~315 ms em A são a chave inicial de 80 KiB atravessando a partida lenta do SCTP. Em B a 4 Mbps a primeira chave perde 36 fragmentos no emissor pelo mesmo motivo e custa um pedido; a 2 Mbps não perde nenhum.
7. **A voz.** Sozinha, perde o que o caminho perde e chega em 32–34 ms (p95) em todos os cenários. Com vídeo em A, o p95 sobe para 90 ms a 1%, 133 ms a 3%, 284 ms a 5% e 2392 ms a 10%. Com vídeo no `j` (B e teto), sobe muito mais: 741 e 724 ms a 1%, 1576 e 1749 ms a 3%. Com B2 fica perto de A até 5% (112, 212, 329 ms). A perda de voz acompanha a do caminho em A e B2; em B e no teto do j passa dela em algumas rodadas (até 22,8% a 5% e 36,7% a 10%).
8. **O canal confiável sob perda também é ruim, e é o menos ruim dos três.** A 1%: p50 de 437 ms, p95 de 848 ms. A 3%: 14 quadros/s, p50 de 1,5 s, 18% do tempo congelado. A 10% e a 1,0 Mbps sem perda, a tela atrasa segundos.

### Duas coisas lidas no código, que explicam parte disso

- `sctp-proto` 0.10.7, `src/config.rs`: `RTO_INITIAL` = 3000 ms, `RTO_MIN` = 1000 ms, `RTO_MAX` = 60 000 ms, e o `str0m` 0.24.1 não muda esses valores (`src/sctp/mod.rs`, `TransportConfig::default()`). Um estouro do T3 são pelo menos 1 s sem mandar. O teste do emulador registrou o `j` mais de 1,5 s sem entregar nada a 10% de perda.
- `sctp-proto` 0.10.7, `src/queue/pending_queue.rs`: há uma fila para os pedaços sem ordem e outra para os ordenados, e a de sem ordem é olhada primeiro. A voz e o `j` são sem ordem e dividem a mesma fila; o `u` é ordenado e fica atrás da voz. É coerente com a voz sofrer mais com o vídeo no `j` que no `u`. **Isto é leitura de código, não medida isolada.**

## O que os números NÃO mostram

- **Loopback, num processo só.** Não há rede de verdade, nem NAT, nem MTU menor que o esperado, nem reordenação (o cano não troca a ordem, e o receptor de B conta com isso para declarar um quadro perdido).
- **Perda independente, não em rajada.** Perda real costuma vir agrupada; um quadro de 6 fragmentos sofre menos com rajada que com perda espalhada, e uma retransmissão sofre diferente. Não foi medido.
- **Sem congestionamento real.** O gargalo é uma fila de tamanho fixo com um fluxo só; não há tráfego concorrente, e a perda sorteada é tratada pelo SCTP como congestionamento sem ser.
- **Sem codificador real.** Os quadros têm tamanho fixo; não há quadro maior em cena movimentada, nem codificador que baixe a taxa.
- **Nenhum dos emissores tem controle de taxa.** O A daqui não tem o balde de teto do produto nem a chave em fatias de `seele-core/src/tela.rs`; manda 1,47 Mbps contra uma rede que, a 3% de perda, não leva isso. O B e o B2 também não reagem: não leem `tunel_descartado`, não baixam a taxa, não espaçam a chave além do ritmo fixo. Um B com controle de taxa, com chave menor, com FEC ou com outra política de pedido **não foi medido**, e a linha «teto do j» é o único limite que esta medida dá para ele.
- **A política de B é uma entre muitas.** O intervalo de 300 ms entre pedidos, o ritmo de 4 ou 2 Mbps, o fragmento de 1100 bytes e a regra de «um fragmento mais novo chegou, o anterior se perdeu» foram escolhidos aqui e não variados (fora o ritmo).
- **Só um sentido de vídeo, um par, 20 s, 3 rodadas.** As faixas entre rodadas são largas a partir de 3% (p95 de A a 10%: 6,2 a 12,8 s). Os números de 5% e 10% servem para a ordem de grandeza, não para a segunda casa.
- **Um transporte fora do SCTP não foi medido.** Nada aqui diz como o vídeo se comportaria em RTP, ou em qualquer coisa que não passe pela janela do `sctp-proto`.
- **Os contadores do SCTP não saem do `str0m`** (`Leitura::pacotes_perdidos` e `eventos_de_congestionamento` são sempre `None`): a janela, os estouros do T3 e as retransmissões não foram observados diretamente, só os efeitos.

## Apêndice: todas as rodadas (ritmo de 4 Mbps)

| cenário | transporte | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | chaves | pedidos | MB UDP | j desc. emissor | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | voz | 1 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | A | 1 | 31 | 32 | 164 | 312 | 1,6 | 314 | 30,0 | 600 | 1 | 0 | 4,21 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | B | 1 | 32 | 33 | 75 | 179 | 2,6 | 512 | 29,5 | 601 | 2 | 1 | 4,53 | 34 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | B2 | 1 | 32 | 33 | 187 | 313 | 2,9 | 313 | 29,6 | 601 | 2 | 1 | 4,52 | 30 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | teto do j | 1 | 32 | 33 | 48 | 124 | 0,0 | 95 | 30,0 | 601 | 0 | 0 | 4,40 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 1% | voz | 1 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,70 | 31 | 32 | 0,75 | 0,00 |
| 1% | A | 1 | 296 | 639 | 719 | 739 | 3,1 | 616 | 29,6 | 599 | 1 | 0 | 4,18 | 0 | 0,60 | 46 | 90 | 0,78 | 0,00 |
| 1% | B | 1 | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 31 | 55 | 3,51 | 2892 | 1,40 | 375 | 986 | 0,95 | 0,00 |
| 1% | B2 | 1 | 773 | 1413 | 1480 | 1480 | 99,2 | 1001 | 2,4 | 601 | 34 | 62 | 4,22 | 2394 | 0,70 | 58 | 112 | 0,82 | 0,00 |
| 1% | teto do j | 1 | 312 | 579 | 712 | 778 | 11,9 | 375 | 18,1 | 601 | 0 | 0 | 3,56 | 694 | 1,10 | 331 | 649 | 0,95 | 0,00 |
| 3% | voz | 1 | — | — | — | — | — | — | — | — | — | — | 0,20 | — | 3,20 | 31 | 32 | 3,16 | 0,00 |
| 3% | A | 1 | 1528 | 2505 | 2657 | 2761 | 26,6 | 1126 | 14,2 | 305 | 1 | 0 | 2,19 | 0 | 1,80 | 61 | 137 | 2,90 | 0,00 |
| 3% | B | 1 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 18 | 54 | 1,78 | 3431 | 2,00 | 885 | 2249 | 2,92 | 0,00 |
| 3% | B2 | 1 | 2048 | 3571 | 3571 | 3571 | 100,0 | 2138 | 0,7 | 601 | 17 | 65 | 2,10 | 3045 | 3,10 | 66 | 212 | 2,95 | 0,00 |
| 3% | teto do j | 1 | 577 | 1076 | 2714 | 2714 | 81,3 | 3210 | 3,1 | 601 | 0 | 0 | 1,74 | 2261 | 1,90 | 874 | 2442 | 2,90 | 0,00 |
| 5% | voz | 1 | — | — | — | — | — | — | — | — | — | — | 0,20 | — | 5,10 | 31 | 32 | 5,80 | 0,00 |
| 5% | A | 1 | 2570 | 3629 | 3783 | 3818 | 43,5 | 1129 | 8,4 | 192 | 1 | 0 | 1,45 | 0 | 6,00 | 69 | 218 | 5,55 | 0,00 |
| 5% | B | 1 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 14 | 63 | 1,50 | 3388 | 4,60 | 1033 | 2673 | 5,05 | 0,00 |
| 5% | B2 | 1 | 2436 | 4280 | 4280 | 4280 | 100,0 | 3471 | 0,5 | 600 | 12 | 68 | 1,42 | 3329 | 5,80 | 63 | 400 | 5,64 | 0,00 |
| 5% | teto do j | 1 | 603 | 1493 | 1787 | 1787 | 88,8 | 6776 | 1,6 | 600 | 0 | 0 | 1,61 | 2374 | 3,80 | 1019 | 1699 | 5,03 | 0,00 |
| 10% | voz | 1 | — | — | — | — | — | — | — | — | — | — | 0,21 | — | 9,50 | 31 | 32 | 10,48 | 0,00 |
| 10% | A | 1 | 6088 | 12798 | 12861 | 12861 | 81,9 | 3422 | 2,7 | 78 | 1 | 0 | 0,68 | 0 | 11,59 | 148 | 3039 | 10,39 | 0,00 |
| 10% | B | 1 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 7 | 63 | 0,68 | 3557 | 18,80 | 3204 | 6953 | 10,72 | 0,00 |
| 10% | B2 | 1 | 7475 | 7871 | 7871 | 7871 | 99,2 | 5329 | 0,2 | 601 | 6 | 66 | 0,82 | 3447 | 10,00 | 148 | 2547 | 10,47 | 0,00 |
| 10% | teto do j | 1 | 430 | 856 | 856 | 856 | 98,1 | 18777 | 0,2 | 601 | 0 | 0 | 0,65 | 3132 | 36,70 | 3478 | 5792 | 10,67 | 0,00 |
| 1,0 Mbps, 0% | voz | 1 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,00 | 33 | 34 | 0,00 | 0,00 |
| 1,0 Mbps, 0% | A | 1 | 1229 | 2764 | 3084 | 3174 | 21,5 | 2166 | 15,5 | 331 | 1 | 0 | 2,35 | 0 | 0,20 | 208 | 305 | 0,00 | 1,55 |
| 1,0 Mbps, 0% | B | 1 | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 19 | 64 | 2,43 | 3457 | 2,70 | 643 | 3502 | 0,00 | 0,49 |
| 1,0 Mbps, 0% | B2 | 1 | 1802 | 2421 | 2421 | 2421 | 100,0 | 2233 | 0,9 | 601 | 18 | 66 | 2,46 | 2820 | 0,50 | 195 | 308 | 0,00 | 0,94 |
| 1,0 Mbps, 0% | teto do j | 1 | 361 | 1767 | 2536 | 2536 | 94,1 | 9933 | 1,0 | 601 | 0 | 0 | 2,48 | 2110 | 2,30 | 595 | 2584 | 0,00 | 0,49 |
| 2,0 Mbps, 1% | voz | 1 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,70 | 32 | 33 | 0,75 | 0,00 |
| 2,0 Mbps, 1% | A | 1 | 670 | 1149 | 1184 | 1198 | 3,6 | 720 | 25,6 | 531 | 1 | 0 | 3,64 | 0 | 0,90 | 58 | 96 | 0,79 | 0,00 |
| 2,0 Mbps, 1% | B | 1 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 28 | 63 | 3,23 | 2925 | 0,30 | 428 | 720 | 0,92 | 0,00 |
| 2,0 Mbps, 1% | B2 | 1 | 941 | 2104 | 2229 | 2229 | 99,0 | 1778 | 1,4 | 601 | 29 | 62 | 3,43 | 2668 | 0,50 | 56 | 130 | 0,78 | 0,00 |
| 2,0 Mbps, 1% | teto do j | 1 | 353 | 651 | 746 | 789 | 40,4 | 1208 | 11,2 | 601 | 0 | 0 | 3,26 | 986 | 0,90 | 411 | 718 | 0,93 | 0,00 |
| 0% | voz | 2 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | A | 2 | 32 | 33 | 198 | 313 | 1,6 | 314 | 29,9 | 600 | 1 | 0 | 4,21 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | B | 2 | 33 | 34 | 76 | 180 | 2,6 | 513 | 29,5 | 600 | 2 | 1 | 4,52 | 36 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | B2 | 2 | 32 | 34 | 187 | 316 | 2,9 | 316 | 29,6 | 601 | 2 | 1 | 4,52 | 30 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | teto do j | 2 | 33 | 34 | 48 | 123 | 0,0 | 94 | 30,0 | 600 | 0 | 0 | 4,39 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 1% | voz | 2 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 1,10 | 31 | 32 | 1,02 | 0,00 |
| 1% | A | 2 | 437 | 848 | 1013 | 1046 | 2,6 | 313 | 28,0 | 581 | 1 | 0 | 3,97 | 0 | 1,40 | 49 | 90 | 0,96 | 0,00 |
| 1% | B | 2 | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 29 | 60 | 3,40 | 2842 | 1,00 | 463 | 730 | 0,98 | 0,00 |
| 1% | B2 | 2 | 656 | 1368 | 1761 | 1761 | 100,0 | 1060 | 3,0 | 600 | 35 | 59 | 4,23 | 2346 | 0,70 | 52 | 106 | 0,91 | 0,00 |
| 1% | teto do j | 2 | 319 | 643 | 773 | 850 | 16,0 | 565 | 15,6 | 600 | 0 | 0 | 3,35 | 869 | 0,90 | 373 | 739 | 0,96 | 0,00 |
| 3% | voz | 2 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 3,00 | 31 | 32 | 3,21 | 0,00 |
| 3% | A | 2 | 1544 | 2680 | 2818 | 2928 | 17,6 | 1134 | 13,8 | 295 | 1 | 0 | 2,12 | 0 | 3,60 | 60 | 133 | 3,60 | 0,00 |
| 3% | B | 2 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 18 | 65 | 1,92 | 3328 | 4,30 | 817 | 1576 | 3,12 | 0,00 |
| 3% | B2 | 2 | 2161 | 5153 | 5153 | 5153 | 100,0 | 4006 | 0,7 | 601 | 14 | 67 | 1,72 | 3134 | 3,40 | 78 | 1394 | 3,55 | 0,00 |
| 3% | teto do j | 2 | 637 | 1084 | 1894 | 1894 | 78,9 | 2067 | 3,8 | 601 | 0 | 0 | 1,83 | 2189 | 1,70 | 823 | 1749 | 3,13 | 0,00 |
| 5% | voz | 2 | — | — | — | — | — | — | — | — | — | — | 0,20 | — | 5,20 | 31 | 32 | 5,51 | 0,00 |
| 5% | A | 2 | 2518 | 5037 | 5098 | 5099 | 54,2 | 3065 | 7,3 | 168 | 1 | 0 | 1,29 | 0 | 5,89 | 80 | 1584 | 5,71 | 0,00 |
| 5% | B | 2 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 10 | 56 | 0,95 | 3591 | 22,78 | 1412 | 4682 | 5,70 | 0,00 |
| 5% | B2 | 2 | 2918 | 6007 | 6007 | 6007 | 100,0 | 3963 | 0,5 | 601 | 11 | 65 | 1,49 | 3143 | 6,29 | 73 | 329 | 5,56 | 0,00 |
| 5% | teto do j | 2 | 1176 | 1694 | 1694 | 1694 | 97,8 | 8527 | 0,8 | 601 | 0 | 0 | 1,32 | 2620 | 4,80 | 1083 | 2122 | 5,24 | 0,00 |
| 10% | voz | 2 | — | — | — | — | — | — | — | — | — | — | 0,22 | — | 10,90 | 31 | 32 | 10,67 | 0,00 |
| 10% | A | 2 | 3759 | 6193 | 6379 | 6379 | 66,2 | 3190 | 4,5 | 113 | 1 | 0 | 0,94 | 0 | 8,60 | 114 | 1990 | 10,58 | 0,00 |
| 10% | B | 2 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 7 | 54 | 0,80 | 3503 | 9,70 | 2288 | 4601 | 10,36 | 0,00 |
| 10% | B2 | 2 | 8615 | 9832 | 9832 | 9832 | 100,0 | 8344 | 0,1 | 600 | 5 | 67 | 0,60 | 3484 | 11,90 | 926 | 4364 | 10,19 | 0,00 |
| 10% | teto do j | 2 | 1464 | 2270 | 2270 | 2270 | 100,0 | 17397 | 0,2 | 601 | 0 | 0 | 0,88 | 2977 | 12,40 | 2112 | 3968 | 10,42 | 0,00 |
| 1,0 Mbps, 0% | voz | 2 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,00 | 33 | 33 | 0,00 | 0,00 |
| 1,0 Mbps, 0% | A | 2 | 1229 | 2712 | 3085 | 3176 | 24,5 | 2164 | 15,6 | 332 | 1 | 0 | 2,37 | 0 | 0,40 | 186 | 291 | 0,00 | 1,58 |
| 1,0 Mbps, 0% | B | 2 | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 22 | 61 | 2,25 | 3308 | 1,60 | 590 | 2330 | 0,00 | 1,11 |
| 1,0 Mbps, 0% | B2 | 2 | 1788 | 2293 | 2293 | 2293 | 100,0 | 1920 | 0,9 | 600 | 18 | 66 | 2,47 | 2821 | 1,10 | 201 | 313 | 0,00 | 1,05 |
| 1,0 Mbps, 0% | teto do j | 2 | 360 | 731 | 731 | 731 | 94,0 | 10669 | 0,9 | 601 | 0 | 0 | 2,52 | 2048 | 2,20 | 593 | 2264 | 0,00 | 0,49 |
| 2,0 Mbps, 1% | voz | 2 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 1,10 | 32 | 33 | 1,02 | 0,00 |
| 2,0 Mbps, 1% | A | 2 | 719 | 1438 | 1715 | 1724 | 8,1 | 1091 | 24,5 | 504 | 1 | 0 | 3,50 | 0 | 0,80 | 61 | 119 | 0,92 | 0,00 |
| 2,0 Mbps, 1% | B | 2 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 26 | 58 | 3,15 | 2871 | 0,50 | 470 | 793 | 0,96 | 0,00 |
| 2,0 Mbps, 1% | B2 | 2 | 1096 | 1622 | 1930 | 1930 | 100,0 | 1163 | 1,4 | 601 | 27 | 58 | 3,21 | 2741 | 1,70 | 55 | 119 | 0,96 | 0,00 |
| 2,0 Mbps, 1% | teto do j | 2 | 284 | 721 | 822 | 843 | 41,8 | 1333 | 12,0 | 601 | 0 | 0 | 3,19 | 1036 | 1,00 | 442 | 778 | 0,93 | 0,00 |
| 0% | voz | 3 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | A | 3 | 32 | 33 | 167 | 313 | 1,6 | 314 | 30,0 | 600 | 1 | 0 | 4,21 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | B | 3 | 33 | 34 | 74 | 179 | 2,6 | 512 | 29,5 | 601 | 2 | 1 | 4,53 | 36 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | B2 | 3 | 33 | 34 | 188 | 315 | 2,9 | 315 | 29,6 | 601 | 2 | 1 | 4,52 | 30 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 0% | teto do j | 3 | 33 | 34 | 52 | 126 | 0,0 | 96 | 30,0 | 601 | 0 | 0 | 4,40 | 0 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| 1% | voz | 3 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,90 | 31 | 32 | 1,02 | 0,00 |
| 1% | A | 3 | 654 | 1180 | 1409 | 1502 | 9,7 | 1064 | 25,1 | 524 | 1 | 0 | 3,61 | 0 | 0,80 | 59 | 100 | 1,10 | 0,00 |
| 1% | B | 3 | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 29 | 58 | 3,36 | 2849 | 0,80 | 417 | 741 | 1,03 | 0,00 |
| 1% | B2 | 3 | 1051 | 2194 | 2842 | 2842 | 99,7 | 1949 | 1,8 | 601 | 29 | 57 | 3,54 | 2559 | 0,90 | 58 | 147 | 1,03 | 0,00 |
| 1% | teto do j | 3 | 373 | 690 | 779 | 837 | 12,2 | 442 | 15,2 | 601 | 0 | 0 | 3,28 | 986 | 1,20 | 408 | 724 | 1,07 | 0,00 |
| 3% | voz | 3 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 2,90 | 31 | 32 | 2,95 | 0,00 |
| 3% | A | 3 | 1260 | 2221 | 2354 | 2414 | 14,1 | 1063 | 16,2 | 346 | 1 | 0 | 2,45 | 0 | 2,90 | 58 | 127 | 2,54 | 0,00 |
| 3% | B | 3 | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 18 | 60 | 1,94 | 3293 | 2,30 | 819 | 1528 | 3,01 | 0,00 |
| 3% | B2 | 3 | 1880 | 2776 | 2776 | 2776 | 100,0 | 1999 | 0,8 | 601 | 18 | 65 | 2,17 | 3025 | 3,40 | 64 | 198 | 2,81 | 0,00 |
| 3% | teto do j | 3 | 702 | 888 | 963 | 963 | 68,1 | 1691 | 4,8 | 600 | 0 | 0 | 2,16 | 1915 | 3,70 | 735 | 932 | 3,08 | 0,00 |
| 5% | voz | 3 | — | — | — | — | — | — | — | — | — | — | 0,20 | — | 5,40 | 31 | 32 | 5,10 | 0,00 |
| 5% | A | 3 | 2004 | 2916 | 3086 | 3119 | 26,9 | 2068 | 10,1 | 222 | 1 | 0 | 1,65 | 0 | 4,60 | 69 | 284 | 4,74 | 0,00 |
| 5% | B | 3 | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 14 | 63 | 1,45 | 3422 | 3,40 | 997 | 3734 | 4,76 | 0,00 |
| 5% | B2 | 3 | 2490 | 3633 | 3633 | 3633 | 99,1 | 2763 | 0,6 | 600 | 14 | 67 | 1,61 | 3255 | 6,10 | 58 | 206 | 4,29 | 0,00 |
| 5% | teto do j | 3 | 776 | 2005 | 2036 | 2036 | 92,3 | 12590 | 1,3 | 601 | 0 | 0 | 1,28 | 2638 | 4,50 | 1184 | 3566 | 4,56 | 0,00 |
| 10% | voz | 3 | — | — | — | — | — | — | — | — | — | — | 0,21 | — | 9,10 | 31 | 32 | 10,09 | 0,00 |
| 10% | A | 3 | 6325 | 11615 | 11626 | 11626 | 81,7 | 4248 | 2,7 | 75 | 1 | 0 | 0,67 | 0 | 6,80 | 287 | 2392 | 10,98 | 0,00 |
| 10% | B | 3 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 8 | 61 | 0,78 | 3522 | 14,40 | 2180 | 5540 | 10,30 | 0,00 |
| 10% | B2 | 3 | 5009 | 12778 | 12778 | 12778 | 100,0 | 9369 | 0,2 | 601 | 6 | 67 | 0,74 | 3475 | 10,10 | 146 | 2737 | 10,33 | 0,00 |
| 10% | teto do j | 3 | 1155 | 10161 | 10161 | 10161 | 100,0 | 10427 | 0,1 | 601 | 0 | 0 | 0,60 | 3180 | 10,70 | 3789 | 10606 | 10,34 | 0,00 |
| 1,0 Mbps, 0% | voz | 3 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,00 | 33 | 34 | 0,00 | 0,00 |
| 1,0 Mbps, 0% | A | 3 | 1228 | 3181 | 3487 | 3496 | 24,6 | 2175 | 15,1 | 322 | 1 | 0 | 2,28 | 0 | 0,40 | 218 | 395 | 0,00 | 1,72 |
| 1,0 Mbps, 0% | B | 3 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 19 | 62 | 2,44 | 3454 | 1,90 | 676 | 3161 | 0,00 | 0,47 |
| 1,0 Mbps, 0% | B2 | 3 | 1811 | 2759 | 2759 | 2759 | 100,0 | 2483 | 0,9 | 601 | 18 | 66 | 2,39 | 2849 | 0,80 | 219 | 367 | 0,00 | 1,18 |
| 1,0 Mbps, 0% | teto do j | 3 | 310 | 1725 | 1725 | 1725 | 95,5 | 16408 | 0,8 | 601 | 0 | 0 | 2,32 | 1821 | 1,90 | 582 | 1668 | 0,00 | 1,36 |
| 2,0 Mbps, 1% | voz | 3 | — | — | — | — | — | — | — | — | — | — | 0,19 | — | 0,90 | 32 | 33 | 1,02 | 0,00 |
| 2,0 Mbps, 1% | A | 3 | 828 | 1986 | 2345 | 2477 | 12,1 | 1122 | 21,7 | 454 | 1 | 0 | 3,16 | 0 | 0,90 | 59 | 111 | 1,09 | 0,00 |
| 2,0 Mbps, 1% | B | 3 | — | — | — | — | 100,0 | 20000 | 0,0 | 601 | 27 | 61 | 2,88 | 3078 | 0,70 | 488 | 902 | 1,02 | 0,00 |
| 2,0 Mbps, 1% | B2 | 3 | 1315 | 2095 | 2470 | 2470 | 100,0 | 2054 | 1,1 | 600 | 24 | 63 | 2,92 | 2771 | 0,60 | 56 | 178 | 1,08 | 0,00 |
| 2,0 Mbps, 1% | teto do j | 3 | 406 | 779 | 858 | 919 | 50,9 | 1197 | 7,6 | 600 | 0 | 0 | 2,83 | 1353 | 1,30 | 516 | 840 | 1,06 | 0,00 |
