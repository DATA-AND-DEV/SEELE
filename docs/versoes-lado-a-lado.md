# Versões lado a lado — o contrato do manifesto e o que falta publicar

O [ADR 0046](adr/0046-toda-versao-continua-de-pe.md) decidiu que o app vira
launcher, e a emenda de 08/10/2026 o fez **por linha de servidor**: a versão do
servidor sobe junto com o protocolo e a API de MODs, o app tem número próprio,
o launcher guarda o app mais novo de cada linha, e cada linha tem o seu
diretório de dados. O app 0.16 fala o Servidor 1.0; a única linha além dela é a
do Servidor 0.15. Esta página é o que aquela decisão precisa por escrito para
virar código dos dois lados: **o formato do manifesto**, **onde a lista de
revogação mora** e **o que ainda falta no `SEELE-RELEASES`**.

O núcleo está em `crates/seele-lancador`. Ele não fala com a rede: recebe os
bytes do manifesto e do pacote, e responde qual executável roda, com que dados,
ou por que não.

## Desde a 0.17: três linhas, e a 1 vira legado (ADR 0063)

A 0.17 fala o **Servidor 2.0** (protocolo 10, linha `2`, transporte WebRTC) e não conversa com a 0.16 em nenhuma direção. O launcher passa a guardar duas linhas além da própria: a `1` (a 0.16) e a `0.15`.

- **A primeira abertura** (`apps/seele-app/src/versoes.rs`, `primeira_abertura`) lê o maior esquema de banco da pasta do app e copia a pasta para a linha legada dona dele.
  - Até 13 (`ESQUEMA_DA_0_15`) é a `0.15`, para quem pulou a 0.16. De 14 a 17 (`ESQUEMA_DA_0_16`, lido do `8faa3379`) é a `1`. Acima disso é `BancoJaMigrado`.
  - A chave fica de fora (ADR 0061).
  - Uma marca por linha (`lancador/primeira-abertura-da-linha-2`) impede a 0.17 de copiar a própria pasta na segunda abertura: ela escreve o mesmo esquema 17 da 0.16, e pelo esquema as duas pastas não se distinguem.
- **A busca em segundo plano** baixa, uma vez cada, toda linha legada que tem dados.
  - A marca da `0.15` mantém o nome que a 0.16 escreveu (`linha-legada-buscada`).
  - A da `1` é `linha-legada-buscada-1`.
- **Um link `v=1.0`** dá `ParaEntrar::Abrir` ou `Baixar` da linha `1`. Um `v=2.0` entra aqui mesmo. Uma recusa que nomeia a linha `"1"` leva ao mesmo lugar.
- **O manifesto** continua no schema 2. A entrada da 0.17 leva `unit { protocol: 10, server: "2.0", mod_api: 6, mod_apis: [6] }`, e `APIS_POR_LINHA` passa a `[("2",[6]), ("1",[6]), ("0.15",[5,4,3])]`.

### O que uma 0.16 já instalada vê da 0.17

Isto foi lido no código do `8faa3379`, e não medido numa 0.16 instalada.

- **O botão de atualizar da 0.16** mostra o `notes` do topo do `latest.json` no bloco «O QUE MUDA», antes de INSTALAR E REABRIR.
  - Desde a 0.17, esse `notes` é o bloco `AVISO-DO-ATUALIZADOR` das notas da versão, em texto puro, até 1200 caracteres, seguido da frase com o link (`empacotar/manifesto.py`, `xtask/tests/aviso_do_atualizador.rs`).
  - **A 0.16 só mostra o aviso, e não pede confirmação.** Quem não lê instala com um clique.
