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
