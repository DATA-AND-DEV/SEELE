# O som de MOD no WKWebView — antes e depois do WebAudio

Especificação de 23/09, Parte II, «Diagnóstico para quem escreve MOD», item 5:
«Primeiro medir no WKWebView.» A medição anterior foi só no Chromium, com a CSP
exata: `default-src 'self'` e nenhum `media-src`, então a mídia herda `'self'`
e recusa `data:`.

**MOD medido:** `apps/seele-app/testes/mod-de-referencia` (`seele/referencia`
2.0.0, API 5), som `som/toque.wav` (1644 bytes).

**A ordem importa.** Depois do WebAudio, os sons de MOD dividem um contexto de
áudio por janela, e um toque aceito libera os seguintes. Por isso o caso B (o
botão do MOD) vem **primeiro**, numa janela recém-aberta, e o caso A depois.

## Máquina

| | |
|---|---|
| macOS (`sw_vers -productVersion`) | 27.0.1 (26A434) |
| WebKit (`CFBundleVersion`) | 22625.1.29.11.28 |
| saída de som | a padrão do sistema, o monitor 27QHD240 (DisplayPort); ver «O repouso do Mac» para a do «depois» |
| release publicado (comando do `CLAUDE.md`) | v0.15.0 · `e2fac4dab` · 2026-09-23 |

## Antes

| | |
|---|---|
| commit medido (`git rev-parse --short HEAD`) | `3d50b21` |
| build (debug ou release) | debug |

| caso | ouviu? | o que a mídia diz na tela | o que o MOD escreve (`som: …`) | console (linhas «Refused» / «Content Security Policy») |
|---|---|---|---|---|
| B · `TOCAR` do MOD | não | nenhum estado em texto sobre a figura; o tocador nativo diz «Error» | `som: recusada` | `Refused to load data:audio/wav;base64,UklGRmQGAABX… because it appears in neither the media-src directive nor the default-src directive of the Content Security Policy.` |
| A · o controle nativo do `<audio>`, dentro da mídia | não | o tocador nativo continua dizendo «Error» | `som: recusada` (não muda) | a mesma recusa |

`seele.log` (linhas «mídia de MOD servida» e, se houver, «mídia de MOD recusada» ou `onde=recusa-de-mod`):

```text
2026-10-08T00:45:29.481662Z  INFO seele_app: mídia de MOD servida mod_id=seele/referencia geracao=4 caminho="som/toque.wav" papel="som" bytes=1644
2026-10-08T00:45:29.526098Z  WARN seele_app: seele/referencia: mídia «toque» o elemento de mídia não abriu (evento error) onde=recusa-de-mod mod_id=seele/referencia
2026-10-08T00:46:10.703242Z  WARN seele_app: seele/referencia: mídia «toque» não tocou: The operation is not supported. onde=recusa-de-mod mod_id=seele/referencia
```

O Rust serviu os 1644 bytes; a página os recusou ao montar o `<audio>`
(00:45:29) e o `play()` do caso B falhou (00:46:10). O console foi lido no Web
Inspector do build de debug, aberto pelo menu de contexto.

## Depois

| | |
|---|---|
| commit medido (`git rev-parse --short HEAD`) | `f1ec0b3` |
| build (debug ou release) | debug |

| caso | ouviu? | o que a mídia diz na tela | o que o MOD escreve (`som: …`) | console |
|---|---|---|---|---|
| B · `TOCAR` do MOD | sim (22:06:50) | nenhum estado em texto; a figura tem o botão do produto «Tocar: um toque curto» | nada (nenhum `som: recusada`) | nenhuma recusa de CSP; só o aviso «Não foi possível conferir os MODs: – Error: disconnected» (`carregarMods`, `base.js:2458`), do instante em que o servidor reiniciou ao aplicar o MOD |
| A · `TOCAR` do produto, dentro da mídia | sim (22:07:31) | o botão volta a «Tocar» logo: o toque dura 0,2 s e não dá tempo de apertar «Pausar»; apertado de novo, toca de novo (23:34:16) | nada | a mesma coisa |

`document.querySelectorAll("audio").length` no console: `0`