- **O launcher da 0.16**, diante de um link `v=2.0`, diz que precisa de uma versão mais nova (`MaisNova`). Na tela de HOSPEDAR, ele lista toda linha do manifesto, e por isso **oferece hospedar no Servidor 2.0 baixando a 0.17 ao lado**. Isso é coerente com o ADR 0046, e não foi medido.
- **A confirmação antes de atualizar** (cancelar, adiar e prosseguir) existe da 0.17 em diante (`apps/seele-app/src/atualizador.rs`). Na 0.16, ela só existe para quem instalar a 0.16.x que a traz, se ela for publicada. Não há promessa para trás.
- **Segurar a 0.16 sem oferta da 0.17** é decisão de publicação, e não de código: publicar a 0.17 sem a marca «latest» no SEELE-RELEASES mantém o `latest.json` que a 0.16 lê. O endpoint de toda versão é `releases/latest/download/latest.json`.

> **A renumeração foi feita em 2026-09-17, ao integrar este trabalho na
> `main`.** Esta página nasceu sobre o ramo dos MODs, onde a decisão do launcher
> era a **0045** e a dos MODs a **0044**; na `main` elas são **0046** e
> **0045**, porque um ADR próprio da `main` — o 0044, do portão da subida
> medida — entrou antes das duas e deslocou a numeração em um. O texto da
> decisão é idêntico nas duas bases.
>
> O que a versão anterior desta nota pedia a quem integrasse está feito: as
> referências desta página e as de `crates/seele-lancador` apontam para 0046 e
> 0045, e a linha do filtro de log em `apps/seele-app/src/main.rs` já nomeia o
> crate. Fica registrado porque um número de ADR errado aponta com confiança
> para a decisão de outra pessoa, que é pior que não apontar — e porque commits
> anteriores a esta data dizem «0045» querendo dizer o launcher.

---

## 1. Onde a lista de revogação mora

Havia duas frases no repositório e elas não cabiam juntas.

| onde | o que diz |
|---|---|
| ADR 0046 | a lista de revogação assinada fica **dentro do manifesto de versões** |
| ADR 0046 | «A mesma peça serve à remoção de um MOD do indexador, e as duas devem ser a mesma coisa — duas listas de revogação seriam duas chances de esquecer uma» |
| ADR 0045 | o catálogo do indexador é assinado com uma chave **separada** da do atualizador, porque «uma chave que atesta duas coisas deixa as duas se passarem uma pela outra, e a do 0026 autoriza instalar programa» |

Se «a mesma peça» significar «o mesmo arquivo», uma das três cai. Ou a lista de
MODs passa a ser assinada pela chave que autoriza instalar programa — e aí
tirar um MOD do ar exige a chave mais perigosa do projeto, e uma publicação de
release. Ou a lista de versões passa a ser assinada pela chave do indexador — e
aí a chave que não autoriza instalar programa passa a poder impedir que um
programa rode, que é a mesma autoridade pelo outro lado.

### A decisão

**Um código, dois documentos, e cada fato num lugar só.**

- **Uma implementação**: `crates/seele-lancador/src/revogacao.rs`. Um formato,
  um analisador, uma conferência de assinatura, uma forma de recusa. É isto que
  o ADR 0046 comprou ao dizer «a mesma peça»: o custo de esquecer uma das duas
  é o custo de manter duas implementações, e não o de haver dois arquivos.
- **Dois documentos, cada um na casa do seu assunto e assinado pela chave que
  manda naquele assunto**:

  | assunto | onde mora | chave |
  |---|---|---|
  | versões revogadas | dentro do manifesto de versões, no release | a do atualizador (ADR 0026) |
  | MODs removidos | dentro do catálogo do indexador | a do indexador (ADR 0045) |

- **Nenhum fato aparece nos dois.** Uma versão revogada está só no manifesto;
  um MOD removido está só no catálogo. Não há lugar em que se possa perguntar a
  mesma coisa e receber duas respostas, que é o que «fonte única» tem de
  significar aqui.

O tipo `Revogacao` tem um campo `target` e não um campo `version`, e é por
isso: ele é um identificador de versão quando a lista veio do manifesto, e um
identificador de MOD quando ela vier do catálogo.

