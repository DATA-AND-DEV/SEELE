# O vídeo por RTP medido no produto, contra o canal, no mesmo binário

**Data:** 2026-11-10 · **Árvore:** `entrega/1.0`, commit `f5824171` mais o ajuste do quadro de mentira deste commit
**O que é:** a etapa 5 do [ADR 0067](../adr/0067-o-video-vai-por-rtp-na-1-0.md). O mesmo `tela_do_produto_sob_perda` das medidas anteriores (servidor de verdade, duas conexões de verdade, o `Cano` entre quem transmite e o servidor), rodado duas vezes: com o vídeo por RTP (o padrão) e com `SEELE_VIDEO_PELO_CANAL=1`. Mediana de 3 rodadas de 20 s; as saídas inteiras estão em `brutos-2026-11-10-rtp-no-produto/`.

```sh
B=(cargo run --release --config profile.release.debug-assertions=true -p seele-conformance --example tela_do_produto_sob_perda --)
"${B[@]}" --variante produto                      # e com --aquecimento 40
SEELE_VIDEO_PELO_CANAL=1 "${B[@]}" --variante produto   # e com --aquecimento 40
```

O quadro de mentira passou a levar o número e o instante em hexadecimal, com enchimento sem zeros: em binário ele trazia `00 00 01` dentro do quadro, que o empacotador de H.264 lê como começo de NAL.

## Regime (40–60 s): atraso p50 ms / congelado % / kbps oferecidos

| cenário | RTP | canal |
|---|--:|--:|
| 0% | 64 / 0,0 / 2375 | 32 / 0,0 / 2375 |
| 0,3% | 66 / 0,0 / 2375 | 32 / 1,1 / 827 |
| 1% | 69 / 0,0 / 2375 | 32 / 2,4 / 479 |
| 3% | 109 / 0,0 / 2375 | 32 / 20,6 / 191 |
| 5% | 127 / 0,0 / 2375 | 64 / 37,1 / 170 |
| 1,0 Mbps, 0% | 55 / **25,6** / 599 | 51 / 3,4 / 585 |

## Primeiros 20 s: atraso p50 ms / congelado % / quadros-chave

| cenário | RTP | canal |
|---|--:|--:|
| 0% | 66 / **9,2** / 12 | 32 / 1,5 / 1 |
| 0,3% | 67 / 10,1 / 16 | 37 / 2,5 / 3 |
| 1% | 86 / 8,3 / 16 | 57 / 7,5 / 6 |
| 3% | 126 / 8,1 / 10 | 662 / 35,2 / 15 |
| 5% | 141 / 10,6 / 19 | 1073 / 52,8 / 17 |
| 1,0 Mbps, 0% | 41 / 30,1 / 60 | 48 / 33,2 / 10 |

## O que mostram

1. **Sob perda, em regime, o RTP entrega a imagem inteira e na banda cheia**: 0% congelado de 0,3% a 5%, a 2375 kbps. O canal cai a 170–191 kbps e congela 21% e 37% a 3% e 5%.
2. **O RTP custa cerca de 30 ms de atraso sem perda** (64 contra 32 ms): o ritmo de saída e a espera de reordenação.
3. **A partida por RTP é pior que a do canal sem perda**: 9% congelado e 12 quadros-chave nos primeiros 20 s, contra 1,5% e 1. Os pedidos de chave (9 a 15 por rodada) dizem onde olhar: os quadros que saem antes de o servidor abrir a entrada, e a chave pedida de novo antes de a anterior chegar.
4. **Em banda estreita (1 Mbps) o RTP é bem pior em regime**: 25,6% congelado e 30 quadros-chave em 20 s, contra 3,4% do canal. A estimativa passa do que o caminho carrega, e cada quadro-chave grande piora.

## O que NÃO mostram

Um espectador só, e o `Cano` só no caminho de quem transmite; codificador de mentira; nenhum navegador; nenhuma chamada no aplicativo de verdade. Três rodadas por célula.

## O que fica aberto

Os itens 3 e 4 são defeitos do caminho novo, e não da medida. Enquanto não forem consertados e medidos de novo, o RTP não está pronto para ser o padrão sem ressalva.

## Adendo de 11/11/2026: os dois defeitos, consertados e medidos de novo

**Árvore:** `entrega/1.0`, commit `277b87cf` mais o piso do teto pela mídia deste commit. As saídas inteiras estão em `brutos-2026-11-11-rtp-no-produto/`.

