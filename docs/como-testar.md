# Como testar o SEELE

Estado em M4: o `connection` é uma TUI de verdade, contra um `seeled` de verdade. Texto
funciona sem placa de som; voz precisa de microfone e alto-falante.

## Subir um servidor

```sh
cargo build --release --bin seeled --bin connection
./target/release/seeled 127.0.0.1:8383
```

Ele imprime a impressão digital do certificado. Guarde: é o que o ADR 0003 pede
que você confira por outro canal se algum dia o cliente avisar que a chave mudou.

## Abrir o cliente gráfico

```sh
cargo tauri build --no-bundle     # ou `cargo build --release -p seele-app`
./target/release/seele-app
```

Instalador do macOS, se quiser:

```sh
cd apps/seele-app && cargo tauri build      # gera .app e .dmg em target/release/bundle
```

O app pede o servidor e o apelido, entra no primeiro VoiceRoom e abre a primeira Linha.
Barra de espaço fala enquanto segurada — aqui a janela relata a soltura de
verdade, então não há a trava que os terminais precisam (ADR 0016). Clicar no
VoiceRoom em que já se está sai dele. O deslizante de volume aparece ao apontar uma
linha do roster.

O app e o `connection` usam o mesmo `$SEELE_HOME`, então por padrão são **o mesmo
pessoa** — que é o que faz a mesma sessão ser retomável entre os dois. Para
serem duas pessoas, dois diretórios.

## Abrir o cliente de terminal

Noutro terminal:

```sh
./target/release/connection --server 127.0.0.1:8383 --nick seunome
```

Sem placa de som, ou numa VPS:

```sh
./target/release/connection --server 127.0.0.1:8383 --nick seunome --sem-audio
```

## Dois pessoas na mesma máquina

A identidade mora em `$SEELE_HOME` (ADR 0017). Dois clientes com o mesmo
`$SEELE_HOME` são a **mesma pessoa** — o servidor recusa o segundo apelido. Para
ser duas pessoas, dois diretórios:

```sh
SEELE_HOME=~/.seele-shinji ./target/release/connection -n shinji
SEELE_HOME=~/.seele-asuka  ./target/release/connection -n asuka
```

O padrão é `~/.config/seele`.

## O que fazer lá dentro

Aperte `?`. É o critério de aceite de M4 que isso baste.

O resumo: `i` escreve, `Enter` envia, `Esc` cancela, `Tab` troca de painel,
`j`/`k` navegam, `Enter` entra no VoiceRoom ou abre a Linha selecionada, `:q` sai.

Comandos úteis para olhar o sistema por dentro:

| Comando | O que mostra |
|---|---|
| `:sync` | RTT, jitter, perda e bitrate em números |
| `:audio` | taxas reais dos dispositivos e o modo de voz |
| `:tema` | desce um degrau na paleta — truecolor → 256 → 16 → mono |
| `:voz vad` | troca push-to-talk por ativação por voz |
| `:volume ayanami 40` | volume por pessoa |
| `:sobre` | versão do cliente e do protocolo |

`:tema` existe principalmente para você **conferir a degradação**: aperte três
vezes e veja se ainda dá para usar o cliente sem cor nenhuma. Se algo sumir, é
defeito — nenhuma informação pode ser transmitida só por cor.

## Falar

Modo Normal, barra de espaço.

Dependendo do terminal, isso se comporta de duas maneiras (ADR 0016):

- **kitty, foot, WezTerm, Ghostty:** segure a barra, solte para parar.
- **Terminal.app, iTerm2, e o que você alcança por SSH:** aperte para abrir,
  aperte de novo para fechar. Esses terminais não informam quando uma tecla é
  solta, e um microfone aberto por um evento que nunca recebe o seu par é um
  microfone que nunca fecha.

Nos dois casos a barra de telemetria diz qual é o estado. `m` ativa o A.T. Field
(mudo), `d` o isolamento total (surdo).

## O que ainda não existe

- `:conectar <host>` em execução. Reconectar de verdade exige derrubar uma
  conexão QUIC viva e uma thread de áudio rodando; reiniciar o processo faz isso
  certo, e o comando avisa em vez de fazer pela metade.
- Editar mensagem redesenha como uma linha nova em vez de reescrever a original.
- Busca (`/`) filtra a lista mas ainda não destaca o trecho encontrado.
- Voz nunca foi validada com dois microfones reais em duas máquinas reais. É a
  tarefa M1.15, e depende dos rigs de teste.

## Se algo der errado

O cliente restaura o terminal antes de imprimir qualquer erro — se ele sair
sozinho, a mensagem estará na tela normal, não perdida na tela alternativa. Se
o terminal ficar estranho mesmo assim, `reset`.

Recusa de conexão com apelido já usado significa que aquele nome pertence a
outra identidade naquele Server. Ou use `$SEELE_HOME` com a identidade certa, ou
escolha outro apelido.