**E a lista que vale é exigida, não oferecida.** A lista que decide não é o
bloco que o manifesto trouxe: é o resultado de `Deposito::conciliar_revogacoes`
— o tipo `Vigente` —, que confronta o que chegou com o que a máquina guardou.
`Resolvedor::novo` **pede um `Vigente`**, e só a conciliação constrói um. Era o
buraco que faltava: enquanto a conciliação era uma chamada opcional, quem
esquecesse dela voltava a ler o bloco do manifesto, e um manifesto servido sem
bloco virava «nada revogado» — exatamente o ataque que a conciliação existe
para fechar. Agora esse caminho não compila, o que é mais forte que um teste
reprovando quem esquecer.

### O que esta decisão custa

O ADR 0046 disse «duas listas de revogação seriam duas chances de esquecer
uma», e essa frase continua verdadeira sobre **operação**: revogar uma versão e
remover o MOD que dependia dela são dois atos, em dois lugares. O que esta
decisão evita é a chance de esquecer **no código**, que é a que se paga em
defeito silencioso. A operacional se paga com um procedimento escrito, e ele é
a seção 4 desta página.

---

## 2. O manifesto de versões

Mora onde sempre morou: `releases/latest/download/latest.json`, no release do
GitHub do `SEELE-RELEASES`. Nenhum serviço novo a hospedar.

**Híbrido, desde a 0.16.** O topo continua no formato do `tauri-plugin-updater`
— é o que o atualizador da 0.15.1 lê, e ele exige `url` e `signature` em todo
alvo do topo (`updater.rs:1388-1398` do plugin 2.10.1). Ao lado vão `schema: 2` e
`versions[]`, que o launcher lê. Quem escreve é o `empacotar/manifesto.py`, e o
teste `apps/seele-app/tests/manifesto_hibrido.rs` lê o arquivo gerado com o
`RemoteRelease` do próprio plugin e com o `Manifesto` do launcher.

```json
{
  "version": "0.16.0",
  "notes": "…",
  "pub_date": "2026-10-20T00:00:00Z",
  "platforms": {
    "darwin-aarch64": { "url": "…/SEELE_0.16.0_aarch64.app.tar.gz", "signature": "…" },
    "windows-x86_64": { "url": "…/SEELE_0.16.0_x64-instalador.exe", "signature": "…" }
  },
  "schema": 2,
  "versions": [
    {
      "version": "0.16.0",
      "pub_date": "2026-10-20T00:00:00Z",
      "notes": "…",
      "unit": { "protocol": 9, "mod_api": 6, "server": "1.0", "mod_apis": [6] },
      "platforms": {
        "darwin-aarch64": {
          "url": "…/v0.16.0/SEELE_0.16.0_aarch64.app.tar.gz",
          "signature": "<base64 do .sig>",
          "sha256": "<64 dígitos hexadecimais>",
          "executable": "SEELE.app/Contents/MacOS/seele-app"
        },
        "windows-x86_64": {
          "url": "…/v0.16.0/SEELE_0.16.0_x64-portatil.tar.gz",
          "signature": "<base64 do .sig>",
          "sha256": "<64 dígitos hexadecimais>",
          "executable": "SEELE.exe"
        }
      }
    },
    {
      "version": "0.15.2",
      "unit": { "protocol": 8, "mod_api": 5, "server": "0.15", "mod_apis": [3, 4, 5] },
      "platforms": { "…": "…" }
    }
  ],
  "revocations": {
    "document": "<base64 dos bytes exatos do documento abaixo>",
    "signature": "<base64 do .sig daqueles bytes>"
  }
}
```

**O topo e o `versions[]` apontam para pacotes diferentes no Windows, de
propósito.** O topo é do atualizador, que troca a instalação de Program Files
pelo `-instalador.exe`. O `versions[]` é do launcher, que guarda a versão ao lado
de outra e precisa do `-portatil.tar.gz` (a mesma carga do instalador num
tar.gz). O portátil **nunca** vai para o topo: o atualizador da 0.15.1 o mandaria
executar como instalador. O `.deb` do Linux também não entra no `versions[]`,
porque instala no sistema.

