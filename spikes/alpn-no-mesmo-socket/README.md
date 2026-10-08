# spike `alpn-no-mesmo-socket` — o `seeled` atende o app e o navegador na mesma porta?

**Descartável.** Existe para responder uma pergunta do
[ADR 0057](../../docs/adr/0057-o-celular-entra-pelo-navegador.md) e morre com a
resposta. Desde 08/10, também prepara a **medição 3** do ADR («O que fica
pendente», item 3), que o dono faz no iPhone.

## A pergunta

O navegador entra por WebTransport (HTTP/3, ALPN `h3`) e aceita o certificado
pelo hash, o que exige validade de no máximo 14 dias. O cliente nativo entra por
`seele/1` com o certificado longo de sempre, cujo pino os clientes já guardaram
(ADR 0003). Um `quinn::Endpoint` só consegue servir os dois, escolhendo o
certificado pelo ALPN do ClientHello, e entregar a conexão `h3` ao `wtransport`?

A medição 3 acrescenta quatro perguntas, e só um iPhone as responde:
1. **O certificado curto com a mesma chave do longo** é aceito? O ADR decidiu a
   mesma chave porque é o único arranjo em que o `fp` pode voltar a ser a âncora
   no navegador. Se ela for recusada, cada janela ganha chave própria, e a lista
   de hashes vira a única âncora para sempre.
2. **Os temporizadores do `seeled`** (20 s de ocioso e 5 s de keepalive)
   seguram a sessão com a página oculta?
3. **Quantos hashes** o Safari aceita?
4. **Um servidor fora do ar demora de forma distinguível da recusa?** Se
   demorar, a página pode dizer «alguém respondeu e não é quem você esperava».

## Como roda

```sh
cd spikes/alpn-no-mesmo-socket
cargo build --release
python3 prova.py chromium      # bancada: os casos 1 a 3, e o ocioso medido matando o navegador
python3 prova.py safari-sim    # bancada: os casos 1 a 3 no Safari do Simulador (um simulador ligado)
python3 iphone.py              # a medição 3: o dono abre o endereço impresso no iPhone
```

O roteiro do dono, passo a passo, está em
[`medicoes-do-dono.md`](../../docs/superpowers/sdd/2026-10-08-0.16/F4/medicoes-do-dono.md).

O binário (`src/main.rs`) sobe:

| ponto | o que é |
|---|---|
| **A** | `seele/1` e `h3`. O certificado é escolhido pelo ALPN: o longo para `seele/1`, e para `h3` o curto **assinado pela mesma chave P-256 do longo**. O binário confere que o SPKI está nos dois antes de subir |
| **A2** | o mesmo, mas o curto tem **chave própria**. É o controle: se o iPhone recusar A e aceitar A2, a culpa é da chave repetida |
| **B** | só o longo, válido de 1975 a 4096, como o `seeled` de hoje |
| **MUDO** | um socket UDP que lê e descarta tudo: um servidor travado, ou um firewall que joga fora |
| **FECHADA** | uma porta sem ninguém: a máquina está ligada e o `seeled` não está rodando |

Todos usam os temporizadores do `seeled`: 20 s de ocioso e 5 s de keepalive
(`seele-proto/src/transport.rs:33`, `:39`). Cada conexão sai no stderr como uma
linha JSON: quando abriu, quando fechou e por quê, ou o alerta TLS com que o
navegador recusou.

`casos.js` tem os casos, escritos uma vez e rodados na bancada (`bancada.html`)
e no iPhone (`pagina-iphone.html`):
1. **os casos:** A com o hash do curto, A com um bit trocado, A com o hash do
   longo, A2 e B;
2. **a lista longa:** 1, 15, 30, 60, 100, 200 e 500 hashes, com o certo por
   último;
3. **fora do ar contra a recusa:** três recusas, três vezes a porta fechada, o
   mudo, e um IP sem ninguém;
4. **ficar aberto:** um ping por datagrama a cada segundo, até a sessão cair.
   Há uma versão com a configuração de áudio da rodada 5 do `voz-no-navegador`.

Todo WebTransport sai com `requireUnreliable: true`, como o produto vai sair
(ADR 0057). Na página do iPhone há uma caixa para desligar.

**Como ler o registro do servidor:** quando a página fecha a sessão, o servidor
anota `connection locally closed`, porque o `wtransport` fecha o QUIC ao receber
o fim da sessão. Quando a página some sem fechar, ele anota
`connection timed out`.

## O que mediu — 07/10/2026

Escrito fora do repositório durante a escrita do ADR 0057 e trazido para cá no
mesmo dia. Chromium 151, com um curto de chave **própria**:

| caso | resultado |
|---|---|
| cliente `seele/1` no endpoint A | recebe o certificado **longo** |
| `h3` com o hash do curto | abre em 2 ms; o datagrama de eco volta; `maxDatagramSize` 1024 |
| `h3` com o hash do curto, um bit trocado | recusado: `Opening handshake failed` |
| `h3` com o hash do longo (o servidor apresenta o curto) | recusado |
| `h3` com 60 hashes, o certo por último | abre em 31 ms |
| `h3` no endpoint B (só o longo, 1975–4096), hash certo | **recusado**: o certificado de hoje não serve ao navegador |

