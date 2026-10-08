# 0056 — MODs na tela inteira

Status: **proposto** — esqueleto de 2026-10-08. As decisões de 29/09 e de 08/10
estão tomadas; o contrato que este ADR fecha (`api/v6.json`) ainda é rascunho,
e os limites esperam a medição com as regiões da API 6.
Data: 2026-10-08
Sobre `versao/0.16` em `92e0d79`, cujo código é o da v0.15.1 publicada
(`54f542f`).

> **Numerado 0056 porque a spec de 23/09 já o chama assim**
> (`docs/superpowers/specs/2026-09-23-sala-pessoal-por-codigo-design.md:540`,
> `:852`). A pasta pulou de 0055 para 0057 por isso: o ADR do celular foi
> numerado 0057 para não tomar este número (`0057:23-27`).

> **Este documento estende o [ADR 0052](0052-um-mod-ganha-superficies-proprias-e-pontos-de-integracao.md)**,
> que deu ao MOD superfícies e pontos de integração, e respeita o
> [0049](0049-um-mod-deixa-de-rodar-na-janela-do-produto.md), que tirou o MOD
> da janela. O contrato com quem escreve MOD continua sendo o do
> [0045](0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md).
>
> **O desenho inteiro, com âncoras, está no rascunho da API 6**
> (`docs/superpowers/specs/2026-10-08-api-6-rascunho.md`). Esta página é a
> decisão, e o lugar dos números da medição de custo.

## O que estava em aberto

A proposta «API de MODs: a tela inteira como superfície», de 28/09, com as
decisões do dono de 29/09 (spec de 23/09, Parte II), e as de 08/10 (spec-mestre
da 0.16):

| decisão | quando | o quê |
|---|---|---|
| núcleo de confiança | 29/09 | os cinco itens da proposta, mais a ligação e a conexão segura; o indicador de quem fala fica fora |
| `decorar` | 29/09 | fase própria, depois: API 7, somada |
| o banner | 29/09 | no cartão da chamada, no fundo da barra do operador e no cabeçalho do perfil |
| a ordem | 29/09 | diagnóstico primeiro (feito na 0.15.1), depois a medição de custo, e só então a API 6 |
| as APIs aceitas | 08/10 | **só a 6** no app 0.16; as 3, 4 e 5 ficam na linha legada (Servidor 0.15) |
| o nome do banner | 08/10 (D-api-1) | **`pessoa.capa`** |
| o celular | 08/10 (D-web-1) | **os MODs rodam no celular**; cada região diz o que faz lá |
| o apelido | 08/10 (D-nucleo-1) | o apelido verificado fica sempre à vista quando um MOD troca o nome; entra no núcleo |
| a capa no celular | 08/10 (D-capa-1) | animada também na chamada, com o mesmo teto; nos dois cabeçalhos de perfil; fora da faixa de voz |
| a aparência pelo celular | 08/10 (D-estilo-1) | sim, com o mesmo formulário numa coluna, em tela cheia, com «ver na sessão» |

## A decisão

**Toda área da janela vira uma região nomeada e estável**, com os modos
`adicionar` e `substituir`. O MOD continua declarando e o produto continua
montando; o alcance passa da lista de convites para a janela inteira. As
regiões, as camadas (`chamada.palco`, `folha`, `dialogo` com formulários,
`aviso`), a `pessoa.capa`, o retrato projetado por lista branca, os eventos da
tela e o aviso da metade de servidor estão no rascunho, §3 a §8.

**O MOD não volta para a janela, não escreve HTML nem seletor, e trocar o que
se desenha nunca troca a chave.** As três decisões medidas dos ADRs 0049 e 0052
são o que deixa a tela inteira ser do MOD sem deixar de ser do servidor.

**O núcleo de confiança é contrato testado.** Oito itens no desktop (sair;
microfone e «no ar»; moderação por identidade; o apelido verificado; atribuição;
aceite; a ligação; a conexão segura), e mais seis no celular (a faixa de voz, as
abas, a pergunta «DE ONDE?», as áreas seguras, os 44 px e os 16 px, o zoom).
**Nenhum mora dentro de uma região, aceita contribuição ou é alcançado pelo
tema**, e o produto garante o contraste deles com qualquer tema (o achado do
ESTILO: hoje um acento `#050403` leva o MUDO apertado a 1,00:1).