`seele.log` (linhas «som de MOD servido em bytes» e, se houver, «mídia de MOD recusada» ou `onde=recusa-de-mod`):

```text
2026-10-08T01:06:17.952473Z  INFO seele_app: som de MOD servido em bytes mod_id=seele/referencia geracao=4 caminho="som/toque.wav" bytes=1644
```

Nenhuma linha `mídia de MOD recusada` nem `onde=recusa-de-mod` na sessão inteira.

### Os casos da revisão ampla do Plano 1D

**Como o repouso foi medido.** No «antes», o monitor do `pmset` (uma leitura
por segundo) mostrou que **a voz do SEELE segura o aparelho de saída a sessão
inteira, também fora de sala**: a asserção `PreventUserIdleSystemSleep` do
`coreaudiod` nasceu ao entrar no servidor (21:41:17, junto com a linha «a
supressão de ruído do microfone está tratando o sinal») e só caiu ao fechar o
app (21:54:39). Com a voz e o som de MOD no mesmo aparelho, o `pmset` não separa
os dois. Por isso, no «depois», a saída padrão do macOS (para onde vai o som da
janela) foi para os **alto-falantes do MacBook**, e a saída da voz do SEELE, nos
ajustes do app, para o **monitor 27QHD240**. O monitor gravou, a cada segundo,
as asserções do `coreaudiod` (o nome de cada uma traz o UID do aparelho) e
quais aparelhos estavam rodando (`kAudioDevicePropertyDeviceIsRunningSomewhere`).

- **O repouso do Mac (I-2), montado e parado, antes de qualquer toque** (logo
  antes do caso B): os alto-falantes do MacBook **parados e sem asserção**; só a
  voz (27QHD240) e o microfone seguravam o Mac.
- **Com a trilha da MESA tocando** (22:10:51): os alto-falantes do MacBook
  rodando, com asserção — o controle de que o monitor vê o som da janela.
- **12 s depois de pausar pelo `PAUSAR` do produto** (pausada às 22:11:07): a
  trilha parou na hora (ouvido); a asserção ficou os 10 s da suspensão e caiu
  às 22:11:18. Parado.
- **3 s depois de sair da sessão** (saiu às 23:43:44): todos os aparelhos
  pararam em 1 s; só a asserção do `AudioTap` (o recorte que tira o som do
  SEELE da captura de tela) durou até 23:43:47. Nenhuma a partir de 23:43:48.
- **Fechar a página com som tocando (I-1).** A trilha da MESA tocando desde
  22:11:36, a página fechada pelo «Voltar à conversa» às 22:11:40: parou na hora
  e não voltou (ouvido). O áudio da janela caiu às 22:11:51, os 10 s da
  suspensão, e não voltou; se a trilha seguisse, ele ficaria de pé até 22:12:06.
- **Um som de contribuição, que não tem botão.** Medido com um MOD de medida
  fora do repositório (`medida/som-de-contribuicao`, API 5: a trilha
  `exploracao.wav` da MESA numa contribuição **sem alvo**, `forma: "midia"`,
  `tocando: true`). Com um destino, tocou sozinho, sem clique, uma vez, e
  terminou (o MOD recebeu `tocando` e depois `terminou`). Com dois destinos (a
  contribuição em `canal.item`, com dois canais), tocou **uma vez só, sem eco**
  (ouvido), e o `seele.log` tem uma linha só de «som de MOD servido em bytes»
  para ela: um destino tomou o som. Com duas **pessoas** não deu: quem visita só
  obtém MODs do catálogo, e o de medida não está nele; o mecanismo (o primeiro
  destino toma os sons, `mods-regiao.js`) é o mesmo para canais e pessoas.
- **A trilha passa de 30 s? (m-4).** Declarada tocando pela MESA, a trilha
  ficou com o áudio da janela aberto **sem nenhuma queda** de 22:12:20 até ser
  pausada às 22:39:29 (27 minutos): ela volta do começo a cada fim, e nunca
  houve 10 s calados. Se há um silêncio curto entre uma volta e outra, não deu
  para perceber de ouvido.
- **Caso B sem gesto:** tocou, sem recusa. O wry 0.55.1 nasce com
  `autoplay: true`, como o plano previa.
