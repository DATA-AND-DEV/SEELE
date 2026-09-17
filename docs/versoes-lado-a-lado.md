# Versões lado a lado — o contrato do manifesto e o que falta publicar

O [ADR 0046](adr/0046-toda-versao-continua-de-pe.md) decidiu que o app vira
launcher: cliente, servidor, protocolo e API de MOD sobem juntos com o mesmo
número, toda versão publicada continua hospedável, e cada uma tem o seu
diretório de dados. Esta página é o que aquela decisão precisa por escrito para
virar código dos dois lados: **o formato do manifesto**, **onde a lista de
revogação mora** e **o que ainda falta no `SEELE-RELEASES`**.

O núcleo está em `crates/seele-lancador`. Ele não fala com a rede: recebe os
bytes do manifesto e do pacote, e responde qual executável roda, com que dados,
ou por que não.

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

```json
{
  "schema": 2,
  "versions": [
    {
      "version": "0.11.0",
      "pub_date": "2026-09-20T00:00:00Z",
      "notes": "…",
      "unit": { "protocol": 4, "mod_api": 1 },
      "platforms": {
        "darwin-aarch64": {
          "url": "https://github.com/DATA-AND-DEV/SEELE-RELEASES/releases/download/v0.11.0/…",
          "signature": "<base64 do .sig, como hoje>",
          "sha256": "<64 dígitos hexadecimais>",
          "executable": "SEELE.app/Contents/MacOS/SEELE"
        }
      }
    }
  ],
  "revocations": {
    "document": "<base64 dos bytes exatos do documento abaixo>",
    "signature": "<base64 do .sig daqueles bytes>"
  }
}
```

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

**A ordem de `versions` é o contrato, e a primeira é a mais nova.** Não há
ordenação calculada em lugar nenhum do launcher, e a razão está publicada: para
o semver, `0.10.5-1` é uma pré-lançamento de `0.10.5` e portanto anterior a
ela; no `SEELE-RELEASES`, a `v0.10.5-1` saiu **depois** da `v0.10.5`. Um
launcher que ordenasse sozinho ofereceria a versão errada como «a mais nova».

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

## 3. As lacunas exatas, para uma tarefa no `SEELE-RELEASES`

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
| 1 | `latest.json` descreve uma versão, não todas | `empacotar/manifesto.py`, que é o único lugar onde a regra do manifesto mora, e o `release.yml` que o chama | o seletor de versão tem uma opção só, e o ADR 0046 vale no código e não no produto |
| 2 | não há `sha256` por pacote | mesmo arquivo; é uma linha por pacote | um download truncado é recusado pela assinatura, com o motivo errado: «não veio de nós» em vez de «veio pela metade» |
| 3 | não há `executable` por pacote | mesmo arquivo, uma constante por alvo | o launcher não sabe o que iniciar, e recusa com `ExecutavelNaoDeclarado` — ao instalar e ao abrir. É o único caminho que toda máquina encontra hoje, e é a lacuna que bloqueia o launcher em produção. O valor por alvo é `SEELE.app/Contents/MacOS/SEELE`, `SEELE.exe` e `bin/seele`, sempre **relativo** à raiz da instalação: o launcher recusa caminho absoluto ou com `..` |
| 4 | não há `unit` (protocolo e API de MOD) por versão | precisa sair do build: são `seele_proto::version::PROTOCOL_VERSION` e o número da API de `api/v1.json` | a conferência de coerência do ADR 0046 não acontece, e a defesa volta a ser só o `negotiate` — que é justamente o que o ADR tirou do posto principal |
| 5 | não há bloco `revocations`, e não há procedimento para emitir um | uma ferramenta nova ao lado de `empacotar/manifesto.py`, que assina o documento com a mesma chave do atualizador | não há como revogar uma versão furada, que é a única exceção ao «toda versão para sempre» |
| 6 | o manifesto lista só o release corrente | o `release.yml` precisa **ler o manifesto anterior** e acrescentar a versão nova no topo, em vez de escrever um do zero | cada release apaga a lista de todas as anteriores, e o launcher esquece o que existe |
| 7 | os pacotes das versões antigas estão em releases antigos, e o manifesto único vai apontar para lá | nada a fazer no formato: as URLs já são absolutas por release | — |

A número 6 é a que muda mais o `release.yml`, e é a que não dá para adiar: um
manifesto de versões que se reescreve do zero a cada release é um manifesto de
uma versão com outro nome.

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

## 5. Dados por versão, e a frase que a tela tem de dizer

Cada versão abre `<raiz>/dados/<versão>/`, e o launcher aponta a variável
`SEELE_HOME` para lá — que é a variável que `config_dir` no `seele-app` já lê
primeiro, antes de `XDG_CONFIG_HOME` e de `HOME`. Não há mecanismo novo, e a
consequência é boa: uma versão **anterior** ao launcher, iniciada por ele,
também obedece.

A assimetria entre subir e descer não é escolha de produto — vem de
`persistence/schema.rs`, que é «append only once shipped», com um teste
reprovando qualquer passo de volta:

- **subir**: os dados são **copiados** da versão mais nova entre as mais
  velhas, e a versão nova migra o que copiou. Cópia e não mudança de lugar, o
  que deixa a origem de pé para se poder voltar a ela.
- **descer**: não há nada mais velho de onde copiar, e a versão começa vazia.

`Deposito::plano_de_dados` responde isso **sem tocar em disco**, e o campo
`perde_conversas` é a frase: ele é verdadeiro só quando as duas coisas valem ao
mesmo tempo — esta versão começa vazia **e** existe conversa em outra. Numa
máquina em que nunca houve conversa nenhuma não há o que avisar, e avisar ali é
ensinar a ignorar o aviso.

**A cópia também pode parar no meio, e ela tem o marcador dela.** A instalação
separa «a pasta existe» de «a pasta chegou inteira» com o `INSTALADA`, escrito
por último; o diretório de dados passa a ter o equivalente, um `PRONTA` escrito
depois da cópia terminar. Sem ele, uma semeadura interrompida — a máquina
desliga durante a cópia — deixava a pasta da versão nova criada e pela metade,
e a abertura seguinte a lia como «esta versão já tem os dados dela»: a versão
abria sem as conversas e **nada era dito**. Com o marcador, aquela pasta é o
que é — um resto —, a semeadura recomeça da origem, que não mudou, e o que a
tentativa anterior deixou não se mistura com a cópia boa. O marcador da origem
também não viaja na cópia: copiado primeiro, ele marcaria como inteira uma
cópia que ainda está no meio.
