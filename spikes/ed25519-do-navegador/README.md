# spike `ed25519-do-navegador` — a assinatura do Safari passa no `verify` do `seeled`?

**Descartável.** Responde a medição 1 do
[ADR 0057](../../docs/adr/0057-o-celular-entra-pelo-navegador.md) («O que fica
pendente», item 1) e morre com a resposta.

## A pergunta

No celular, a identidade é uma chave Ed25519 do WebCrypto, não extraível. O
`spikes/alpn-no-mesmo-socket/` já tinha visto que a chave pública sai com 32 B
e a assinatura com 64 B. Ninguém tinha visto se a assinatura passa no
`verify` do `seeled` (`crates/seele-server/src/session.rs:594-599`). A dúvida
existia porque a assinatura do Safari é aleatória: a RFC 8032 deriva o nonce da
mensagem, e o Safari não. **Se não passar, a autenticação do celular não
funciona.**

A spec-mestre acrescenta uma pergunta (D-fio-3): o S2 assina
`rótulo ‖ fp do servidor ‖ nonce de 32 B` nos dois transportes. A F1
implementa; daqui sai a confirmação de que isso cabe no navegador.

## Como roda

```sh
cd spikes/ed25519-do-navegador
cargo build --release
python3 prova.py chromium safari-mac safari-sim
python3 prova.py iphone        # o dono, no aparelho: HTTPS na LAN
```

- **`src/main.rs`** é o verificador. Ele tem as linhas de `session.rs:550-599`
  sem mudança: `VerifyingKey::from_bytes`, `Signature::from_bytes` e
  `Verifier::verify`, com o `ed25519-dalek` 3.0.0 e o `curve25519-dalek` 5.0.0
  do `Cargo.lock` da raiz. Também diz o veredito do `verify_strict`, caso a F1
  troque por ele.
- **`pagina.html`** gera a chave como o celular faria
  (`generateKey({name: "Ed25519"}, false, …)`) e assina:
  - 100 nonces de 32 B, que são o `Challenge` de hoje;
  - 50 mensagens do S2, `rótulo ‖ fp(32) ‖ nonce(32)`, com um rótulo
    provisório (o de verdade é da F1);
  - a mesma mensagem 10 vezes;
  - quatro bordas: 0, 1, 1000 e 4096 B.

  Depois guarda a chave em IndexedDB, recarrega a página, lê a chave de volta,
  assina mais 20 nonces e apaga o banco.
- **`prova.py`** serve a página em `http://localhost:8093`, que é contexto
  seguro, e abre o navegador:
  - o Chromium pelo Playwright;
  - o Safari do Mac por `open -a Safari`;
  - o do Simulador por `xcrun simctl openurl booted`.

  Ela colhe tudo e passa pelo verificador.

**A prova contra a regressão está dentro da rodada.** De 20 assinaturas boas
saem 80 adulteradas: um bit trocado no `R`, um bit trocado no `s`, um bit
trocado na mensagem, e o `s` fora do intervalo. As 80 têm de ser recusadas.
Sem isso, um verificador que aceitasse tudo daria o mesmo «passa».

## O que mediu — 08/10/2026

No Mac (aarch64, macOS 27), no `d4eb6f1`. O `seeled` desse commit é o da
v0.15.1 publicada.

| | Chromium 151 (Playwright, headless) | Safari 27.0.1 do Mac | Safari do Simulador, iOS 27.0 (24A434) |
|---|---|---|---|
| contexto seguro | sim | sim | sim |
| a privada sai do `exportKey`? | **não** (`extractable: false`) | **não** | **não** |
| chave pública / assinatura | 32 B / 64 B | 32 B / 64 B | 32 B / 64 B |
| assinaturas distintas para a mesma mensagem, em 10 | **1**: determinística, RFC 8032 | **10**: **aleatória** | **10**: **aleatória** |
| **`verify` do `seeled`**: nonces de 32 B | **100/100** | **100/100** | **100/100** |
| **`verify` do `seeled`**: S2, `rótulo ‖ fp ‖ nonce` | **50/50** | **50/50** | **50/50** |
| `verify` do `seeled`: a mesma mensagem 10× | 10/10 | 10/10 | 10/10 |
| `verify` do `seeled`: bordas (0, 1, 1000 e 4096 B) | 4/4 | 4/4 | 4/4 |
| `verify` do `seeled`: a chave relida do IndexedDB depois de recarregar | 20/20, mesma pública, ainda não extraível | 20/20, idem | 20/20, idem |
| `verify_strict` | 184/184 | 184/184 | 184/184 |
| adulteradas aceitas | **0/80** | **0/80** | **0/80** |
| gerar a chave | 2,8 ms | 2 ms | 9 ms |
| assinar, p50 / p95 | < 0,1 ms / 0,1 ms | < 1 ms / 1 ms | < 1 ms / 1 ms |

O Safari arredonda o `performance.now()` a 1 ms, e por isso os números dele são
«menos de 1 ms».

O Simulador foi rodado quatro vezes, com o mesmo resultado nas quatro.

**Conclusão:**
- **A assinatura aleatória do Safari passa no `verify` do `seeled`**, e também
  no `verify_strict`. A autenticação do celular funciona com o código de hoje.
- **O S2 da F1 cabe no navegador** do jeito que a spec-mestre escreve: bytes
  arbitrários de rótulo, o `fp` de 32 B que a página tira do link, e o nonce de
  32 B. As 50 de cada navegador passam.
- **A `CryptoKey` não extraível sobrevive a recarregar a página** nos três
  navegadores. Fechar o web app e reiniciar o aparelho é a medição 4, do dono.

### Um achado à parte: o `crypto.subtle.verify` do WebKit erra na mensagem vazia

Nos dois Safaris, em todas as rodadas (quatro no Simulador e uma no Mac), o próprio
`crypto.subtle.verify` recusa a assinatura que o próprio `crypto.subtle.sign`
acabou de fazer sobre uma mensagem de **0 bytes**. O `ed25519-dalek` aceita essa
assinatura, inclusive no `verify_strict`, então ela está certa: quem erra é a
verificação do WebKit. Com 1, 32, 84, 1000 e 4096 B, o `verify` do WebKit
acerta.

Para o produto não muda nada: o nonce tem 32 B e o S2 tem mais. Mas a página
**nunca** deve usar o `verify` do navegador como autoteste, e um teste da
ponte que assine uma mensagem vazia vai falhar no Safari sem defeito nosso.

## O que não mede

- **O iPhone.** O Safari do Simulador e o do Mac são o mesmo WebKit 27, e o
  `prova.py iphone` existe para o dono confirmar no aparelho (ver
  `docs/superpowers/sdd/2026-10-08-0.16/F4/medicoes-do-dono.md`).
- **O aperto de mão inteiro.** O `seeled` de produto ainda não fala `h3`, e a
  página não fala o protocolo. O que se mediu é o passo que a página não
  controla: a assinatura que o navegador produz contra o `verify` que o
  servidor roda.
- **O rótulo do S2.** O daqui é provisório. O que se prova é a forma
  `rótulo ‖ fp ‖ nonce`, e não o texto.
- **O Android** (medição 5).

## Uma nota para quem for repetir

O certificado da página do modo `iphone` precisa de **curva nomeada**. O
`openssl` do macOS é o LibreSSL 3.3.6, e com `-pkeyopt ec_paramgen_curve:prime256v1`
sozinho ele grava a curva por parâmetros explícitos. A pilha TLS da Apple recusa
isso com `decode error`, e o `curl` do sistema também. O `prova.py` passa
`ec_param_enc:named_curve`.
