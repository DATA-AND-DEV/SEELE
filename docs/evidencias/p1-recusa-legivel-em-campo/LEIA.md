# A recusa legível em campo: o app 0.15.1 publicado contra o `seeled` da 0.16

A medida de campo da P1 (plano F1b, «a recusa legível e o carimbo negociado, no
servidor»). Ela foi feita pela orquestradora em 15/10/2026, às 13:22, no Mac, a
pedido do dono. É a primeira vez que a recusa legível chega a um cliente
publicado.

## O que se mediu

- **O app** é o SEELE **0.15.1** instalado em `/Applications/SEELE.app`, a versão
  publicada (`CFBundleShortVersionString` = `0.15.1`; sha256 do
  `Contents/MacOS/seele-app` começando por `5faa8065`). Ele foi aberto com um
  `SEELE_HOME` vazio, para não haver pin nem preferência de antes, e com
  `--entrar seele://127.0.0.1:8392/?fp=…`.
- **O servidor** é o `seeled` da `versao/0.16` em `52b8278b`, compilado em debug.
  Ele subiu em `[::]:8392` com um banco novo e respondeu `pilha=Dupla`.
- **A régua** é a do plano: o app 0.15.1 tem de dizer «VERSÃO INCOMPATÍVEL», e
  não «não foi possível alcançar».

## O que se viu

**Na tela do app 0.15.1** (lida pela árvore de acessibilidade, com o `ax` da M3
em `docs/evidencias/m3-custo-dos-mods/ax.swift`), na casa, debaixo de CONECTAR e
HOSPEDAR AQUI:

> VERSÃO INCOMPATÍVEL COM ESTE SERVIDOR. Um dos dois lados está desatualizado — atualize o SEELE nas duas máquinas e tente de novo.

**No `seele.log` do app**:

```
INFO seele_core::enlace: desfiz o pin que este aperto de mão fixou: o aperto falhou depois de o verificador aceitar a chave …
WARN seele_ffi: could not reach the server motivo=Refused { reason: Incompatible } depois de 3 passos
```

O app classifica a resposta como recusa por incompatibilidade, e não como
servidor fora de alcance. Ele também desfaz o pin do aperto que falhou, então a
próxima visita não trata a chave como já conhecida.

**No registro do servidor**:

```
INFO seele_server: link unverified peer=[::ffff:127.0.0.1]:55118
INFO seele_server: session closed peer=[::ffff:127.0.0.1]:55118 error=handshake refused: first frame stamped 8: peer speaks protocol 8, oldest supported is 9
```

## Resultado

**Passou.** O servidor da 0.16 leu o carimbo 8 do primeiro quadro, recusou
antes de decodificar e respondeu num formato que a 0.15.1 lê. Do lado do app, a
recusa chegou como «VERSÃO INCOMPATÍVEL».

## O que esta medida não cobre

- **A outra direção:** o app 0.16 contra o `seeled` 0.15.1. Ela depende da P8,
  que está parada esperando o dono na sessão da F1. Sem a P8, o app 0.16 diz que
  não alcançou o servidor, e o launcher não sabe que versão abrir.
- **O Windows e o celular:** a medida foi só no Mac.
- **A frase:** ela manda atualizar «nas duas máquinas». Com o launcher da 0.16,
  quem tem o app 0.15.1 e encontra um servidor 1.0 precisa atualizar; o servidor
  não. A frase é da 0.15.1 publicada e não muda mais.