**`unit.protocol` e `unit.mod_api` continuam obrigatórios.** O `Unidade` da
0.15.1 os exige, e um manifesto sem eles seria recusado inteiro por quem ainda
está nela. `unit.server` (a versão do servidor, que dá a linha: `1.0` e `1.3` são
da linha `1`, e `0.15` é a linha `0.15`) e `unit.mod_apis` são acréscimos, e
chegam opcionais pela regra do `schema`.

**O executável do Mac é `SEELE.app/Contents/MacOS/seele-app`**, medido no `.app`
da 0.15.1 (`CFBundleExecutable`). Até 08/10/2026 esta página dizia
`…/MacOS/SEELE`, e estava errada.

E o documento de revogação, cujos **bytes exatos** são o que a assinatura
cobre:

```json
{
  "schema": 1,
  "issued_at": "2026-09-20T00:00:00Z",
  "revoked": [
    {
      "target": "0.10.4-2",
      "defect": "descrição em uma frase do que a versão fazia de errado",
      "fixed_in": "0.10.4-3"
    }
  ]
}
```

### As regras que não são óbvias no exemplo

**A ordem de `versions` é o contrato, e a primeira é a mais nova.** Dentro de
uma linha, a ordem é a de publicação, e não há ordenação de versões calculada
em lugar nenhum: para o semver, `0.10.5-1` é uma pré-lançamento de `0.10.5` e
portanto anterior a ela; no `SEELE-RELEASES`, a `v0.10.5-1` saiu **depois** da
`v0.10.5`. **Entre linhas, a mais nova vem primeiro**, e aí há ordem calculada,
porque o número da linha é nosso e não tem sufixo: a 1 vem depois da 0.15 em
qualquer leitura. É o que faz um conserto da 0.15 publicado depois da 0.16 ficar
atrás dela, e o topo do Tauri continuar sendo o da linha mais nova.

**Campo desconhecido é ignorado; `schema` desconhecido é recusado.** É o
contrário da regra do `mod.json`, e de propósito. Um `mod.json` é escrito por
uma pessoa uma vez, e ali a recusa é o único retorno que o formato dá. Um
manifesto de versões é lido por **todas as versões já instaladas** — se um
campo novo o fizesse ser recusado, acrescentá-lo trancaria fora exatamente as
instalações antigas que o ADR 0046 promete manter de pé. Então mudança aditiva
não sobe o `schema`, e subir o `schema` é declarar que quem está instalado
deixou de conseguir ler.

**O manifesto continua sem assinatura; a lista de revogação tem a dela.** O
argumento de `empacotar/manifesto.py` — «cada entrada carrega a assinatura do
pacote a que se refere, então trocar o manifesto só troca qual pacote é
oferecido, e o pacote trocado é recusado na conferência» — continua inteiro
para as versões, e **não alcança uma revogação**. Uma revogação é uma afirmação
sobre o que *não* deve rodar; a forma de atacá-la é removê-la, e depois disso
não há pacote nenhum a conferir. Por isso ela vem assinada, e por isso o
launcher **guarda em disco o envelope inteiro** da última lista aceita —
bytes e assinatura, não só o carimbo (`Deposito::conciliar_revogacoes`). As
duas regras que saem disso: uma lista mais velha que a conhecida é recusada, e
**o ato junto com ela**; e uma lista que sumiu do manifesto não vira «nada
revogado», porque vale a guardada.

**O documento de revogação é assinado como bytes e viaja em base64.** Assinar
«o objeto JSON» exigiria uma canonicalização, e duas serializações do mesmo
objeto que difiram num espaço produzem assinaturas diferentes. Quem gera e quem
lê não combinam espaços.