## O que mediu — 08/10/2026, a bancada da medição 3

No Mac, em localhost, no `d4eb6f1`. As versões são as do `Cargo.lock` do spike:
`quinn` 0.11.12, `rcgen` 0.14.10 e `wtransport` 0.7.2. O produto usa o `quinn`
0.11.11 e o `rcgen` 0.14.8, da mesma família, e a validade padrão do longo é a
mesma.

**1 · O certificado curto com a mesma chave do longo:**

| caso | Chromium 151 | Safari do Simulador, iOS 27.0 |
|---|---|---|
| A · curto da **mesma chave** do longo | **abre**, 10–13 ms; eco de datagrama e de quadro | **abre**, 56 ms; eco de datagrama e de quadro |
| A · um bit trocado no hash | recusado, 5–7 ms | recusado, 27 ms |
| A · o hash do longo (o servidor apresenta o curto) | recusado | recusado |
| A2 · curto de chave própria (controle) | abre | abre |
| B · só o longo | recusado | recusado |
| `maxDatagramSize` na página | 1024 | **65535**; o servidor diz 1295 ou 1358, então a página não pode confiar no número do navegador |
| o texto do erro | `WebTransportError: Opening handshake failed.` | `WebTransportError:` **vazio** |

No servidor, toda recusa do navegador chega como alerta TLS 46. O Chromium
manda `certificate unknown … CERTIFICATE_VERIFY_FAILED`, e o Safari manda
`TLS alert error`.

**2 · A lista longa:** os dois navegadores abriram com **1, 15, 30, 60, 100,
200 e 500** hashes, o certo por último. Nenhum dos dois mostrou teto até 500 na
bancada.

**3 · Fora do ar contra a recusa:** o tempo até o `ready` falhar. O texto do erro
é o mesmo em todos os casos de cada navegador.

| alvo | Chromium 151 | Safari do Simulador |
|---|---|---|
| recusa (hash errado), 3× | 58–202 ms | 5–7 ms |
| porta fechada (ICMP), 3× | 184–1437 ms | **3–4 ms** |
| mudo (lê e descarta) | 7,1–7,6 s | **30,0 s** |
| ninguém (192.0.2.1, TEST-NET-1) | 9,9–11,6 s | **30,1 s** |

O que isso diz, em localhost:
- **no Safari, «recusou» e «o `seeled` não está rodando» chegam iguais**: o
  mesmo erro, sem texto, em poucos milissegundos. A porta fechada chega até
  antes da recusa. O tempo não separa os dois;
- **o silêncio separa**: um servidor mudo ou uma máquina desligada levam o prazo
  inteiro do navegador (30 s no Safari, 7 a 12 s no Chromium);
- então a frase do ADR («alguém respondeu e não é quem você esperava») **não
  tem como ser dita a partir do tempo**. O que a página pode dizer é «ninguém
  respondeu», depois de esperar o prazo, ou «a conexão foi recusada: o link pode
  ter vencido, o servidor pode estar desligado, ou não é o servidor do link».

Falta o iPhone: na rede de verdade, o tempo de ida e volta soma aos dois, e o
ICMP de porta fechada pode nem voltar.

**4 · O ocioso, medido no Chromium:** com uma sessão aberta, o navegador foi
morto com `SIGKILL`, sem fechar nada. **O servidor desistiu 24,6 s depois**, com
`connection timed out`. São os 20 s de ocioso, recomeçados pelo keepalive de
5 s: pela RFC 9000, o temporizador recomeça no primeiro pacote que pede
confirmação depois do último recebido. Com o padrão do `quinn`, de 30 s, não
dariam 24,6 s. **A conexão adotada pelo `wtransport` herda os temporizadores do
`Endpoint`.**

**O caminho do dono, conferido no Simulador:** o `iphone.py` serviu a página por
HTTPS no IP da LAN do Mac. Testei:
- o aviso de certificado da página: «Mostrar Detalhes → visitar este site →
  Visitar Site»;
- o botão 1: os mesmos resultados da bancada;
- o botão 4, «ficar aberto»: 20 s de pings, depois «parar».

O registro do servidor e o da página foram gravados.

## O que não mede

Tudo o que a medição 3 pede ao aparelho:
- o iPhone de verdade, na rede de verdade;
- a página oculta com a tela bloqueada, com e sem áudio;
- a aba contra o web app instalado.

A bancada só diz que a página e o servidor funcionam **antes** de alguém levar
o iPhone para a mesa, para que um defeito no iPhone seja do iPhone.

O despacho por ALPN aqui é o do spike, e não o do `seeled` de produto, que ainda
não fala `h3` (Tarefa 2.4 do plano da F4).
