# Pôr a v0.12.0 e a API 3 no ar

Quatro coisas vão ao ar, e a ordem entre elas **não é preferência**. Cada passo
diz o que quebra se ele for feito fora de ordem.

O estado de partida, conferido e não suposto em 19/09:

| peça | no ar hoje | vai para |
| --- | --- | --- |
| aplicativo | `v0.11.2`, commit `e90bcc16`, protocolo 6, MOD API 2 | `v0.12.0`, protocolo 7, MOD API 3 |
| catálogo | `api_oferecida: 2`, ESTILO 1.0.1 · MESA 1.2.1 · PERFIS 1.2.2 | `api_oferecida: 3`, os três em 2.0.0 |
| guia | a versão que descreve a API 3 pela metade | reescrito |
| chave de MOD | `32D58C50C0AB34E8` | a mesma — **nada a trocar** |

---

## 0. Antes de tudo: confira que a chave é a de produção

Ela vive em `~/.minisign/mods.key`. **A de desenvolvimento ainda está no
disco**, em `~/.minisign/mods-dev.key`, e até 19/09 o `chaves/LEIA.md` mandava
usar aquela.

Assinar o catálogo com a de desenvolvimento produz um `.minisig` que **todo
SEELE no mundo recusa**, e o modo como isso aparece é o indexador parecendo fora
do ar para todo mundo ao mesmo tempo. O comando não reclama; o erro só aparece
do outro lado.

A conferência não precisa da senha:

```sh
cd ~/SEELE-MODS-INDEXER && minisign -Vm publicado/catalogo.json -p chaves/mods.pub
```

Tem de dizer `Signature and comment signature verified`.

**Nunca `minisign -G -f`.** O `-f` sobrescreve a chave privada, e não há segunda
cópia dela em lugar nenhum.

---

## 1. O aplicativo, antes do catálogo

**Por quê primeiro:** a tela de MODs do cliente pega **a última versão da lista**
do catálogo, sem filtrar por API — conferido em `camada-mods.js`. Publicar o
catálogo antes faria todo cliente v0.11.2 no ar passar a ver «versão 2.0.0» nos
três MODs e falhar ao instalar, com `api-too-new`. A tela diria «não carregou», e
a causa — a versão — não apareceria em lugar nenhum que quem usa leia.

O contrário é inofensivo: um cliente v0.12.0 diante de um catálogo de API 2
simplesmente não acha MOD que possa instalar.

O caminho é o mesmo da v0.11.2, à mão:

```sh
cd ~/SEELE && ./empacotar/publicar.sh --conferir 0.12.0
```

`--conferir` roda só as conferências — alcance do Windows por SSH, Docker de pé,
ferramentas no PATH — e não compila nada. É o que se roda antes de sair para o
almoço: o pior resultado possível deste script é noventa minutos de Linux
emulado terminando em «não consegui alcançar o Windows».

**Falta o `gh` nesta máquina.** O script o usa para criar o rascunho do release.
`brew install gh && gh auth login` antes de começar.

Passando as conferências:

```sh
cd ~/SEELE && ./empacotar/publicar.sh 0.12.0
```

macOS, Windows e Linux, nessa ordem, cerca de noventa minutos. O release nasce
**rascunho** em `DATA-AND-DEV/SEELE-RELEASES`.

### Antes de publicar o rascunho

1. **Baixe e abra pelo menos um dos arquivos.** Quem decide que uma versão está
   pronta é uma pessoa que fez isso.
2. **Ponha o corpo** de `docs/notas-da-v0.12.0.md`.
3. **Escreva a linha do commit no corpo**, no formato que as outras releases
   usam: ``commit `998b0f098` ``. É dela que sai a conferência de «o que está
   naquela máquina» — sem ela, todo diagnóstico futuro volta a ser suposição.

### O CI está vermelho, e isto é um risco assumido