**`sha256` não substitui a assinatura, e a assinatura não o dispensa.** Ele
responde «o download veio inteiro?», que é uma pergunta sobre a rede, e dá o
motivo certo — «baixe de novo» — para um arquivo truncado. A assinatura
responde «isto veio de nós?», e é a que recusa um pacote adulterado. Confundir
as duas manda a pessoa insistir contra um pacote adulterado, ou desistir de um
truncado.

**`executable` é declarado e não adivinhado.** O caminho do executável dentro
da instalação muda com o sistema, e um launcher que o adivinha erra calado num
sistema só — que aqui é o pior defeito possível, porque ele aparece na máquina
de outra pessoa. Sem o campo, o launcher recusa com `ExecutavelNaoDeclarado` em
vez de chutar, e recusa **antes** de instalar: uma instalação publicada sem se
saber o que iniciar é uma que só falha no dia em que alguém a abre.

**`executable` é relativo à raiz da instalação, e o launcher confere isso.**
Nada de caminho absoluto, nada de `..`, nada de dois pontos. O motivo é o
parágrafo acima deste: o manifesto **não é assinado**, e o argumento que
sustenta isso — «cada entrada carrega a assinatura do pacote a que se refere» —
vale para *qual pacote é oferecido* e não alcança um campo que nomeia *o
programa a rodar*, exatamente como não alcança uma revogação. Sem a
conferência, um `"executable": "/bin/sh"` faria o `join` descartar a instalação
inteira, e um `"../0.10.4/bin/seele"` faria a versão escolhida iniciar o
binário de outra — quem serve o manifesto passaria a decidir o que roda, no
lugar da versão selecionada. A recusa é por pacote e tem motivo próprio
(`ExecutavelInvalido`); ela **não** derruba o manifesto inteiro, porque um erro
de empacotamento num alvo não pode trancar fora as versões já instaladas. O
identificador de versão tem um tipo próprio pela mesma razão, e este campo
ganhou o dele: `crates/seele-lancador/src/executavel.rs`.

**A régua do `executable` é a do sistema mais estrito, em todo sistema.** O
manifesto é um só para as três máquinas e o alvo é escolhido por quem instala,
não por quem publica. Um `"executable": "bin/aux"` ou um nome terminado em
ponto passa no Mac, instala, e falha só ao abrir no Windows — na máquina de
outra pessoa, dias depois, sem dado nenhum junto. Por isso o launcher recusa,
nos três sistemas: os nomes de dispositivo que o Windows reserva (`CON`,
`AUX`, `NUL`, `COM1`…`LPT9`, com ou sem extensão), os caracteres que ele não
aceita em nome de arquivo (`< > : " | ? *`) e qualquer componente terminado em
ponto ou espaço. A lista de nomes reservados é **uma só** no código, partilhada
com a conferência do identificador de versão: duas listas seriam duas respostas
para a mesma pergunta, e a que ficasse para trás recusaria menos sem ninguém
notar.

**Um pacote a que falta `url` ou `signature` custa aquele alvo, e não o
manifesto.** É a mesma regra do `executable`, e vale escrever porque a decisão
oposta é a que o `serde` toma sozinho: com os campos obrigatórios, um
`platforms` de um alvo publicado sem assinatura faria a leitura do arquivo
inteiro falhar, e com ela sumiriam da lista **todas as versões já instaladas**
— inclusive a que a pessoa está rodando naquele instante. Então os campos são
lidos como opcionais e são **privados**: quem precisa deles passa por um
acessador que devolve «não declarado», e instalar sem assinatura é recusado com
`AssinaturaNaoDeclarada`, que é coisa diferente de `AssinaturaRecusada`. Ali
houve conferência e ela reprovou — não adianta tentar de novo; aqui não houve o
que conferir, e o defeito é do manifesto. O caminho que não pode existir é o
terceiro: conferir contra uma assinatura vazia, que é não conferir.

