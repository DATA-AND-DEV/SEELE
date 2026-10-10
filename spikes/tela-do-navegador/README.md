# spike `tela-do-navegador`: o que o VideoEncoder de um navegador de desktop codifica, e se o desktop o lê

**A pergunta** (Tarefa 3.10, D-web-3): o web app no PC compartilha a tela com o
`VideoEncoder` do WebCodecs. O fio declara `ScreenCodec::H264Baseline`, e o
desktop decodifica com o OpenH264. Que perfil cada navegador codifica de fato a
1080p, e o OpenH264 o lê?

## Como roda

    python3 servir.py <saída> chromium      # o Chromium do Playwright
    python3 servir.py <saída> safari        # o Safari do Mac (abre uma aba)

A página pergunta ao `isConfigSupported` por seis perfis, nas duas formas de
saída (`annexb` e `avc`), a 1920×1080, 6 Mbps e 30 quadros. Ela codifica 60
quadros (texto em monoespaçada rolando, um retângulo que se move, IDR no 0 e no
30) com cada perfil que diz aceitar em Annex-B. Os fluxos vão ao servidor, e um
marco por etapa diz onde a página parou.

O OpenH264 lê os fluxos por `ler_do_navegador.rs.txt`, um exemplo do
`seele-video`. Para rodar, copie-o para `crates/seele-video/examples/`:

    cargo run --release -p seele-video --example ler_do_navegador -- \
      ~/.config/seele/libopenh264.dylib <saída>/*.bin

## O que mediu (09/10/2026)

| perfil (pedido) | Chromium 151: `isConfigSupported` | Chromium 151: codificou | OpenH264 leu | Safari 27.0.1 (Mac): `isConfigSupported` | Safari: codificou | OpenH264 leu |
|---|---|---|---|---|---|---|
| Baseline restrito 3.1 (`42E01F`) | não | — | — | **sim** | **0 pedaços; o `flush` não volta** | — |
| Baseline restrito 4.0 (`42E028`) | sim | 60, SPS `66 192 40` | **60 de 60** | sim | 60, SPS `66 224 40` | **60 de 60** |
| Baseline restrito 4.2 (`42E02A`) | sim | 60, SPS `66 192 40` | 60 de 60 | sim | 60, SPS `66 224 42` | 60 de 60 |
| Baseline 3.1 (`42001F`) | não | — | — | **sim** | **0; o `flush` não volta** | — |
| Main 4.0 (`4D0028`) | sim | 60 | 59 de 60 (um retido pela reordenação) | **sim** | **0; o `flush` não volta** | — |
| High 4.0 (`640028`) | sim | 60 | 59 de 60 | **sim** | **0; o `flush` não volta** | — |

**O achado: no Safari, o `isConfigSupported` mente.** Ele diz que aceita os seis.
Em quatro deles o codificador recebe os 60 quadros, não devolve nenhum pedaço,
e o `flush` não volta nem com erro: nenhuma chamada ao `error`. A primeira
versão desta página morreu calada nesse `flush`. Só com um prazo e com um marco
por etapa deu para ver onde ela parava.

**O que isso pede ao produto:**
- **codificar no Baseline restrito 4.0, em Annex-B** (`avc1.42E028`,
  `avc.format: "annexb"`): é o perfil que o fio declara, os dois navegadores o
  produzem, e o OpenH264 do desktop lê 60 de 60;
- **não confiar no `isConfigSupported`.** O botão de compartilhar só se oferece
  depois de uma prova: um quadro de verdade codificado, com prazo. Sem pedaço
  dentro do prazo, o botão diz por quê, e o resto da sessão segue.

## O que não mede

- **O Chrome, o Edge e o Firefox de verdade** (o Chromium aberto não é o Chrome:
  o H.264 dele pode ser outro), e o Safari de outras versões. Os passos estão em
  `docs/superpowers/sdd/2026-10-08-0.16/F4/medicoes-do-dono.md`.
- **O som do `getDisplayMedia`**, que pede um gesto e a escolha da pessoa num
  diálogo do navegador. É medida manual, também nos passos do dono.
- **O custo**: a CPU e o tempo de codificar a 1080p no navegador.
