# A origem do web app: o que o dono configura

**A D-web-2 (15/10/2026):** a origem do web app é `https://web.seele.app.br`.
Ela é permanente, porque as identidades do celular e do navegador moram nela, e
trocá-la depois perde todas.

No produto, ela está escrita num lugar só: a
`seele_proto::uri::ORIGEM_DO_NAVEGADOR`. Um guarda reprova uma cópia à mão
(`a_origem_do_web_app_e_escrita_num_lugar_so`, em `apps/seele-app/tests/web.rs`).
Este documento é o que fica fora do repositório: **o host, o DNS e os
cabeçalhos são do dono**.

## 1. O DNS

Um registro para `web.seele.app.br` apontando para o host estático escolhido:
- no Cloudflare Pages, o domínio personalizado do projeto cria o `CNAME`;
- noutro host, o `CNAME` (ou o `A`/`AAAA`) que ele indicar.

**Nada mais nesse host.** Tudo o que roda em `https://web.seele.app.br` lê o
armazenamento das identidades. Uma página de outro projeto nessa origem teria
acesso às chaves de quem entrou por ela.

## 2. O HTTPS

O certificado é o que o host emite. Só HTTPS: o `http://` redireciona para o
`https://`, e nada é servido sem TLS.

## 3. Os cabeçalhos

A fonte é o arquivo `apps/seele-app/web/_headers`, no formato que o Cloudflare
Pages e o Netlify leem:
- **num desses dois hosts**, ele vai na raiz do pacote, e não há nada a
  configurar à mão;
- **noutro host**, configure exatamente estes cabeçalhos, por caminho. O bloco é
  uma cópia do arquivo, conferida pelo guarda `os_cabecalhos_da_origem_sao_um_so`.

```
# Os cabeçalhos da origem do web app (D-web-2), no formato `_headers` que o
# Cloudflare Pages e o Netlify leem. É o único lugar deles: as bancadas os leem
# daqui (`bancada/cabecalhos-da-origem.cjs`), e os passos do dono
# (`docs/superpowers/sdd/2026-10-08-0.16/F4/origem-para-o-dono.md`) os copiam
# inteiros, conferidos pelo guarda `os_cabecalhos_da_origem_sao_um_so`.
#
# A CSP da página vai só no documento, e não em `/*`: as regras que casam com o
# mesmo caminho se somam, e o `frame-ancestors 'none'` dela impediria a página
# de emoldurar o `iframe` dos executores, que traz a CSP dele.

/*
  Strict-Transport-Security: max-age=31536000
  X-Content-Type-Options: nosniff
  Referrer-Policy: no-referrer
  Permissions-Policy: microphone=(self), display-capture=(self), fullscreen=(self), camera=(), geolocation=(), payment=(), usb=()
  Cross-Origin-Opener-Policy: same-origin
  Cross-Origin-Resource-Policy: same-origin
  Cache-Control: no-cache

/
  Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self' https:; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'

/index.html
  Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self' https:; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'

/executor-de-mod.html
  Content-Security-Policy: frame-ancestors 'self'
```

**Por que cada um:**
- **a CSP da página**:
  - `script-src 'self' 'wasm-unsafe-eval'`: só os scripts da origem, e a
    compilação do `.wasm`. Sem `unsafe-inline`: a página não tem script
    embutido;
  - `connect-src 'self' https:`: a voz e a sessão vão por WebTransport direto ao
    `seeled` de cada servidor, e o endereço vem do link;
  - `img-src 'self' data:`: os retratos chegam como `data:`, como no desktop;
  - `frame-ancestors 'none'`: ninguém emoldura a página;
- **a CSP do `executor-de-mod.html`**: só `frame-ancestors 'self'`, porque a
  política de verdade dele está no `<meta>` do próprio arquivo, com o hash do
  script. Ele não pode receber a CSP da página: o `frame-ancestors 'none'` dela
  o impediria de ser emoldurado pela página;
- **`Strict-Transport-Security`**, por um ano, sem `preload` e sem
  `includeSubDomains`: a origem é um subdomínio, e o `preload` pede o domínio
  inteiro;
- **`X-Content-Type-Options: nosniff`**: um `.js` servido com outro tipo não
  roda (ver os tipos abaixo);
- **`Referrer-Policy: no-referrer`**: o link BAIXAR O APP DO SEELE não leva o
  endereço da página;
- **`Permissions-Policy`**: o microfone, a captura da tela (o compartilhar pelo
  PC) e a tela cheia, só para a origem; a câmera, a localização, o pagamento e
  o USB, para ninguém;
- **`Cross-Origin-Opener-Policy` e `Cross-Origin-Resource-Policy`**: outra
  origem não alcança a janela da página nem embute os arquivos dela;
- **`Cache-Control: no-cache`**: o navegador confere a cada abertura, e uma
  versão nova chega ao abrir a página de novo. É a frase da casca larga.

## 4. Os tipos de conteúdo

| extensão | tipo |
|---|---|
| `.html` | `text/html; charset=utf-8` |
| `.js` | `text/javascript; charset=utf-8` |
| `.css` | `text/css; charset=utf-8` |
| `.wasm` | `application/wasm` |
| `.svg` | `image/svg+xml` |
| `.json` | `application/json` |
| `.woff2` | `font/woff2` |
| `.png` | `image/png` |

## 5. Nada injetado no HTML

**No Cloudflare, para este hostname:**
- o **Web Analytics** (o beacon) desligado;
- o **Rocket Loader** desligado;
- a **Email Address Obfuscation** desligada;
- as **JavaScript detections** do Bot Management e o **Zaraz** desligados;
- nenhuma regra que reescreva o corpo do HTML.

A razão é uma só. Um script injetado é recusado pela CSP e enche o console de
recusas; o que passasse seria código de terceiro rodando ao lado da chave da
pessoa. As regras de cache não podem ignorar o `no-cache`.

## 6. O que não precisa de nada

- **CORS:** hoje a página só busca arquivos da própria origem (o `.wasm`, o motor
  dos MODs, o símbolo da marca). Se um dia ela buscar o catálogo de MODs, o
  `mods.seele.app.br` passa a precisar de
  `Access-Control-Allow-Origin: https://web.seele.app.br`.
- **`#baixar`:** o link BAIXAR O APP DO SEELE da página leva a
  `https://seele.app.br/#baixar`. A página do site precisa da âncora `baixar`.

## 7. O que vai para lá, e quando

O pacote é:
- os arquivos de `apps/seele-app/ui/`;
- o `seele_web.wasm` e o `seele_motor_web.wasm` (`cargo xtask web`);
- o `_headers`.

O passo que o monta e o entrega é a Tarefa 3.6 do plano da F4, e ainda não
existe. **O DNS e o host podem ser configurados já.** Publicar a página não
adianta antes da sessão pelo navegador (as Tarefas 2.4 a 2.6): sem ela, a página
abre e não conecta.
