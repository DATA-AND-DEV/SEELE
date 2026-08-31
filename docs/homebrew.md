# Instalar o `seeled` pelo Homebrew

## O que isto resolve, e o que não resolve

**Resolve o alerta do macOS para o `seeled`.** Um binário instalado pelo `brew`
não recebe o atributo `com.apple.quarantine` — é o navegador que o põe, e o
`brew` não é navegador. Sem o atributo, o Gatekeeper não tem o que reclamar.

Hoje o `install.sh` já contorna isso na unha, na linha 150:

```sh
xattr -d com.apple.quarantine "$BIN/seeled" 2>/dev/null || true
```

Um contorno que funciona e que a pessoa tem de confiar que é inofensivo, num
script que ela acabou de baixar da rede. Com a fórmula, ele deixa de ser
necessário em vez de ser automatizado.

**Não resolve o alerta do aplicativo gráfico.** Um `cask` aplica quarentena por
padrão, então o `.app` continuaria abrindo com aviso. Ali o conserto é
**notarização** — Apple Developer Program, US$ 99/ano — e ela conserta *todos* os
caminhos de instalação de uma vez, inclusive o download direto. Publicar um cask
sem notarizar seria trocar um contorno por outro, com mais peças para manter.

**Não substitui o `install.sh` no Linux nem no Windows.** O Homebrew existe nos
dois primeiros, e no Linux ele não traz vantagem de assinatura nenhuma — lá o
problema que o `xattr` resolve não existe.

## Onde a fórmula mora

Num **tap**, que é um repositório `homebrew-seele` da mesma organização:

```
DATA-AND-DEV/homebrew-seele
└── Formula/
    └── seeled.rb
```

E não no `homebrew-core`, que exige critérios de notoriedade — número de
estrelas, idade do projeto, presença em outros gerenciadores — que este projeto
ainda não atende. Um tap não tem porteiro e é o caminho normal para projeto novo.

Quem instala:

```sh
brew tap DATA-AND-DEV/seele
brew install seeled
```

## Como a fórmula é gerada

**Ela não é escrita à mão.** Uma fórmula carrega a versão e a soma SHA256 de cada
pacote, e as três mudam a cada release. Escrita à mão, ela nasce certa e fica
errada no release seguinte — e erra **calada**: o `brew install` baixa, confere a
soma, e falha com uma mensagem sobre integridade que parece adulteração e é
desatualização.

```sh
empacotar/publicar.sh 0.7.0          # monta entrega/ e gera SHA256SUMS
empacotar/homebrew.sh  0.7.0 > seeled.rb
```

O `homebrew.sh` lê `entrega/SHA256SUMS`, que o `publicar.sh` acabou de gerar, e
emite a fórmula. Um pacote que não foi montado — o `publicar.sh` aceita
`--pular` — simplesmente não ganha bloco, o que é o estado certo para um release
sem macOS.

O passo que falta automatizar é o commit no tap. Ele não está no `publicar.sh`
de propósito: escrever num segundo repositório a partir de um script de release
é o tipo de coisa que se faz com credencial de escrita ampla, e a decisão de dar
essa credencial ao release é de quem hospeda o projeto, não deste documento.

## A versão, conferida

O `test do` da fórmula roda `seeled --versao` e compara com a versão que a
fórmula declara. É o que a instalação promete: **este** binário, e não outro —
um pacote da arquitetura errada não chega a executar, e um montado do commit
errado responde um número que não bate.

Isso só passou a ser possível quando a pendência 30 fechou. Antes, o `seeled`
não tinha `--versao` e o teste se contentava com `--ajuda`, que prova que o
binário roda e não prova qual build é.

## O que conferir antes do primeiro tap

- `brew install --build-from-source` **não** vai funcionar: a fórmula instala um
  binário pronto, e compilar o `seeled` da fonte arrasta `libopus` e o resto. Se
  alguém quiser build da fonte, é `cargo build --release --bin seeled`, que o
  próprio `install.sh` sugere quando a plataforma não tem pacote;
- `brew audit --strict` reclama de `license :cannot_represent`. É verdade e é
  proposital: a licença do projeto ainda não foi definida (ver o README), e
  declarar uma que não foi decidida seria pior que declarar que não se sabe;
- o tap precisa do diretório `Formula/`. Um `.rb` na raiz funciona em versões
  antigas do Homebrew e não é mais o caminho documentado.