Dois consertos por suposição (guardar os quadros de antes da abertura, espaçar os quadros-chave) **não mudaram os números**. Com o código instrumentado, as causas eram outras três:

1. **O balde do canal recusava o quadro-chave no RTP.** Cada recusa pedia outro, recusado de novo: 40 recusas em 20 s sem perda. No RTP quem dosa a saída é o transporte, e o balde deixou de decidir ali.
2. **O servidor começava cada espectador a 300 kbps** (a estimativa inicial do `str0m`). Um quadro-chave levava segundos para sair, a fila de envio estourava, e quem assistia perdia quadros com a rede perfeita. A perna de descida passou a partir do teto do vídeo.
3. **Cada pequena mudança da estimativa reconfigurava o codificador**, e reconfigurar custa um quadro-chave. A 1 Mbps a rajada derrubava a própria estimativa (de 3,3 Mbps a 260 kbps em 6 s). O teto passou a cair na hora, mas só de 15% para mais, e a subir de 25% para mais, no máximo a cada 3 s.

E um quarto, achado na medida seguinte: a estimativa caindo abaixo do piso **parava** o vídeo, e parado ele não tem de onde subir (18 s de imagem parada numa rodada). O teto pela mídia não vai mais abaixo do piso.

### Primeiros 20 s: atraso p50 ms / p95 ms / congelado % / quadros-chave

| cenário | RTP antes | RTP agora | canal |
|---|--:|--:|--:|
| 0% | 66 / 777 / 9,2 / 12 | 66 / 72 / 2,8 / 3 | 32 / 33 / 1,5 / 1 |
| 0,3% | 67 / 756 / 10,1 / 16 | 66 / 129 / 2,7 / 3 | 37 / 465 / 2,5 / 3 |
| 1% | 86 / 744 / 8,3 / 16 | 67 / 142 / 3,0 / 3 | 57 / 1324 / 7,5 / 6 |
| 3% | 126 / 794 / 8,1 / 10 | 96 / 156 / 1,6 / 3 | 662 / 3678 / 35,2 / 15 |
| 5% | 141 / 794 / 10,6 / 19 | 124 / 176 / 2,8 / 3 | 1073 / 5797 / 52,8 / 17 |
| 1,0 Mbps, 0% | 41 / 974 / 30,1 / 60 | 43 / 221 / 5,0 / 8 | 48 / 3170 / 33,2 / 10 |
| 2,0 Mbps, 1% | 62 / — / 21,0 / 39 | 38 / 304 / 6,1 / 8 | — |

### Regime (40–60 s): atraso p50 ms / congelado % / kbps oferecidos

| cenário | RTP antes | RTP agora | canal |
|---|--:|--:|--:|
| 0% | 64 / 0,0 / 2375 | 66 / 0,0 / 2375 | 32 / 0,0 / 2375 |
| 1% | 69 / 0,0 / 2375 | 66 / 0,0 / 2375 | 32 / 2,4 / 479 |
| 3% | 109 / 0,0 / 2375 | 111 / 0,0 / 2335 | 32 / 20,6 / 191 |
| 5% | 127 / 0,0 / 2375 | 123 / 0,0 / 2375 | 64 / 37,1 / 170 |
| 1,0 Mbps, 0% | 55 / 25,6 / 599 | 51 / 0,0 / 455 | 51 / 3,4 / 585 |
| 2,0 Mbps, 1% | 60 / 7,9 / 1117 | 43 / 0,0 / 401 | 45 / 7,1 / 567 |

Nenhum pedido de quadro-chave em nenhuma célula, e nenhuma pausa do vídeo em 42 rodadas.

### O que fica

- **Em banda estreita o RTP usa menos do caminho do que ele carrega**: 455 kbps de 1 Mbps, e 401 kbps de 2 Mbps com 1% de perda. A imagem não congela, mas sai com menos qualidade do que podia. A estimativa só cresce com o que é mandado, e a subida do teto é lenta de propósito; sondar o caminho com enchimento não está ligado.
- O RTP custa cerca de 30 ms de atraso sem perda (66 contra 32 ms).
- Os limites da medida são os de antes: um espectador, o `Cano` só no caminho de quem transmite, codificador de mentira, nenhum navegador, nenhuma chamada no aplicativo de verdade.

## Adendo de 11/11/2026, à tarde: o sinal, a faixa e a chave na troca de orçamento

Três mudanças depois da tabela acima, e a medida de novo (commit `9ab54567`,
mediana de 3 rodadas, variante **produto**, os mesmos cenários; brutos em
`brutos-2026-11-11-rtp-no-produto/rtp4-grade.out` e `rtp4-regime.out`):