**A API 6 promete só o que é entregue.** Ela é escrita por inteiro, sem
`extends`, e o `check-api` cobra os dois sentidos para momentos, eventos,
pontos, tipos e capacidades. O rascunho, §13, lista o que as versões
publicadas prometeram e não entregaram.

## A medição de custo (M3)

O ADR 0052 pediu, e ninguém fez, a medição do aplicativo inteiro sob carga:
*«sem MOD, com três ativos e UI fechada, com um editor aberto, com MESA em
arraste, e os mesmos cenários com voz»* (`0052:189-193`). Com a tela inteira
aberta a MODs, ela deixa de ser opcional: os limites de cada região congelam
junto com o `v6.json`, e por isso **esta medição segura o congelamento, e não a
construção**.

Ela é feita duas vezes, com o mesmo roteiro: na v0.15.1 publicada, como
«antes», e com as regiões da API 6, como «depois». Os limites do rascunho, §12,
saem da segunda.

### O roteiro

Em [`docs/evidencias/m3-custo-dos-mods/`](../evidencias/m3-custo-dos-mods/LEIA.md).
Em uma frase: um servidor novo por cenário, com 0, 1 ou 3 MODs oficiais (os
pacotes publicados no indexador, com o hash conferido contra o catálogo); o
**app publicado, sem mudança nenhuma no binário**, aberto com uma pasta de dados
descartável e `--entrar`; a janela conduzida pela acessibilidade, sem mover o
mouse; e um falante sintético (Opus de 440 Hz, a cada 20 ms) na sala de voz.

| o quê | como |
|---|---|
| memória | `phys_footprint` do `seele-app` e dos processos do WebKit que nasceram com ele (WebContent, GPU, Networking), por `footprint`; mediana de 5 amostras em 60 s |
| CPU | o tempo de CPU somado dos quatro processos no intervalo, por `ps` |
| tempo de quadro | `xctrace` com o instrumento «Core Animation Commits», preso ao app, nos primeiros 30 s: os commits do `seele-app` (no WebKit, é o processo do app que faz o commit do quadro da página), por segundo e por duração, e os que passam de um quadro a 60 Hz |
| jitter da voz | o número da barra de telemetria (JITTER), lido a cada 2 s |
| a máquina | o cenário só começa sem compilação de outra sessão (carga < 4) e com a máquina ociosa há 5 min; qualquer entrada durante ele o marca interrompido |

### Os números de base (v0.15.1, macOS)

**Máquina:** Apple M5 Pro, 24 GB, macOS 27.0.1; janela de 1280 × 800 pontos na
tela embutida (Liquid Retina XDR). **App:** SEELE 0.15.1 publicado
(`/Applications/SEELE.app`, assinado ad hoc, `54f542f`). **MODs:** PERFIS 3.3.0
(API 5), MESA 3.1.0 (API 4), ESTILO 3.1.0 (API 4).

**Parcial: 4 dos 10 cenários.** A campanha de 08/10 foi parada às 14:05 a
pedido da orquestradora, porque a integração das frentes de código ia rodar a
bateria e as duas disputariam CPU. Faltam `3-mods-mesa` e os cinco cenários
com voz. Os relatos inteiros estão em
`docs/evidencias/m3-custo-dos-mods/medidas.json`.

Memória em MiB (`phys_footprint`, mediana na janela de 60 s); CPU em % de um
núcleo, somada dos quatro processos; commits de Core Animation do `seele-app`
nos primeiros 30 s, e a duração deles em ms.

| cenário | rodada | entrada | `rustc` na janela | app | GPU | WebContent | Networking | **total** | CPU | commits/s | p50 | p95 | máx | acima de 1 quadro |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| sem MOD | 13:15 | não | 14 | 38,3 | 94,7 | 56,2 | 5,8 | **195,0** | 4,7 | 4,2 | 0,17 | 0,26 | 0,28 | 0 |
| sem MOD | 13:17 | não | 10 | 37,3 | 94,8 | 55,0 | 5,7 | **192,8** | 5,3 | 4,2 | 0,18 | 0,24 | 0,32 | 0 |
| 1 MOD (PERFIS) | 13:19 | não | 14 | 42,8 | 95,0 | 66,5 | 5,8 | **210,1** | 5,4 | 4,6 | 0,15 | 0,23 | 0,38 | 0 |
| 1 MOD (PERFIS) | 13:22 | não | 15 | 43,3 | 95,2 | 61,9 | 5,8 | **206,2** | 5,9 | 4,6 | 0,17 | 0,28 | 0,38 | 0 |
| 3 MODs, UI fechada | 13:47 | não | **0** | 40,6 | 100,1 | 106,2 | 4,8 | **251,7** | 8,9 | 5,5 | 0,18 | 0,33 | 0,42 | 0 |
| 3 MODs, editor do PERFIS aberto | 14:03 | **sim** | 0 | 44,4 | 101,1 | 140,2 | 4,9 | **290,6** | 11,7 | 13,1 | 0,14 | 0,22 | 0,31 | 0 |