O que a conferência não cobre é o **conteúdo** do pacote — uma ligação
simbólica dentro da instalação aponta para onde quem a criou quis, e quem a
criou assinou o pacote. Ali quem responde é a assinatura do ADR 0026.

---

## 3. As lacunas, e o que a 0.16 fechou

Nada abaixo foi editado por esta tarefa: o `SEELE-RELEASES` é outro
repositório. O que está publicado hoje, conferido em 10/09/2026, é o
`latest.json` do `tauri-plugin-updater`: uma versão, `platforms` com `url` e
`signature`, sem `schema`.

**O launcher lê esse formato**, como um manifesto de uma publicação só, e é de
propósito: recusá-lo diria «nenhuma versão publicada» num dia em que há onze
delas na página — a frase errada, do jeito que o ADR 0026 já errou uma vez. O
que se perde enquanto as lacunas não forem fechadas está na coluna da direita.

| # | lacuna | onde se conserta | sem isso |
|---|---|---|---|
| 1 | `latest.json` descreve uma versão, não todas — **fechada na 0.16** (`manifesto.py` híbrido) | `empacotar/manifesto.py`, que é o único lugar onde a regra do manifesto mora, e o `release.yml` que o chama | o seletor de versão tem uma opção só, e o ADR 0046 vale no código e não no produto |
| 2 | não há `sha256` por pacote — **fechada na 0.16** | mesmo arquivo; é uma linha por pacote | um download truncado é recusado pela assinatura, com o motivo errado: «não veio de nós» em vez de «veio pela metade» |
| 3 | não há `executable` por pacote — **fechada na 0.16**, com o do Mac corrigido para `SEELE.app/Contents/MacOS/seele-app` | mesmo arquivo, uma constante por alvo | o launcher não sabe o que iniciar, e recusa com `ExecutavelNaoDeclarado` — ao instalar e ao abrir. Até a 0.16 era o único caminho que toda máquina encontrava, e o «GUARDAR A MAIS NOVA» nunca instalou nada por causa dele. O valor por alvo é `SEELE.app/Contents/MacOS/seele-app` e `SEELE.exe` (o portátil), sempre **relativo** à raiz da instalação: o launcher recusa caminho absoluto ou com `..` |
| 4 | não há `unit` (protocolo e API de MOD) por versão — **fechada na 0.16**: o `manifesto.py` lê `PROTOCOL_VERSION`, `MOD_API_VERSION`, `APIS_ACEITAS` e `VERSAO_DO_SERVIDOR` do código compilado, e um guarda confere a leitura contra as constantes | precisa sair do build: são `seele_proto::version::PROTOCOL_VERSION` e o número da API de `api/v1.json` | a conferência de coerência do ADR 0046 não acontece, e a defesa volta a ser só o `negotiate` — que é justamente o que o ADR tirou do posto principal |
| 5 | não há bloco `revocations`, e não há procedimento para emitir um | uma ferramenta nova ao lado de `empacotar/manifesto.py`, que assina o documento com a mesma chave do atualizador | não há como revogar uma versão furada, que é a única exceção ao «toda versão para sempre» |
| 6 | o manifesto lista só o release corrente — **fechada na 0.16**: `--anterior`, que o `publicar.sh` e o `release.yml` preenchem com o `latest.json` publicado | o `release.yml` precisa **ler o manifesto anterior** e acrescentar a versão nova no topo, em vez de escrever um do zero | cada release apaga a lista de todas as anteriores, e o launcher esquece o que existe |
| 7 | os pacotes das versões antigas estão em releases antigos, e o manifesto único vai apontar para lá | nada a fazer no formato: as URLs já são absolutas por release | — |

A número 5 continua aberta: emitir uma revogação «pode vir depois» (spec 0.16,
§2, F6). O lado que **lê** a lista está ligado desde a 0.16.

### Publicar a linha legada, e um conserto dela depois

**A 0.15.2** sai do branch `linha/0.15`, antes do app 0.16, como **atualização
normal para todos** (D-linha-2, que revê a D-linha-1: o conserto da sala cheia
não podia ficar só para o launcher):

