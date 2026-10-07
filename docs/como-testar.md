# Como testar o SEELE

Um `seeled` de verdade e o app de verdade, na mesma máquina. Voz precisa de
microfone e alto-falante; para testar entre dois computadores, o roteiro é o
`docs/teste-duas-maquinas.md`.

## Subir um servidor

```sh
cargo build --release --bin seeled
./target/release/seeled 127.0.0.1:8383
```

Ele imprime a impressão digital do certificado. Guarde: é o que o ADR 0003 pede
que você confira por outro canal se algum dia o cliente avisar que a chave mudou.

## Abrir o app

```sh
cargo tauri build --no-bundle     # ou `cargo build --release -p seele-app`
./target/release/seele-app
```

Instalador do macOS, se quiser:

```sh
cd apps/seele-app && cargo tauri build      # gera .app e .dmg em target/release/bundle
```

Escolha o apelido no perfil, no rodapé da entrada. Depois **CONECTAR**, digite
`127.0.0.1:8383` no campo de baixo e **ENTRAR**. Na primeira vez a sessão abre
dizendo `PRIMEIRO CONTATO — CHAVE FIXADA`, com a impressão digital: confira
contra a que o `seeled` imprimiu.

Com o `seeled` escutando em `[::]` ou em `0.0.0.0`, em vez de `127.0.0.1`, ele
imprime também um link `seele://…?fp=…` para colar no mesmo campo. O link leva a
impressão, e aí o app a confere sozinho.

## Duas pessoas na mesma máquina

A identidade mora em `$SEELE_HOME` (ADR 0017). Dois apps com o mesmo
`$SEELE_HOME` são a **mesma pessoa** — o servidor recusa o segundo apelido. Para
serem duas pessoas, dois diretórios:

```sh
SEELE_HOME=~/.seele-rafael ./target/release/seele-app
SEELE_HOME=~/.seele-carla  ./target/release/seele-app
```

O padrão é `~/.config/seele`.

## O que fazer lá dentro

Aperte `?`: a ajuda abre de qualquer tela, com as palavras e as teclas que
existem.

O que olha o sistema por dentro:

| Onde | O que mostra |
|---|---|
| o rodapé da sessão | ATRASO (a ida e volta), JITTER, DISTÚRBIO (a perda), CODEC, SINAL DA SALA e CAMINHO |
| CONFIGURAÇÕES · MICROFONE E SOM | os aparelhos, e «COMO O MICROFONE ABRE»: TECLA, VOZ ou ABERTO |
| o deslizante de uma linha do roster | o volume daquela pessoa |
| o `seele.log`, ao lado da identidade | o que o app fez e por quê — por exemplo, a taxa que o aparelho de áudio deu, ou por que o degrau 4 de um HOSPEDAR AQUI não subiu |

## Falar

Entre numa sala de voz e segure **ESPAÇO**, no modo TECLA. A janela relata a
soltura da tecla, então não há a trava que os terminais precisavam (ADR 0016).
O microfone e o fone, para cortar um ou outro, ficam no bloco OPERADOR, ao lado
do seu nome e antes da engrenagem das configurações.

## Se algo der errado

Recusa de conexão com apelido já usado significa que aquele nome pertence a
outra identidade naquele Server. Ou use `$SEELE_HOME` com a identidade certa, ou
escolha outro apelido.

O `seele.log` é o primeiro lugar a olhar: ele fica em `$SEELE_HOME`, ou em
`~/.config/seele/` sem ela (no Windows, em `%APPDATA%\tech.datadev.seele\`).