- **macOS 11.0 a 11.2: não medido.** Não havia máquina.

## Conclusão

**Antes**, o WKWebView do macOS 27.0.1 (WebKit 22625.1.29.11.28) não tocava o
som de MOD: a CSP recusava o `data:` do `<audio>` e o `play()` falhava, nos dois
casos. **Depois**, os dois casos tocam, sem elemento `<audio>` e sem recusa de
CSP, o som de uma contribuição toca uma vez por contribuição, e o áudio da
janela só abre no toque, se suspende 10 s depois de calar e não fica depois de
sair da sessão. O roteiro separou os dois estados: a hipótese de que o WKWebView
recusava se confirma, e a de que o WebAudio conserta também.

## WebView2 (Windows)

| | |
|---|---|
| Windows | 10.0.26300.9457, o PC de testes de quem opera |
| WebView2 | 154.0.4258.62 |
| commit medido | `f1ec0b3`, levado por bundle (`e2fac4d..main`) e compilado lá, em debug |
| como | o app aberto na sessão interativa com `--remote-debugging-port=9222`; os cliques, o console e a contagem de `<audio>` pelo DevTools Protocol, por um túnel SSH; o `seele.log` e o `powercfg /requests` (de 2 em 2 s, com administrador) pelo SSH; os seletores de pasta e o ouvido, de quem estava no PC |

| caso | ouviu? | o que o MOD escreve | console |
|---|---|---|---|
| B · `TOCAR` do MOD, primeiro toque de uma janela nova | sim (00:03:43) | nada | nenhuma recusa de CSP |
| A · `TOCAR` do produto | sim (00:04:00; e às 00:02:03, numa janela anterior) | nada | nenhuma recusa de CSP; só a recomendação do Chromium sobre um campo de senha fora de formulário |

`document.querySelectorAll("audio").length`: `0`. No `seele.log`, «som de MOD
servido em bytes» para `seele/referencia`, e nenhuma linha
`onde=recusa-de-mod`.

**O repouso.** No Windows o `powercfg /requests` separa as duas fontes sozinho:
a voz do SEELE aparece como pedido de **driver** (a saída NVIDIA High
Definition Audio e o microfone USB), a sessão inteira, também fora de sala; o
som da página aparece como **processo**, `msedgewebview2.exe: Playing audio`.

- **Montado e parado, antes de qualquer toque:** nenhum processo; só os
  drivers da voz.
- **Com a trilha da MESA tocando** (00:06:32): `msedgewebview2.exe: Playing
  audio`.
- **12 s depois de pausar** (pausada às 00:06:44): parou na hora (ouvido); o
  WebView2 soltou o áudio em menos de 3 s e continuou solto.
- **Depois de sair da sessão** (00:02:45): nenhum processo nem driver, na
  primeira leitura.
- **Fechar a página com som tocando (I-1):** fechada às 00:07:16 com a trilha
  tocando; parou na hora e não voltou (ouvido); o WebView2 soltou em 3 s.
- **Um som de contribuição:** o mesmo MOD de medida, em `canal.item`, com dois
  canais. Tocou sozinho, **uma vez só, sem eco** (ouvido). O `seele.log` tem a
  mídia pedida uma vez por canal («mídia de MOD servida», duas linhas) e os
  bytes do som uma vez («som de MOD servido em bytes», uma linha).
- **A trilha passa de 30 s? (m-4):** em laço enquanto a MESA a declara
  tocando; o WebView2 segurou o áudio sem nenhuma queda de 00:07:48 a 00:09:16.
  De ouvido, **sem silêncio** entre as voltas: o volume diminui e volta a
  subir, o fade da própria trilha.

## Conclusão do WebView2

O som de MOD toca no WebView2 154 do Windows nos dois casos e na contribuição,
sem `<audio>` e sem recusa de CSP, e o áudio da página não fica preso depois de
pausar, de fechar a página ou de sair da sessão. Com o «depois» do WKWebView,
o item 14 de «Testes» da especificação de 23/09 está cumprido nas duas
plataformas de desktop medidas; o macOS 11.0 a 11.2 continua não medido.
