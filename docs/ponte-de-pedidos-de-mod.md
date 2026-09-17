# A ponte de pedidos: um MOD pergunta, e só quem perguntou ouve

> Protocolo **6**, API de MOD **2**. A decisão é o
> [ADR 0045](adr/0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md).

O produto ganha uma facilidade genérica e **nenhuma regra de MOD em Rust**:
pedido autenticado, resposta privada, execução limitada e estado persistente.
O que se faz com isso — fichas, mapas, votações, o que for — vive no JavaScript
de quem escreve o MOD, no repositório dele.

## O que existia antes, e por que não servia

Até aqui um MOD só era **avisado**: `aoAcontecer` recebia um momento da sala, e
o que ele quisesse devolver tinha de sair pelo mesmo barramento que todo mundo
lê. Duas consequências, e as duas são de desenho e não de implementação:

- **não havia como responder a uma pessoa só** — o que torna impossível
  qualquer coisa privada, de uma ficha a um voto;
- **não havia como o MOD saber quem perguntou** sem acreditar no que o cliente
  escreveu no corpo do pedido, que é acreditar no lado errado do fio.

## O contrato

`globalThis.SeeleMods.request(id, canal, objeto)` devolve uma `Promise` de JSON.
Do outro lado o host chama `aoPedir(contextoJSON, pedidoJSON)`, que devolve uma
string JSON.

**O contexto é escrito pelo servidor**, a partir da sessão autenticada e das
permissões de agora — `person`, `channel`, `admin`, `write`. Nunca a partir do
pedido. Um cliente que escreva `person` ou `admin` dentro do corpo não muda
coisa nenhuma, e é isso que
`crates/seele-conformance/tests/ponte_de_pedidos.rs` prende com um pedido
forjado de propósito.

## Os tetos, e por que cada um está onde está

| o quê | teto | por quê |
|---|---|---|
| pedido | 12 KiB | cabe num quadro de controle sem fatiar a ida |
| resposta | 1 MiB | acima disso é arquivo, e arquivo tem outro caminho |
| fragmento | 10 KiB | o teto do quadro de controle; o corte respeita fronteira de caractere |

O corte por fronteira de caractere não é elegância: cortar por bytes parte um
par substituto no meio e entrega texto quebrado. `partes()` tem teste de unidade
com `á🧙魔` justamente aí, e a bateria de conformidade prova a remontagem ponta
a ponta.

## Onde a resposta **não** passa

Ela não passa pelo histórico de conversa, não passa pelo barramento de eventos e
não vira transmissão. Sai pelo fluxo de controle **da conexão que pediu**. A
bateria prova a ausência: uma segunda pessoa na mesma sala faz o próprio pedido,
e a resposta alheia não aparece nela.

## Execução e gravação

O runtime nasce a cada pedido, com os mesmos limites de memória e de instruções
do runtime de eventos. Leitura e gravação acontecem sob a trava do banco, numa
thread de trabalho — JavaScript nunca roda na Tokio. **A resposta só sai depois
da gravação**, que é o que permite a um MOD usar revisões e recibos em vez de
repetir mutação por conta própria depois de um tempo sem resposta.

O despachante de eventos deixou de regravar quintais que não mudaram. Sem isso,
um evento sem tratador sobrescrevia o estado de um MOD que só recebe pedidos.

## Compatibilidade

O protocolo **6** acrescenta `ModRequest` e `ModReply` ao fim dos enums, e os
ordinais anteriores ficam onde estavam. A janela passa a ser 6 e 5; **a v4 sai**.
`api/v1.json` fica intacto e `api/v2.json` acrescenta `aoPedir` e a ponte da
janela. O limiar do anúncio de MODs continua **5**, e há um teste que prende
isso: se ele subisse junto, o anúncio voltaria a ficar dormente.

Esta entrega exige recompilar host e clientes. Não publique os bytes novos sob
uma versão antiga.

## Como levantar um servidor para desenvolver um MOD

```sh
cargo run -p seele-server --example servidor-com-mod -- \
    /caminho/absoluto/do/mod /caminho/absoluto/do/mundo
```

O exemplo não conhece MOD nenhum: a pasta do pacote é argumento. Ele recusa um
mundo que já existe, porque reabrir um mundo com código diferente do que o gerou
é como se perde a campanha que está lá dentro.
