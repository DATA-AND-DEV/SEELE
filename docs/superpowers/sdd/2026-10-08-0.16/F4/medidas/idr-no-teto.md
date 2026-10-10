# O teto do quadro de tela, medido (F4, 09/10/2026)

**Por quê.** O `MAX_QUADRO_LEN` (512 KiB) vai para o `seele_proto::screen`
(D-celular-2), e ali ele vira contrato da linha 1: uma 1.x não pode mandar
quadro maior para uma 1.0, que o recusaria. A F1 pediu a medida antes de
congelar. A única que existia era a do `spikes/tela-no-codec`: uma IDR de
65,4 KiB a 1080p, **a 1200 kbps**, com tela real e em regime.

**O que ela não cobria.** O produto chega a 1080p com tetos bem acima de
1200 kbps:
- o teto é `min(caminho de quem hospeda × 60% ÷ espectadores, caminho de quem
  compartilha × 60%, escolha da pessoa)` (`camada-compartilhar.js:10-14`);
- o limiar de 1080p é de cerca de 6,2 Mbps (`TETO_ESTIMADO_PARA_1080P_BPS`), e
  há um degrau no dobro dele;
- numa rede local, com um espectador e sem escolha, o teto passa muito disso.

## Como foi medida

O codificador do produto (`seele_video::codec::Codificador`, OpenH264, CABAC,
modo bitrate, 30 quadros). O módulo é o `libopenh264.dylib` que o produto
baixou, com SHA-256 `052e98bf…` (o mesmo do README do spike).

- O conteúdo vem de `telas.py`, sem captura da tela de quem roda. São 300
  quadros que rolam 3 px por quadro, como no spike, e dão a volta no fim da
  tela. A volta é uma troca de cena, como trocar de janela.
- A IDR fria é a da partida. As IDRs em regime são forçadas a cada 2 s, depois
  de 30 quadros de aquecimento.
- O programa, em `idr_no_teto.rs.txt`, é um exemplo do `seele-video`. Para
  rodar, copie-o para `crates/seele-video/examples/idr_no_teto.rs` e:

      python3 telas.py /tmp/telas
      cargo run --release -p seele-video --example idr_no_teto -- \
        ~/.config/seele/libopenh264.dylib /tmp/telas/codigo.yuv /tmp/telas/foto.yuv

## O resultado, em KiB

| tela | teto | IDR fria | IDR em regime (máx.) | maior quadro comum |
|---|---|---|---|---|
| código | 1,2 Mbps | 140 | 121 | 77 |
| código | 6,2 Mbps | 177 | 154 | 271 |
| código | 12,4 Mbps | 198 | 230 | 344 |
| código | 25 Mbps | 198 | 283 | 351 |
| código | 50 Mbps | 198 | 283 | 370 |
| código | 100 Mbps | 198 | 283 | 423 |
| foto com grão | 1,2 Mbps | 93 | 28 | 32 |
| foto com grão | 6,2 Mbps | 252 | 59 | 219 |
| foto com grão | 12,4 Mbps | 349 | 217 | **540** |
| foto com grão | 25 Mbps | 349 | 436 | **895** |
| foto com grão | 50 Mbps | 349 | **638** | **905** |
| foto com grão | 100 Mbps | 349 | **638** | **973** |

**Um quadro acima de 512 KiB aparece a partir de 12,4 Mbps**, com conteúdo
realista e tetos que o produto permite. Um conteúdo sintético extremo (cada
quadro inteiro novo, texto que é ruído preto e branco, ou ruído puro) chega a
1723 KiB. Ele serve só de cota superior, e não de caso de uso.

## O que acontece hoje com esse quadro

O lado que envia o descarta (`MotivoDeDescarte::GrandeDemais`,
`seele-core/src/tela.rs:1347` e `:1447`). O fluxo não morre. Uma IDR descartada
deixa quem assiste sem imagem até a próxima que caiba, e um quadro comum
descartado quebra a predição até a próxima IDR. A medida não conferiu se o
descarte é dito, nem se ele pede um quadro-chave novo.

## O limite que não corta quadro legítimo

A resolução tem teto em 1080p: «não há uma quarta»
(`seele-video/src/codec.rs:81-83`). Um quadro I420 cru de 1080p tem
1920 × 1080 × 1,5 = **3037,5 KiB**. O H.264 nunca precisa de mais que isso
(o macrobloco PCM carrega o pixel cru), fora alguns bytes de cabeçalho. Um
`MAX_QUADRO_LEN` de **4 MiB** nunca corta um quadro de 1080p.

O custo é a memória de um quadro em andamento por transmissão assistida, no
leitor que guarda: o `LeitorDeTela` do celular e a `Recepcao` do desktop. O
servidor conta bytes e não guarda quadro. A conferência continua antes de
alocar.
