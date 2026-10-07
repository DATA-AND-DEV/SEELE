# spike `alpn-no-mesmo-socket` — o `seeled` atende o app e o navegador na mesma porta?

**Descartável.** Existe para responder uma pergunta do
[ADR 0057](../../docs/adr/0057-o-celular-entra-pelo-navegador.md) e morre com a
resposta.

## A pergunta

O navegador entra por WebTransport (HTTP/3, ALPN `h3`) e aceita o certificado
pelo hash, que exige validade de no máximo 14 dias. O cliente nativo entra por
`seele/1` com o certificado longo de sempre, cujo pino os clientes já guardaram
(ADR 0003). Um `quinn::Endpoint` só consegue servir os dois, escolhendo o
certificado pelo ALPN do ClientHello, e entregar a conexão `h3` ao `wtransport`?

## Como roda

```sh
cd spikes/alpn-no-mesmo-socket
cargo build --release
python3 prova.py          # Chromium do Playwright
python3 prova_safari.py   # Safari do Simulador do iOS (um simulador ligado)
```

O binário sobe dois endpoints: **A** com os dois ALPNs e o certificado escolhido
por ALPN, e **B** só com o certificado longo, para ver o navegador recusá-lo.

## O que mediu — 07/10/2026

Escrito fora do repositório durante a escrita do ADR 0057 e trazido para cá no
mesmo dia. A rodada no Chromium foi repetida daqui (Chromium 151):

| caso | resultado |
|---|---|
| cliente `seele/1` no endpoint A | recebe o certificado **longo** |
| `h3` com o hash do curto | abre em 2 ms; datagrama de eco volta; `maxDatagramSize` 1024 |
| `h3` com o hash do curto, um bit trocado | recusado: `Opening handshake failed` |
| `h3` com o hash do longo (o servidor apresenta o curto) | recusado |
| `h3` com 60 hashes, o certo por último | abre em 31 ms |
| `h3` no endpoint B (só o longo, 1975–4096), hash certo | **recusado**: o certificado de hoje não serve ao navegador |

No servidor, a recusa chega como alerta TLS 46 (`certificate unknown`), e o
cabeçalho `Origin` da página chega no pedido WebTransport.

O Safari do Simulador do iOS 27 foi medido pela `prova_safari.py` na escrita do
ADR; a rodada não foi repetida daqui.

## O que não mede

Nada disto foi feito num iPhone. Falta repetir no aparelho, com o certificado
curto usando a mesma chave do longo, e medir o tempo de um servidor fora do ar
contra o da recusa (ADR 0057, «O que fica pendente»).
