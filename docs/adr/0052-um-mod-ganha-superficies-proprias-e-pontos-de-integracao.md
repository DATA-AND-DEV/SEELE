# 0052 — Um MOD ganha superfícies próprias e pontos de integração

Status: **aceito**
Data: 2026-09-20
Sobre o commit `655a137` — o que a v0.12.1 publicou.

> **Este documento estende o [ADR 0049](0049-um-mod-deixa-de-rodar-na-janela-do-produto.md),
> que tirou o MOD da janela, e o [ADR 0045](0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md),
> que é contrato com quem escreve MOD.**
>
> **Decidido:** a API 4 acrescenta superfícies com ciclo de vida, um registro de
> contribuições na interface do SEELE, e estilos declarados e validados. O
> executor, a ponte e o renderer não mudam. A conferência de manifesto deixa de
> ser igualdade e passa a ser um conjunto de versões aceitas.

## O que a auditoria mediu

A auditoria de UX/UI de 20/09/2026 percorreu o aplicativo nativo 0.12.1 com os
três MODs oficiais instalados e ativos. O resultado dela sobre apresentação é
uma frase: *«A apresentação dos MODs não atende ao objetivo de criar
experiências próprias dentro de um servidor. A faixa inferior limita atividades
diferentes ao mesmo formulário estreito.»*

Os três achados P1 dizem a mesma coisa por três ângulos:

- **U01.** «Todos os MODs disputam uma faixa de até 240 px. O centro da janela
  fica disponível para conversa vazia, enquanto editar perfil/tema e jogar exige
  rolar um rodapé.»
- **U02.** «A rolagem pertence ao contêiner dos três MODs. Rolar PERFIS desloca
  ESTILO e faz MESA desaparecer.»
- **U03.** «Não há caminho de MOD para abrir uma experiência ampla, modal
  próprio ou painel ajustável. Os MODs aparecem todos ao conectar, sem escolha
  da atividade.»

E um quarto, sobre integração:

- **U27.** «Cartões na API são conteúdo adicional, sem interação, sem composição
  de banner e sem substituição dos campos nativos. Isso é inferior ao PERFIS
  anterior.»

## O que **não** era o problema

Espaço. Aumentar a faixa resolveria o primeiro sintoma e nenhum dos quatro
problemas, e o plano diz isso pelo nome: *«Aumentar `max-height` da faixa não
cria uma API de aplicações.»*

O que faltava era **ciclo de vida**. Uma região existe enquanto o MOD estiver de
pé, aparece sozinha ao conectar, e não tem título, foco, rota, estado de
alteração nem saída. Uma superfície tem as cinco coisas, e é por isso que ela
pode ser uma mesa de jogo e a região não.

A segunda metade de U03 é a mais barata de perder de vista: os três MODs
desenhavam ao conectar porque **não havia o gesto de abrir um**. Uma faixa com
três regiões é o produto decidindo, por omissão, que as três atividades estão
acontecendo o tempo todo.

## A decisão

### 1. Superfícies

`SeeleUI.superficies.criar(descricao)` devolve um punho com `montar`, `classes`,
`mostrar`, `ocultar`, `suja`, `titulo`, `fechar` e `descartar`. Quatro tipos:

| Tipo | Onde mora | O que o produto garante |
|---|---|---|
| `pagina` | na célula da conversa, por cima dela | voltar no cabeçalho; a voz, a lista de pessoas e a saída continuam de pé |
| `painel` | uma coluna que **acrescenta**, e some quando não há nenhum | largura contida; recolhe em janela estreita |
| `dialogo` | camada sobre a aplicação | foco contido, `inert` no resto, Escape, retorno do foco |
| `aviso` | fila de vida curta | teto de quatro; o mais velho sai |

A saída é montada **pelo produto**, com o texto do produto, ligada a um comando
do produto. Um MOD que não desenhe botão nenhum, que trave no meio da montagem
ou que sature a fila continua sendo uma superfície de onde se sai.

O cabeçalho diz de qual MOD a superfície é. Uma tela de MOD indistinguível das
do SEELE é o que tornaria uma tela de confiança falsificável.

### 2. Contribuições

`SeeleUI.contribuicoes.registrar({ ponto, modo, alvo, prioridade, ... })`
registra num **ponto semântico** e devolve um handle revogável. Nunca um
seletor de CSS, um nó do documento, `innerHTML` ou folha global: um seletor é um
contrato que ninguém escreveu, e no dia em que a conversa renomear
`.roster-linha` todo MOD publicado quebra junto.

Três garantias, e elas são o contrato inteiro:

