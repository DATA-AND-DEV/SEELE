# Medição 3 no iPhone: o ALPN, o certificado curto e a página oculta

A medição 3 da F4 (`F4/medicoes-do-dono.md`, no `0.16/web`), feita pelo dono em
18/10/2026, das 13:52 às 13:57, num **iPhone com iOS 27.0.1, no Safari 27.0.1 em aba**
(não como web app da Tela de Início), na mesma Wi-Fi do Mac. O servidor foi o
`spikes/alpn-no-mesmo-socket` (`iphone.py`, portas UDP 4500 a 4504, página em
8443), com **requireUnreliable** marcado. Os brutos ficam em
`spikes/alpn-no-mesmo-socket/resultados/` (fora do git: têm endereço e user
agent). Esta página tira os endereços.

## O que foi medido

| caso | aba (iOS 27.0.1) |
|---|---|
| A · curto da **mesma chave** do longo | **abriu**, em 52 ms e em 36 ms, eco ok |
| A · um bit trocado no hash | recusado (18 e 13 ms) |
| A · o hash do longo, com o servidor apresentando o curto | recusado (15 e 12 ms) |
| A2 · curto de chave própria (controle) | **abriu**, em 23 ms, eco ok |
| B · só o longo (validade 1975–4096), hash certo | recusado (18 e 14 ms) |
| lista longa: até quantos hashes | **abriu com 500**, o certo por último (27 ms); 1, 15, 30, 60, 100, 200 e 500 abriram |
| recusa (hash errado), 3 vezes | 19, 10 e 11 ms, `WebTransportError source=session` |
| porta fechada, 3 vezes | 7, 9 e 9 ms, o mesmo erro |
| mudo (lê e descarta) | 30 005 ms, o mesmo erro |
| ninguém (um IP sem máquina, o `.250` da rede, conferido sem resposta ao ping e sem entrada no ARP) | **30 010 ms**, o mesmo erro (medido em 20/10, pela automação; abaixo) |
| sem áudio, tela bloqueada (~1 min e ~2 min 30 s) | **a sessão cai ~25 s depois de bloquear** (medido em 20/10, à mão; abaixo) |
| **com o áudio do produto** (`play-and-record`), tela bloqueada ~2 min 14 s | **sobreviveu**: oculta de ~13 s a ~143 s, 149 de 150 pings voltaram, e o servidor viu a sessão durar 152 375 ms até ela ser parada à mão |
| com áudio, tela bloqueada ~3 min 14 s | **sobreviveu**, 189 de 190 pings (medido em 20/10, à mão); os 5 min não foram alcançados |
| como web app da Tela de Início | **não medido** |

No servidor, toda recusa do certificado é o iPhone abortando o aperto de mão
(`error 46: TLS alert`), e a sessão aberta tem `max_datagram` de 1295 bytes.

**A versão do iOS.** O user agent do Safari diz «iPhone OS 18_7», mas esse número
é fixo: o Safari congela a versão do sistema que declara aos sites. A sessão do
`safaridriver` em 20/10 leu do aparelho o iOS **27.0.1** e o Safari 27.0.1. A
versão de 18/10 é a mesma.

## 20/10: o «ninguém», pela automação

Em 20/10 o caso 3 rodou de novo pelo `safaridriver`, com o iPhone no cabo, o «Web
Inspector» e a «Automação Remota» ligados, e a página aberta pela sessão. **O
toque do WebDriver no botão não dispara o caso** (o elemento recebe o clique, e a
página não começa), e a digitação no campo não entra. O caso foi acionado pelo
script da página (o `value` do campo e o `click()` do botão). Os tempos da recusa
(40, 10 e 8 ms), da porta fechada (11, 8 e 10 ms) e do mudo (30 007 ms) batem com os
de 18/10, feitos à mão: a sessão automatizada não muda esses casos.

**O que o «ninguém» decide:** ele leva o mesmo prazo do mudo (30 010 ms). O iPhone
separa dois grupos, e não quatro: **quem responde** (a recusa e a porta fechada,
em menos de 50 ms) e **quem não responde** (o servidor mudo e o IP sem máquina, em
30 s). A tela pode dizer «o servidor recusou ou está fechado» na hora e «não houve
resposta» depois de 30 s, mas não separa a recusa da porta fechada, nem o mudo do
IP vazio.

## 20/10: a tela bloqueada, à mão

**Pela automação, não vale.** Com a sessão do `safaridriver`, bloquear a tela
encerrou a própria sessão de automação: a página parou de mandar tudo no instante
do bloqueio (nem o «ficou oculta» chegou), a conexão caiu no servidor por tempo
esgotado, e ao desbloquear a sessão já não existia. Não dá para separar o que o iOS
faz com uma aba comum do que ele faz com a aba automatizada. Os casos de tela
bloqueada foram refeitos à mão, numa aba comum, com o servidor no Mac registrando.

| caso (aba comum, iOS 27.0.1) | bloqueada | o que aconteceu |
|---|---|---|
| sem áudio, 1ª | de 10:47:25 a 10:48:30 (~65 s) | o servidor fechou por tempo esgotado às 10:47:51, **26 s depois do bloqueio**; ao desbloquear, a página viu `WebTransportError` |
| sem áudio, 2ª | de 10:48:41 a 10:51:12 (~2 min 30 s) | o servidor fechou por tempo esgotado às 10:49:06, **25 s depois do bloqueio**; ao desbloquear, `WebTransportError` e «escrita falhou» |
| com o áudio do produto | de 10:51:24 a 10:54:38 (~3 min 14 s) | **sobreviveu**: 189 de 190 pings voltaram com a tela bloqueada, e a sessão foi fechada à mão depois |

**O que decide:** sem áudio, a página para de mandar ao bloquear, e o servidor a
derruba quando o ocioso esgota: a sessão de quem está só no texto **cai uns 25 s
depois de bloquear o iPhone**, mesmo num bloqueio curto. A reconexão ao voltar é
o caminho normal do celular sem áudio, e não a exceção. Com o áudio do produto
(`play-and-record`), a sessão segura a tela bloqueada: 2 min 14 s em 18/10 e 3 min
14 s em 20/10, com os pings passando.

(Os eventos de «ficou oculta» e «voltou à frente» chegam repetidos, um por caso já
aberto na mesma página; os tempos acima são os do servidor e os da primeira
ocorrência de cada um.)

## O que isso decide (pelo «Como ler o resultado» da F4)

- **A mesma chave serve.** O curto pode ter a chave do longo: o A abriu, e o A2
  também. O ADR 0057 segue pelo caminho simples, sem chave própria por janela de
  certificado.
- **A lista de hashes não é um limite prático:** 500 abriram no mesmo tempo de 1.
  O número do item 17 do contrato de fio pode ser folgado.
- **Só o longo é recusado**, como o Chromium já mostrava: no navegador, a âncora
  é o certificado curto.
- **Com o áudio do produto, a sessão segura a tela bloqueada** por mais de 2
  minutos, com os pings passando. É o caso de quem está numa sala de voz.
- **A recusa e a porta fechada não se distinguem** (os dois em 7 a 19 ms, com o
  mesmo erro), como a bancada já sugeria. Só o silêncio se separa, pelos 30 s.
  A tela não tem como separar «recusado» de «fechado», e diz isso.

## O que falta

- **Com áudio, os 5 min completos** (chegou a 3 min 14 s). Opcional: o que se queria
  saber, se o áudio segura a sessão bloqueada, já está respondido.
- *(Opcional)* os mesmos casos como web app da Tela de Início.
