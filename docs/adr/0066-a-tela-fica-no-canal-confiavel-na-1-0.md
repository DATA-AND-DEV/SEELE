# 0066 — Na 1.0 a tela fica no canal confiável, e o quadro continua indo à janela em base64

Status: **aceito por default** · medido em 10/11/2026, na `entrega/1.0`
Data: 2026-11-10

## Contexto

O mapa para a 1.0 (`docs/analise-para-a-1.0-2026-11-09.md`) recomendava duas mudanças no caminho do vídeo da tela, as duas por raciocínio e nenhuma por medida:

1. **tirar o vídeo do canal confiável** (`u`), porque num canal ordenado e confiável um pacote perdido segura todos os seguintes até a retransmissão;
2. **levar o quadro à WebView por IPC binário**, em vez de base64 dentro do JSON do evento `ScreenFrame`.

As duas foram medidas antes de serem feitas.

## O que a medida disse

### O canal

`docs/medidas/2026-11-10-tela-sob-perda.md`, sobre o SCTP real do enlace (`str0m` 0.24.1, `sctp-proto` 0.10.7) e um emulador de rede (`Cano`, em `crates/seele-enlace/src/teste.rs`):

- o canal **não confiável** entrega **menos** quadros que o confiável em toda perda medida (a 1% de perda, 15,6 contra 28 quadros por segundo): um quadro de tela são vários fragmentos, e a perda de um só joga o quadro inteiro fora, enquanto o controle de congestionamento do SCTP vale para os dois canais;
- baixar o tempo mínimo de retransmissão do SCTP de 1 s para 200 ms melhora a latência sob perda em 10 a 20%, dentro da variação entre rodadas. O que limita é a janela de congestionamento, e não o relógio. Não paga manter uma cópia alterada do `str0m`.

### A ponte

`JSON.parse` do evento, `atob` e a cópia byte a byte, medidos no V8 (Node, com a máquina sob carga de testes):

| Quadro | Custo por quadro |
|---|---|
| 4 KiB | 0,006 ms |
| 20 KiB | 0,024 ms |
| 65 KiB (quadro-chave de 1080p) | 0,075 ms |
| 250 KiB | 0,32 ms |

A 30 quadros por segundo de 20 KiB isso é menos de 1 ms por segundo de vídeo. O caminho binário do Tauri (`ipc::Channel` com corpo cru) troca isso por uma busca de ida e volta por quadro acima de 1 KiB.

## Decisão

1. **Na 1.0 a tela continua no canal confiável `u`**, como está publicado.
2. **O `ScreenFrame` continua levando o quadro em base64.** O custo medido não justifica mexer num caminho que só se prova com o app de verdade nas três plataformas.
3. **O que a medida não descarta é RTP** (com NACK, pedido de quadro-chave e estimativa de banda, que o `str0m` já tem). Isso é reescrever o transporte de mídia nas três pontas (cliente nativo, servidor e navegador) e fica como projeto próprio, depois da 1.0. O carimbo de tempo no quadro vai junto: o RTP já o traz, e pô-lo no fluxo de hoje pediria ao servidor filtrar por versão de quem assiste (o cabeçalho do quadro é fixo, 5 bytes, e o servidor repassa os bytes sem olhar).

## Consequências

- Sob perda a tela atrasa em vez de quebrar. É o comportamento da 0.19, agora sabido e medido.
- A medida da ponte foi feita no V8, e não no JavaScriptCore da WebView do macOS nem dentro do app. A ordem de grandeza (centésimos de milissegundo) deixa folga para a diferença entre motores, mas quem for reabrir este ponto mede lá.
- Custo de reverter: nenhum código mudou por esta decisão.