`cargo test --workspace` falha no Linux e no Windows, e `clippy` falha no
Windows, desde 18/09 — **antes** deste trabalho. A bateria passa no macOS: 84
suítes, 2283 testes, `saida=0`.

Não consegui ler os registros do CI desta máquina (sem credencial, e o `gh` não
está instalado). Então o que se sabe é: as duas outras metades nunca ficaram
verdes, e o empacotamento delas compila mas não roda a bateria. **Se a v0.12.0
sair assim, ela sai sem que Linux e Windows tenham sido verificados.**

Isso é uma decisão, não um detalhe. A alternativa é ler o CI e consertar antes.

---

## 2. O catálogo e o guia, juntos

Eles não se separam: `gerar.py` copia `site/` para dentro de `publicado/` na
mesma corrida em que monta e assina o catálogo. Uma corrida, um commit, um
deploy.

**As três avaliações de 2.0.0 já estão escritas**, com os commits que já estão
no ar:

| MOD | versão | commit |
| --- | --- | --- |
| `seele/estilo` | 2.0.0 | `b6dc1cd13` |
| `seele/perfis` | 2.0.0 | `e218ff6e5` |
| `seele/mesa`   | 2.0.0 | `73b627f92` |

O veredito de cada uma é `oficial`, e o texto delas está em
`avaliacoes/seele/*.toml`. **Leia antes de assinar** — o nível é seu veredito,
não meu.

O ensaio já foi feito: tudo o que o `gerar.py` faz **menos assinar** roda limpo.
Os três commits são alcançáveis, os manifestos concordam com as avaliações, os
hashes fecham e a regra append-only passa. A sua corrida com a senha não vai ser
a primeira a descobrir um problema.

```sh
cd ~/SEELE-MODS-INDEXER
export MINISIGN_PASSWORD='...'          # no seu shell, e em lugar nenhum mais
.venv/bin/python ferramentas/gerar.py --chave ~/.minisign/mods.key
unset MINISIGN_PASSWORD
```

Confira antes de comitar:

```sh
minisign -Vm publicado/catalogo.json -p chaves/mods.pub
python3 -c "import json;d=json.load(open('publicado/catalogo.json'));print(d['api_oferecida'],[ (m['id'],[v['versao'] for v in m['versoes']]) for m in d['mods']])"
```

Depois, o commit — **o commit é o deploy**:

```sh
git add publicado avaliacoes chaves indexador-de-mods.md
git commit -m "..." && git push
```

---

## 3. O catálogo novo volta para os vetores do SEELE

Depois que o `publicado/` estiver no ar, copie os quatro arquivos assinados para
`apps/seele-app/testes/` do SEELE. O `LEIA.md` de lá descreve exatamente quais.

**Por que isto não é burocracia:** aquele vetor é o que faz a bateria do produto
reprovar quando as duas metades divergem. Sem ele, as duas suítes ficam verdes
enquanto discordam — e ficaram, por meses.

---

## O que fica de fora, e é escolha

**Quem não atualizar vê três MODs quebrados.** Um cliente v0.11.2 passa a ver
«2.0.0» oferecida e falha ao instalar, sem caminho de volta para a 1.x que
funciona para ele. O aplicativo se atualiza sozinho, então a maioria sai disso
sem fazer nada — mas quem recusar a atualização fica assim.

Há um conserto possível **para o futuro, e não para quem já está no ar**: a tela
de MODs escolher a última versão cuja `api` este build entende, em vez da última
da lista. Isso ajudaria da v0.12.0 em diante e não ajuda em nada quem está na
v0.11.2, porque o código que escolhe é o que já foi publicado. Não está feito.

**Homologação em Windows e Linux.** Esta máquina é uma só, e a interação na
janela — digitar, arrastar, apertar — continua sem automação: o macOS recusa
acesso assistivo a este processo. O que foi exercitado no aplicativo nativo está
em `registro-de-execucao.md`, e o que ficou coberto só por bancada está dito
como prova mais fraca.
