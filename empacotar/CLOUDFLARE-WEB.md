# seele.sh pelo repositório no Cloudflare Pages

Configuração do projeto Pages com integração Git:

- raiz: raiz deste repositório;
- preset: nenhum;
- comando: `sh empacotar/cloudflare-web.sh`;
- saída: `target/pacote-web`;
- imagem: v3, Linux x86_64;
- variável: `SKIP_DEPENDENCY_INSTALL=1` (não há instalação npm neste build).

O comando só constrói e confere a página. A integração Git do Pages publica a
saída conforme as branches configuradas no projeto; executar o script localmente
não publica. Nenhuma credencial Cloudflare é necessária no script. Não adicionar
WASM, `target/`, SDK ou credenciais ao Git. A release manual e o CI existentes
continuam com seus comandos próprios.

## Ferramentas e saída

A imagem precisa fornecer CA certificates, `curl`, `tar`, `gzip`, `sha256sum` e um
linker C (`cc`) para o xtask nativo. Não precisa de GTK/WebKitGTK, Tauri CLI, Node,
wasm-bindgen ou SDK do desktop. Se faltar rustup, o bootstrap baixa **1.28.2** de
`static.rust-lang.org`, confere seu SHA-256 fixado e só então executa. O Rust usado
é **1.97.1**, igual a `rust-toolchain.toml`, com os alvos `wasm32-unknown-unknown`
e `wasm32-wasip1`; mudança do pin exige revisar o script.

`cargo run --locked -p xtask -- wasi-sdk` prepara o SDK **24.0**, usando o
SHA-256 Linux já fixado em `xtask/src/wasi_sdk.rs`. Em seguida, `pacote-web`
compila só `seele-web` e `seele-motor-web`, com `--locked`, e aplica os guardas
existentes de perfil/tamanho/exportações WASM, CSP, tokens e comandos da ponte.
A saída inclui `_headers`. O script também exige os dois WASM e `index.html` e
recusa alteração de `Cargo.lock`.

O SDK e as compilações reutilizam `target/` quando presente; o preparador do SDK
reconhece sua marca de verificação. Só reutilizar cache confiável deste projeto.
O script não exige cache persistente: a documentação do Pages não promete cache
de Cargo/target. Um build frio precisa baixar Rust/crates/SDK. Não apresentar um
build local quente como medida de duração no Pages.

Há uma trava por checkout em `target/.cloudflare-web.lock`, removida ao terminar
ou falhar. Depois de interrupção forçada, só remover a trava após confirmar que
não existe build vivo nesse checkout. Cargo preserva suas próprias travas; outros
comandos Cargo não devem rodar em paralelo neste checkout. `CARGO_BUILD_JOBS`
padrão é 2, incremental desligado. `CARGO_TARGET_DIR` externo é recusado, pois o
empacotador lê o motor no `target/` da raiz.

## Validação

Sem compilar ou baixar: `sh -n empacotar/cloudflare-web.sh` e
`python3 empacotar/test_cloudflare_web.py`. Os seis testes usam ferramentas falsas
em diretórios temporários; cobrem ordem, locks, propagação de falha e recusa de
instalador corrompido. Não substituem um build real.

Antes de ativar o domínio, conferir um build Git/preview no Linux/Pages: log do
SDK conferido, Rust 1.97.1, pacote verde, saída e cabeçalhos publicados. Build,
preview e smoke do ponto de encontro externo são evidências distintas. Não
considerar o ponto validado só porque a página compilou.

Referências: [imagem de build](https://developers.cloudflare.com/pages/configuration/build-image/)
e [cache](https://developers.cloudflare.com/pages/configuration/build-caching/).
