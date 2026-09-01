# quadros-para-a-casca

Gera quadros H.264 de verdade, em base64, para provar a metade da casca que
**recebe** uma tela — `palco-imagem.js`, o `VideoDecoder` da janela e o canvas.

Ela só roda num navegador, e um navegador não tem como pedir quadros a um
servidor durante um teste. Os quadros que ele produz estão embutidos em
`tools/roteiros/palco.js`; este binário é o registro de **como** foram feitos.

    cd spikes/quadros-para-a-casca
    cargo run -- ~/.config/seele > quadros.json

E a prova, que é o que se roda no dia a dia:

    python3 tools/carga-da-casca.py --roteiro tools/roteiros/palco.js

Sem o conserto de `armarPeloSps`, ela diz `canvas: ESCONDIDO 0x0`. Com ele,
`canvas: visivel 960x540` e metade dos pixels claros — o xadrez que entrou.
