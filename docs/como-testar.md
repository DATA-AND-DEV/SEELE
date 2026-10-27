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

## Desde a 0.17: o que mudou para testar (ADR 0063)

- A conexão é WebRTC. O `seeled` escuta **UDP** na 8383, onde STUN, DTLS e o ENC/3 dividem o mesmo socket. O firewall precisa da regra UDP de sempre.
- **A 0.17 não entra num servidor da 0.16, e o contrário também vale.**
  - Para testar contra um servidor 1.0, abra a 0.16 guardada pelo launcher.
  - Um `seeled` da 0.16 e um app da 0.17 na mesma máquina não conversam, e isso é o esperado: com link, a tela diz «versão mais nova» ou «abra a 0.16»; com o endereço digitado na 0.16, a tela diz que nada respondeu.
- A impressão do `seeled` é a mesma da 0.16 para o mesmo banco: os pinos não mudam.
- Os comandos rápidos, por crate (com `--target-dir` se houver builds em paralelo):

  ```sh
  cargo test -p seele-proto                 # o fio: versão, bytes-ouro das linhas 1 e 2, convite, ENC/3
  cargo test -p seele-enlace                # o transporte: envelope, canais, loopback
  cargo test -p seele-server
  cargo test -p seele-encontro
  cargo test -p seele-conformance           # o servidor em processo, de ponta a ponta
  cargo test -p xtask --test adr_index --test aviso_do_atualizador --test empacotamento
  cargo clippy -p <crate> --all-targets -- -D warnings
  cd fuzz && cargo +nightly fuzz build      # os sete alvos de fuzz
  ```

- A bateria inteira é a do `empacotar/publicar.sh`. O `cargo test --workspace` leva uns 5 min quente no Mac (`publicar.sh`, o teto da etapa).

### Reconectar o servidor inteiro e voltar à voz

O caso: sair do servidor inteiro e voltar, com alguém falando o tempo todo na sala. O defeito combina áudio antigo acumulado e falhas cíclicas depois dos primeiros segundos.

1. Deixe uma pessoa falando sem parar na sala, sem reconectar (o RECEPTOR é quem volta).
2. Na outra, saia do servidor inteiro, volte, e **espere pelo menos 12 s antes de entrar na sala de voz**. A espera é o que faz a fila de voz da conexão encher antes de a voz abrir, que era o gatilho.
3. Entre na sala e meça **os dois sentidos por pelo menos 3 min**. O primeiro som chegar não basta: confira que a voz que se ouve é a do presente (sem um atraso de segundos que vai diminuindo) e que ela não falha em ciclos depois dos primeiros segundos.

O teste automático do mesmo caso:

```sh
cargo test -p seele-conformance --test voz_de_quem_volta_e_senta_depois
```

O que foi medido na 0.17, e o que não foi:

- **Medido, Mac nativo:** com voz sintética (o aparelho de mentira) e um emissor contínuo, blocos contínuos com som nos dois sentidos por mais de 3 min, a profundidade da fonte em 20 ms, sem perda nem PLC registrados. É continuidade e profundidade de buffer, não latência física.
- **Medido, na build com o reparo:** sair do servidor inteiro, voltar, esperar 12 s e entrar na voz. O app descartou a fila velha (257 datagramas) antes do laço; sem o reparo, o teste de conformidade falhava (616 de 901 blocos com som) e, com ele, passa (900 de 900).
- **Medido antes do reparo, no mesmo Mac:** 15 min de vídeo H264 gerado e voz sintética nos dois sentidos, sem intervalos mudos; a imagem voltou ao trocar de vista. A vista pessoal e as amigas também passaram, contra uma amiga de bancada. O soak de 15 min não foi repetido na build final.
- **Medido no Windows real:** o app compilou (perfil dev) e 8 testes passaram; outros 8, do codec oficial, só com imagens geradas, também. Uma hora corrida (3600 s), em loopback e **sintética**, com voz e H264 gerados: 57299 quadros de tela contados no transporte, saída e reconexão do servidor inteiro, e o primeiro som depois de entrar de novo na voz (12 s depois de reconectar) em 62 ms, 0 erros. Nessa hora não houve captura de GUI nem WASAPI. Um smoke físico de 3 s a 48 kHz abriu a captura e a saída, sem erro de stream e com 2496 underruns: **não é áudio físico aprovado**. Ritmo físico de 600 s no Windows: 600 s completos, exit 0, em 60 intervalos de 10 s; só o primeiro teve falta (1056 amostras, 1 bloco inicial); dos 20 s aos 600 s, todos com falta 0 e anel cheio 0; reserva de 44 ms e 1 reposição. O aparelho a 48 kHz foi alimentado com silêncio, no compasso real: não é uma chamada completa nem latência física.
- **Medido, Linux ARM64 em Docker:** o app compilou e 8 testes do jitter passaram, sem GUI nem hardware.
- **Medido, voz depois de uma pausa da máquina, LAN de bancada (180 s):** fila em 40 a 60 ms, sem silêncio prolongado, voz de volta em 68 ms. O processo saiu com erro pelo gate de tela durante a pausa planejada: este ensaio **não é verde por inteiro**. O ramo do buraco pulado não foi exercitado nele (um teste dirigido o cobre). Um teste dirigido de pausa de 8 s mede a maior lacuna sem voz em até 540 ms nos primeiros 5 s depois da pausa, e não durante; não é garantia geral nem um limite de 1 s.
- **Não medido:** o Windows com GUI, captura de tela real ou uma chamada longa com hardware e o DSP (do hardware, só o smoke de 3 s e o ritmo de 10 min acima); NAT e IPv6 externos (sem relay: não há TURN na 0.17, por decisão do dono); o celular físico (o iPhone conectou, mas o espelhamento foi bloqueado); a cadeia de atualização empacotada, pela interface (não foi executada: a ferramenta de captura, ScreenCaptureKit 3811, bloqueou a interação com o Mac desbloqueado; o isolamento global não está provado, pois um scan amplo de Cookies falhou por permissão); o Linux com GUI ou hardware; o backport já teve patch, aplicação, 826 testes e o build do legado 0.16.1, e falta a cadeia empacotada. O teste automático acima não prova esses casos.

## Se algo der errado

Recusa de conexão com apelido já usado significa que aquele nome pertence a
outra identidade naquele Server. Ou use `$SEELE_HOME` com a identidade certa, ou
escolha outro apelido.

O `seele.log` é o primeiro lugar a olhar: ele fica em `$SEELE_HOME`, ou em
`~/.config/seele/` sem ela (no Windows, em `%APPDATA%\tech.datadev.seele\`).