1. `./empacotar/publicar.sh 0.15.2`, sem o `--linha-legada`. Ela vira o
   `latest`, e quem está na 0.15.1 a recebe pelo atualizador.
2. O `latest.json` dela já sai híbrido: o topo aponta para a 0.15.2 (o
   instalador do Windows e o `.app.tar.gz` do Mac), e `versions` = [0.15.2], na
   linha 0.15. O atualizador da 0.15.1 (`tauri-plugin-updater` 2.10.1) lê esse
   arquivo: medido em 09/10/2026 com o `latest.json` que o `manifesto.py` do
   `linha/0.15` escreve, e é o que `apps/seele-app/tests/manifesto_hibrido.rs`
   prende.

**A 0.16.0** sai pelo `publicar.sh 0.16.0` de sempre. O `latest.json` publicado
naquele dia é o da 0.15.2, com `schema` 2, e o `manifesto.py` o acumula: a 0.16
sai com a linha 1 no topo e a 0.15.2 em `versions`. O `SEELE_MANIFESTO_ANTERIOR`
não é preciso.

**Um conserto da linha legada depois da 0.16** (uma 0.15.3) só chega ao launcher
se o `latest.json` do release `latest` for refeito com ele. O `manifesto.py` já
preserva o topo quando a linha nova não é a mais nova, então o arquivo que ele
escreve para a 0.15.3 serve também ao release `latest`: o dono sobe esse arquivo
nos dois releases. Não está automatizado, de propósito: é a única publicação que
mexe num release já publicado.

### O que **não** é lacuna

- **A chave.** É a mesma do ADR 0026, já está no `tauri.conf.json`, e o
  `seele-lancador` a lê pelo mesmo caminho — há um teste que prova isso.
- **O lugar.** `releases/latest/download/latest.json` continua servindo, e a
  propriedade que o ADR 0026 comprou continua valendo: enquanto uma pessoa não
  publicar o rascunho à mão, nenhum app enxerga a versão nova.
- **A conferência de assinatura.** É o `minisign-verify`, o mesmo que o
  `tauri-plugin-updater` usa, com os mesmos formatos de chave e de assinatura.

---

## 4. Revogar uma versão, quando for preciso

Procedimento, para o dia em que ele for necessário e ninguém lembrar:

1. Publicar a versão que **corrige** o defeito. Uma revogação sem saída deixa
   quem está nela sem para onde ir, e a recusa que o launcher escreve nomeia a
   versão corrigida.
2. Escrever o documento de revogação com a versão nova em `fixed_in`, e o
   `issued_at` **maior** que o da lista anterior — o launcher recusa uma lista
   que retrocede.
3. Assinar os bytes exatos do documento com a chave do atualizador, e pôr
   `document` e `signature` no manifesto.
4. Se algum MOD publicado dependia daquela versão, **removê-lo do catálogo do
   indexador é um segundo ato**, no `SEELE-MODS-INDEXER`, com a chave de lá.
   Ver a seção 1.

O que acontece na máquina de quem já tem a versão revogada: ela **continua
abrindo**, e as conversas continuam lá para serem lidas. O que ela não faz é
hospedar e entrar. Está no ADR 0046 e está no código: a lista é conferida em
atos, e abrir não é um ato.

---

## 5. Dados por linha, e a frase que a tela tem de dizer

Cada linha abre `<raiz>/dados/servidor-<linha>/` — `dados/servidor-0.15`, e não
`dados/0.15.2` —, e o launcher aponta a variável `SEELE_HOME` para lá, que é a
variável que `config_dir` no `seele-app` já lê primeiro. Não há mecanismo novo,
e a consequência é boa: uma versão **anterior** ao launcher, iniciada por ele,
também obedece. Por linha, e não por versão, para um conserto da linha (a
0.15.3) herdar os dados da 0.15.2 sem cópia nenhuma.

