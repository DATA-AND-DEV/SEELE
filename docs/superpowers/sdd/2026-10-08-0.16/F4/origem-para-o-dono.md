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

# A mesma regra sem o `.html`. O Cloudflare Pages responde ao `.html` com um 308
# para o nome sem ele, e a página chega por esse segundo caminho: com a regra só
# no primeiro, o `frame-ancestors` ia no redirecionamento e não na página, e o
# executor podia ser emoldurado por qualquer origem (medido na origem publicada,
# em 24/10/2026, na primeira publicação da 0.16).
/executor-de-mod
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

## 6. O que a página busca fora da origem

- **O catálogo de MODs** (`https://mods.seele.app.br`): o `catalogo.json`, as
  revogações e os arquivos de um MOD que o servidor exige, com as assinaturas.
  Ele já responde `Access-Control-Allow-Origin: *` (conferido em 21/10), e
  isso basta. Se um dia ele passar a nomear origens, a desta página é
  `https://web.seele.app.br`.
- **A prévia de um link** colado na conversa: a página busca a imagem pelo
  `fetch`, e ela só aparece quando o site dela libera CORS. A que não libera
  não tem prévia, e a conversa segue.
- **A sessão e a voz**: WebTransport direto ao `seeled` de cada servidor, pelo
  endereço e pelos hashes do link.
- **`#baixar`:** o link BAIXAR O APP DO SEELE da página leva a
  `https://seele.app.br/#baixar`. A página do site precisa da âncora `baixar`.

## 7. O pacote

    cargo xtask pacote-web

monta e confere o pacote em **`target/pacote-web/`**, e diz «o pacote sai»
quando as seis regras passam (o doc do `xtask/src/pacote_web.rs`). É essa
pasta, inteira, que vai à raiz do projeto `seele-web` no Cloudflare Pages:

- os arquivos de `apps/seele-app/ui/`;
- o `seele_web.wasm`, compilado com o perfil da página (D-web-5:
  `opt-level = "z"`, LTO e `strip = "symbols"`): em 21/10, 1 714 052 bytes,
  542 KB em gzip e 405 KB em brotli;
- o `seele_motor_web.wasm`, o motor dos MODs (o wasi-sdk já conferido pelo
  `cargo xtask wasi-sdk`; o passo nunca o baixa);
- o `_headers`.

O envio é do dono: pelo upload direto do Pages, ou pelo `wrangler pages deploy
target/pacote-web --project-name seele-web` com o token dele.

## 8. A conferência no ar, depois do envio

Nenhuma bancada roda o `.wasm` exato do pacote (elas precisam da `prova`, e o
pacote sai sem ela). Esta conferência é a que fecha isso.

**1. Os arquivos, como a origem os serve.**
- `https://web.seele.app.br/` e `/index.html`: os cabeçalhos da seção 3, e o
  corpo **igual ao do pacote** (o mesmo `sha256` do `target/pacote-web/index.html`:
  nada injetado);
- `/seele_web.wasm`: `200`, `content-type: application/wasm`, comprimido
  (`content-encoding: br` ou `gzip`), com o tamanho perto do da seção 7;
- `/seele_motor_web.wasm` e `/executor-de-mod.html`: `200`, e o
  `executor-de-mod.html` com a CSP do `<meta>` dele intacta.

**2. Uma entrada de verdade, pela página publicada, num servidor de teste.**
Num Chromium de PC, com o console aberto:
- colar o link do navegador do servidor de teste na ENTRAR EM, entrar, e ver o
  nome do servidor e os canais;
- mandar uma mensagem e vê-la confirmada;
- com uma segunda aba (ou outro aparelho), entrar na mesma sala de voz e ouvir
  uma à outra;
- compartilhar a tela numa aba e assistir na outra;
- mandar um anexo e abri-lo na outra;
- derrubar a rede do aparelho por uns segundos: a faixa da bateria aparece, a
  sessão volta sozinha, e uma mensagem escrita no meio sai e fica confirmada;
- num servidor que exige um MOD do catálogo de verdade: aceitar, e o MOD
  sobe. É a única prova da chave do catálogo embutida no pacote (a bancada usa
  uma chave de teste);
- o console sem recusa de CSP e sem erro de página.

**De onde vem o link do passo 2** (D-web-6): do app de quem hospeda. Depois
de HOSPEDAR, o diálogo A PORTA (e depois CONFIGURAÇÕES · A PORTA) tem, embaixo
do CONVITE, a linha NAVEGADOR com o botão «copiar link do navegador», até
quando ele vale, e o aviso de quem entra por ele (D-web-7, saída C):
- «QUEM TIVER ESTE LINK ENTRA PELO NAVEGADOR DE QUALQUER REDE»: serve para o
  teste de fora da rede;
- «PELO NAVEGADOR, SÓ QUEM ESTÁ NA SUA REDE ENTRA»: o servidor só é alcançado
  pelo ponto de encontro, que o navegador não usa. O teste então é da mesma
  rede, ou depois de abrir a porta UDP que o aviso nomeia no roteador.
