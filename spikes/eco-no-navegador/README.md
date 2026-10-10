# O eco no navegador

**A pergunta** (F8, 09/10/2026, para o aviso de fone das duas cascas): o
`echoCancellation` do `getUserMedia` cancela o som que **a voz do SEELE**
toca? Se cancelar, a casca pode dizer que o navegador cancela o eco; se não,
continua pedindo o fone. Uma frase que afirma o cancelamento sem medida é a que
convence a tirar o fone justamente quem faz o outro lado se ouvir.

**Por que medir, e não supor.** Pedir o cancelador não é cancelar. A voz do
SEELE chega pelo WebTransport e toca pelo Web Audio, e não por um
`RTCPeerConnection`. Se o cancelador do navegador usa essa saída como
referência depende do motor e do sistema, e é isso que esta página mede.

## Como mede

`pagina.js` toca um tom de 1 kHz pelo Web Audio (o mesmo caminho da voz, até o
`destination`) e lê o nível de 1 kHz no microfone, em quatro rodadas
alternadas, com o `echoCancellation` desligado, ligado, desligado e ligado (sem
`noiseSuppression` nem `autoGainControl`, que mexeriam no nível). Cada rodada:
- 1,5 s de silêncio, a referência;
- o tom: 2 s para o cancelador aprender, e 2 s medidos;
- **o retorno** é o nível do tom menos o do silêncio, em dB.

**O veredito, por navegador:**
- **sem laço**, se o retorno sem o cancelador ficar abaixo de 20 dB: o tom não
  chegou ao microfone (fone, volume baixo, a saída noutro lugar), e a medida
  não diz nada;
- **cancela**, se o cancelador tirar 20 dB ou mais;
- **cancela em parte**, de 6 a 20 dB; **não cancela**, abaixo de 6.

A página também diz o cancelador **aplicado** (`getSettings()`): um navegador
que aceita o pedido e não o liga apareceria aí.

## Rodar

    python3 spikes/eco-no-navegador/servir.py mac       # os navegadores do Mac
    python3 spikes/eco-no-navegador/servir.py iphone    # o iPhone, na mesma rede
    python3 spikes/eco-no-navegador/servir.py chromium  # a conferência automática

O `chromium` abre o Chromium do Playwright **mudo e com o microfone falso**.
Ele não mede eco nenhum (não há laço). Ele confere que a página roda as quatro
rodadas, que o cancelador aplicado é o pedido, e que, sem laço, o veredito diz
«sem laço» em vez de inventar um número. Passou em 09/10/2026.

Os resultados vão para `resultados/eco.jsonl`, que o git ignora (lá também
mora a chave do certificado da página do iPhone).

## O que ainda não foi medido

Tudo o que precisa de alto-falante e microfone de verdade. Os passos são do
dono, em `docs/superpowers/sdd/2026-10-08-0.16/F4/medicoes-do-dono.md`
(«O eco no navegador»): o Chrome, o Safari e o Firefox do Mac, e o Safari do
iPhone. **O Simulador do iOS não serve**: ele toca e grava pelo áudio do Mac,
então o cancelador que ele exercitaria é o do macOS.