A linha que está aberta não mora no depósito: o app 0.16 usa a pasta dele
(`~/.config/seele`, ou `%APPDATA%\tech.datadev.seele` no Windows), e o depósito
fica dentro dela, em `lancador/`.

### A primeira abertura da 0.16

Spec 0.16, §1. Antes de qualquer outra coisa — antes de a janela existir e antes
da migração dos MODs da pasta —, a 0.16 copia a pasta dela para
`dados/servidor-0.15`, **deixando o depósito (`lancador/`) e a chave
(`identity.key`) de fora** e com o marcador `PRONTA` escrito por último.

**A identidade é uma por máquina** ([ADR 0061](adr/0061-a-identidade-e-uma-por-maquina.md),
a decisão D-id-1), e por isso a chave não vai na cópia. O launcher passa a toda versão que
abre a variável `SEELE_IDENTIDADE` com a pasta da identidade **do próprio
processo**: a `SEELE_IDENTIDADE` que ele recebeu, ou a pasta de configuração
dele (no Mac, `~/.config/seele`; no Windows, `%APPDATA%\tech.datadev.seele`). A
0.15.2 lê a chave de `<pasta>/identity.key` quando a variável não está vazia;
o resto (o banco, os pins, as preferências) continua em `SEELE_HOME`. A pessoa
é a mesma em toda linha, e não há duas cópias da chave que possam divergir.

- **Não copia um banco do futuro.** Um banco com esquema acima do 13 (o da
  0.15.1, lido em `54f542f`) já foi migrado por uma versão mais nova, e
  copiá-lo daria à 0.15.2 um banco que o `migrate()` dela não recusa. A linha
  0.15 abre vazia, e a tela diz.
- **Uma cópia que entraria em si mesma é recusada.** O depósito mora dentro da
  origem; `Deposito::semear_linha` recusa um destino dentro da origem que não
  foi deixado de fora.
- **Onde houve 0.15, a linha legada é baixada uma vez**, em segundo plano. Sem
  rede, ela é baixada quando alguém precisar dela — ao entrar num servidor 0.15
  ou ao hospedar nele.
- **A ordem é estrutural.** No Tauri 2.11.6 as janelas do `tauri.conf.json`
  nascem antes do `setup` do app (`app.rs:2521-2531`); a janela é
  `create: false` e nasce no `setup`, depois da cópia.

### A frase

`Deposito::plano_da_linha` responde **sem tocar em disco**, e o campo
`perde_conversas` é a frase: ele é verdadeiro só quando as duas coisas valem ao
mesmo tempo — esta linha começa vazia **e** existe conversa em outro lugar
(noutra linha do depósito, ou na pasta do próprio app). Numa máquina em que
nunca houve conversa nenhuma não há o que avisar, e avisar ali é ensinar a
ignorar o aviso.

A tela diz, **antes** de abrir outra linha, que as conversas de uma não aparecem
na outra: no painel de servidores, ao entrar, e na tela de preparar, ao
hospedar. Sem a palavra «linha»: o ADR 0033 a tirou do texto de tela, e a frase
diz «Servidor 0.15».

**A cópia também pode parar no meio, e ela tem o marcador dela.** A instalação
separa «a pasta existe» de «a pasta chegou inteira» com o `INSTALADA`, escrito
por último; o diretório de dados tem o equivalente, o `PRONTA`, escrito depois da
cópia terminar. Sem ele, uma semeadura interrompida deixava a pasta criada e
pela metade, e a abertura seguinte a lia como «esta linha já tem os dados dela»:
a linha abria sem as conversas e **nada era dito**. Com o marcador, aquela pasta
é o que é — um resto —, a semeadura recomeça da origem, que não mudou, e o que a
tentativa anterior deixou não se mistura com a cópia boa. O marcador da origem
também não viaja na cópia: copiado primeiro, ele marcaria como inteira uma
cópia que ainda está no meio.