1. **O sinal deixou de ficar em 100** (`22d022b1`). Com ele se mexendo, a faixa
   do sinal passou a cortar o teto do vídeo — o laço nunca tinha rodado. Numa
   medida intermediária a 5% de perda o vídeo caiu de 2375 para 1215–1459 kbps
   com o caminho livre.
2. **Pela mídia, a faixa não corta o teto de novo** (`9865e553`): quem regula
   a taxa no RTP é a estimativa do TWCC.
3. **A troca de orçamento não custa mais um quadro-chave à câmera**
   (`9ab54567`). A 1 Mbps, cada subida do teto reconfigurava o codificador, a
   chave saía em rajada e a estimativa que acabara de subir caía a um terço:
   uma serra. Numa rodada de 40 s, 13 chaves e 370 kbps com a chave na troca,
   1 chave e 526 kbps sem. O exemplo segue a casca; `--com-chave-na-troca`
   repete o comportamento antigo.

Latência p50 (ms) / congelado (%) / kbps oferecidos:

| cenário | primeiros 20 s | em regime (aquecimento de 20 s) | regime antes (tabela acima) |
|---|--:|--:|--:|
| 0% | 65 / 0,0 / 1993 | 66 / 0,0 / 2375 | — |
| 0,3% | 66 / 0,0 / 1990 | 67 / 0,0 / 2375 | — |
| 1% | 66 / 0,0 / 1993 | 69 / 0,0 / 2330 | — |
| 3% | 91 / 0,0 / 1980 | 104 / 0,0 / 2375 | 111 / 0,0 / 2335 |
| 5% | 123 / 1,0 / 1771 | 129 / 0,0 / 2375 | 123 / 0,0 / 2375 |
| 1,0 Mbps, 0% | 47 / 1,5 / 312 | 53 / 0,0 / 499 | 51 / 0,0 / 455 |
| 2,0 Mbps, 1% | 64 / 0,0 / 1090 | 64 / 0,0 / 1203 | 43 / 0,0 / 401 |

Nos primeiros 20 s o congelado era de 1,6% a 6,1%; agora é de 0% a 1,5%. Um
quadro-chave por rodada (o do começo), nenhum pedido de chave, nenhuma pausa,
nenhuma tentativa falhada em 42 rodadas.

### O que fica, depois deste adendo

- **A 1 Mbps o RTP ainda usa menos do caminho que o canal**: 499 contra 585
  kbps. A 2 Mbps com 1% de perda passou dele (1203 contra 567).
- **Entre duas máquinas de verdade** (um Mac e um Windows na mesma rede, o
  `seeled` no Mac, `tela_entre_maquinas`): ver o fim de `docs/teste-duas-maquinas.md`.
- Os limites da medida são os de antes.

## Repetição no código da entrega (12/11, commit `17647b27`)

A mesma grade, variante **produto**, mediana de 3 rodadas, depois de tudo o que
entrou desde a medida anterior (relato de recepção, transmissão que cai,
codificador do Windows, sala pessoal). Brutos em
`brutos-2026-11-12-rtp-no-produto/rtp5-20s.out` e `rtp5-regime.out`.

| cenário | primeiros 20 s: congelado % · kbps oferecidos | em regime: congelado % · kbps oferecidos |
|---|--:|--:|
| 0% | 0,0 · 2003 | 0,0 · 2375 |
| 0,3% | 0,0 · 2004 | 0,0 · 2375 |
| 1% | 0,0 · 2006 | 0,0 · 2375 |
| 3% | 0,0 · 1994 | 0,0 · 2375 |
| 5% | 0,0 (0,0–1,1 entre rodadas) · 2005 | 0,0 (0,0–1,0) · 2375 |
| 1,0 Mbps, 0% | 1,5 (1,5–2,1) · 220 | 0,0 · 444 |
| 2,0 Mbps, 1% | 0,0 · 899 | 0,0 · 1266 |

Um quadro-chave por rodada nos primeiros 20 s e nenhum em regime; nenhum pedido
de chave, nenhuma pausa, nenhum quadro recusado ou perdido. Igual à medida de
11/11 dentro da variação entre rodadas.

**O que esta rodada não vale:** a parte «em regime» correu com outras
compilações na mesma máquina (a de 20 s correu sozinha). Os números de regime
não pioraram com isso, mas a rodada não serve de referência de latência.