Uma rodada do editor, às 13:49, está nos relatos e fora da tabela: o roteiro
tocou o cartão antes de o PERFIS registrá-lo, e o editor não abriu (conserto no
`m3.py`: a ação espera o cartão).

**Como ler, e o que ainda não se pode dizer:**

- **O custo dos MODs mora no WebContent.** De nenhum para três MODs com a UI
  fechada, o total sobe de ~194 MiB para ~252 MiB (+58), e o WebContent sozinho
  de ~56 para ~106 (+50). O app, onde rodam os três QuickJS, sobe ~3 MiB. Abrir o editor do
  PERFIS soma mais ~34 MiB no WebContent.
- **A CPU de repouso quase dobra** (de ~5 % para ~9 % de um núcleo com três
  MODs), e o editor aberto leva a ~12 %, com mais que o dobro dos commits por segundo
  do `3-mods` (13,1 contra 5,5).
  [inf] O laço de consulta de cada MOD (4 s no PERFIS e no ESTILO, 2 s na MESA,
  e a remontagem do editor a cada volta, PERFIS B6) é a explicação mais
  provável; os eventos e o aviso do servidor (rascunho §7) são o que a tira.
- **Nenhum commit passou de um quadro** em nenhum cenário medido. Isso diz pouco:
  com a UI parada, a página quase não desenha. O cenário que exercita o quadro
  é o da MESA em arraste, que falta.
- **As rodadas com `rustc` de outra sessão na janela** (sem MOD e 1 MOD) não têm
  par limpo. As duas de cada cenário concordam entre si em 4 MiB e 0,6 ponto de
  CPU; a comparação com o `3-mods`, limpo, é de memória com segurança, e de CPU
  com essa ressalva.
- **A rodada do editor teve entrada de uma pessoa durante a janela**, e por isso
  é indicativa, e não base.

**O que retomar, com o mesmo roteiro, quando a orquestradora liberar:**
`3-mods-editor` limpo, `3-mods-mesa`, e os cinco `+voz`; e, se a máquina
ficar quieta, um `sem-mod` e um `1-mod` sem `rustc` na janela.

### O que falta medir, e quem mede

- **Windows (WebView2)**: o mesmo roteiro pede um equivalente do `footprint` e
  do `xctrace`; pedido à orquestradora.
- **iPhone**: memória e tempo de quadro com os três MODs oficiais, e o cenário
  da capa (sete capas animadas de 60 quadros na chamada e na barra, mais a
  lista com 20 capas paradas — PERFIS R1). É o item 3 da F4, e do dono.
- **Com as regiões da API 6**: depois do porte, na tarefa 11 do plano da F3.

## O que este ADR não decide

- **Os limites** (rascunho, §12): esperam a M3 com a API 6.
- **`pagina` e `painel` contra `folha`; `regiao`, `cartoes` e `tema`; e o m-4
  da pendência 49** (`repetir` ou o contrato de `tocando`): do dono com as
  sessões de MOD, com a recomendação no rascunho, §18.
- **Os três candidatos ao fio** (rascunho, §14): o aviso da metade de servidor,
  os pedidos endereçados ao servidor e o tema como dado do servidor. Com a F1.
- **Como o MOD roda no navegador**: decidido pelo spike da F4 (o motor do
  desktop num Worker dentro de um `iframe` opaco, spec-mestre `00935dac`).
  Este ADR fixa só o que isso pede da API: um contrato e um motor, nada que
  dependa de o MOD rodar fora de uma volta do anfitrião, e os mesmos limites
  nos dois (rascunho, §16).

## Custo de reverter

**O nome de uma região publicada não reverte**, como o formato do código da
sala pessoal: depois do `api/v6.json` publicado, renomear quebra os MODs. Por
isso os nomes nascem congelados, e só depois da medição.

**O núcleo de confiança reverte com `git revert`**, mas reverter é tirar uma
garantia que o `v6.json` promete, e isso é uma versão de API nova.
