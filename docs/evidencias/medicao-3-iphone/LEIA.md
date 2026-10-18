# Medição 3 no iPhone: o ALPN, o certificado curto e a página oculta

A medição 3 da F4 (`F4/medicoes-do-dono.md`, no `0.16/web`), feita pelo dono em
18/10/2026, das 13:52 às 13:57, num **iPhone com iOS 18.7, no Safari em aba**
(não como web app da Tela de Início), na mesma Wi-Fi do Mac. O servidor foi o
`spikes/alpn-no-mesmo-socket` (`iphone.py`, portas UDP 4500 a 4504, página em
8443), com **requireUnreliable** marcado. Os brutos ficam em
`spikes/alpn-no-mesmo-socket/resultados/` (fora do git: têm endereço e user
agent). Esta página tira os endereços.

## O que foi medido

| caso | aba (iOS 18.7) |
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
| ninguém (um IP sem máquina) | **não medido** (o caso não rodou) |
| sem áudio, tela bloqueada 30 s e 2 min | **não medido**: a sessão sem áudio ficou aberta 5 s, à vista, e foi fechada ao começar o caso com áudio |
| **com o áudio do produto** (`play-and-record`), tela bloqueada ~2 min 14 s | **sobreviveu**: oculta de ~13 s a ~143 s, 149 de 150 pings voltaram, e o servidor viu a sessão durar 152 375 ms até ela ser parada à mão |
| com áudio, 5 min bloqueada | **não medido** |
| como web app da Tela de Início | **não medido** |

No servidor, toda recusa do certificado é o iPhone abortando o aperto de mão
(`error 46: TLS alert`), e a sessão aberta tem `max_datagram` de 1295 bytes.

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

## O que falta, e leva uns 10 minutos com o servidor ainda no ar

1. **Sem áudio:** o caso «4 · ficar aberto, sem áudio», com a tela bloqueada 30 s e
   depois 2 min. É o que decide se quem está só no texto cai ao bloquear.
2. **Com áudio, 5 min** bloqueada.
3. **Ninguém:** o caso «3 · fora do ar contra a recusa» com um IP da rede que nenhuma
   máquina use, escrito no campo.
4. *(Opcional)* os mesmos casos como web app da Tela de Início.