- **identidade.** Trocar o que se desenha não troca a chave. Um MOD pode
  escrever outro nome no cartão de alguém — é para isso que o PERFIS existe — e
  o menu de moderação daquela linha continua agindo sobre a pessoa certa;
- **determinismo.** Duas substituições do mesmo ponto se resolvem por prioridade
  declarada e, empatadas, por ordem de registro. Com escolha de quem administra,
  ela ganha — uma pessoa decidiu, e um número não. A disputa é **mostrada** na
  gestão de MODs, com «usar apresentação padrão» ao lado;
- **recuperação.** Ao revogar, a apresentação nativa é recalculada do estado
  atual, e não restaurada de um `innerHTML` guardado. A pessoa pode ter mudado
  de sala enquanto o MOD estava aberto.

### 3. Estilos

Propriedades declaradas por categoria, validadas uma a uma e montadas de partes
conferidas. A fronteira é dita pelo que ela é — **alcance, e não gosto** —, e
está emendada em `specs/07-estetica.md`.

Não existe, e não deve passar a existir, uma função que receba CSS e tente
limpá-lo. Limpeza por substituição de texto é uma corrida contra o analisador do
navegador, e quem escreve o analisador não sabe que esta corrida existe.

### 4. Compatibilidade: um conjunto, e não um número

Até aqui a conferência era igualdade: `manifest.api == MOD_API_VERSION`, com
`ApiTooOld` para tudo abaixo. Estava certa na ruptura anterior — a API 3
**tirou** capacidades, e «serve uma API mais velha» tinha deixado de ser verdade.

A API 4 não tira nada. Ela acrescenta sobre o mesmo executor, a mesma ponte e o
mesmo renderer, e um pacote de API 3 continua sendo o que era: uma região, um
tema e cartões.

Manter a igualdade significaria que subir a constante **quebraria todo pacote
publicado no mesmo instante** — inclusive os três oficiais, na máquina de quem
já os tinha instalado. `APIS_ACEITAS` é o que permite publicar o aplicativo
compatível antes dos pacotes novos, que é a única ordem em que ninguém fica sem
MOD.

**Aceitar a 3 não dá à 3 o que a 4 tem.** As capacidades são por versão
(`capacidades_da_api`), e quem as aplica é o prelúdio do executor: um pacote que
declara `api: 3` recebe um `SeeleUI` em que `superficies` simplesmente não
existe. Ausência, e não recusa em tempo de execução — um método que existisse e
sempre falhasse faria o autor descobrir o problema dentro de um `catch`, em
produção; um método que não existe é um `TypeError` na primeira linha que o
chama.

A API 2 não volta. Ela executava na janela, e o ADR 0049 explica por que não há
caminho de volta disso.

## O que isto custa, dito antes

**O renderer cresceu.** `mods-regiao.js` foi de onze formas para trinta, e ganhou
estilos, classes e um segundo perfil de orçamento. Cada forma nova é uma decisão
de API registrada na tabela, e uma forma que não está lá continua sendo recusada
e contada — mas a tabela é maior, e uma tabela maior é mais superfície para
revisar.

**Um MOD pode cobrir a conversa.** Uma página ocupa a célula dela enquanto está
aberta. A mitigação é o cabeçalho persistente, com o voltar do produto, e ela é
a mesma da configuração — mas é mitigação, e não impossibilidade.

**A disputa de apresentação é nova e real.** Dois MODs de perfil no mesmo
servidor produzem uma decisão que antes não existia. Ela é determinística e
visível, e continua sendo uma decisão a mais na tela de quem administra.

**O custo não foi medido sob carga.** A auditoria pede medição do aplicativo
inteiro — sem MOD, com três ativos e UI fechada, com um editor aberto, com MESA
em arraste, e os mesmos cenários com voz —, e isso não foi feito. O que existe
são os tetos declarados e o descarte registrado; o que não existe é a linha de
base. Nada aqui deve ser lido como «o custo cabe».

## O que este ADR não decide

- **Nada sobre publicar.** Os pacotes 4 dos três MODs oficiais existem neste
  repositório e nos repositórios irmãos; publicá-los exige a chave, e a ordem
  está no runbook: aplicativo compatível primeiro, pacotes depois.
- **Nada sobre homologação.** As oito jornadas de aceite do plano terminam em
  observação nativa, com dois participantes e dados existentes. Contagem de
  suítes verdes não as substitui, e este documento não as declara feitas.
- **Nada sobre Windows e Linux.** A auditoria não os percorreu, e esta entrega
  tampouco.
