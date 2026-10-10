# Lote RT da F5: os roteiros da casca rodam no CI e no `publicar.sh`

Ramo `0.16/lotes-rt`, a partir de `b101e385` (`0.16/lotes`, com o W2 e o W3
integrados). Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a39ea57996d5694c2`.

**Estado: pronto.** Os 34 roteiros passam aqui. O CI e a bateria do
`publicar.sh` rodam todos, e um guarda reprova o roteiro que ficar fora de uma
das duas listas. Três mudanças na `carga-da-casca.py` vieram de passagem, cada
uma num commit próprio (§2). Duas delas não estavam no brief: o relato que a
aspa cortava, e o `--no-sandbox`.

| commit | o que faz |
|---|---|
| `d2bd9d0d` | carga: o relato atravessa a aspa, e um REPROVADO depois dela deixa de sumir |
| `a95f0463` | carga: vários `--roteiro` numa chamada, todos rodados, e o fim diz quem reprovou e com que frase |
| `be0ec125` | carga: o navegador abre com `--no-sandbox`, como o Playwright o abre para as bancadas |
| `6362d926` | o passo do CI, a etapa do `publicar.sh`, o guarda e os testes da bancada do `publicar.sh` |
| (este)     | este relatório |

**A pausa da orquestradora** (medida de CPU) chegou depois do `6362d926`.
Tudo o que está nas seções abaixo foi rodado **antes** dela. Este relatório foi
escrito e commitado durante a pausa, sem rodar nada.

## 1. Os roteiros rodados aqui, e o que cada um prova hoje

A pasta tem **34** roteiros, e não os 31 do brief. Cada um rodou sozinho pela
carga de `b101e385`:
`python3 tools/carga-da-casca.py --roteiro tools/roteiros/<x>.js`. **Os 34
saíram com 0 («carga limpa»)**, e cada um levou menos de 1 s. Depois de cada
mudança na carga, os 34 rodaram de novo, sozinhos e numa chamada só, e saíram
iguais (§2).

**O que a carga reprova** (lido em `carga-da-casca.py` e medido com sondas):

- um `Uncaught …Error`, inclusive `(in promise)`;
- um `REPROVADO` de um `exigir`;
- um `ROTEIRO ESTOUROU`, isto é, uma exceção no roteiro (um `getElementById`
  que volta `null` e é usado, por exemplo);
- um roteiro que não chega ao fim.

**Ela não reprova um `console.error`, ao contrário do que o brief diz.** Ele
não casa com a expressão de estouros, que só pega `Uncaught`, e só aparece com
`--tudo`. Para quem não tem `exigir`, o que segura o roteiro é esta lista e
nada mais.

### Os da área de MODs: `exigir` no que dizem (lote W3)

| roteiro | `exigir` | prova hoje |
|---|---|---|
| `aceite-de-mods.js` | 18 | a recusa por MODs abre o diálogo; os itens, o alcance e o repositório desenhados; ACEITAR grava o sim e tenta de novo, nessa ordem |
| `aceite-de-mods-que-falha.js` | 13 | ACEITAR que rejeita por dentro vira frase no diálogo, o botão volta, um registro vai ao seele.log, e a segunda vez entra |
| `aceite-de-mods-que-nao-abre.js` | 14 | a pergunta que não abre vira linha vermelha, e a pergunta pendente é limpa |
| `aceite-de-mods-em-corrida.js` | 12 | com duas recusas em corrida, o ACEITAR responde pela lista que está na tela |
| `catalogo-de-mods.js` | 11 | BUSCAR desenha o que o catálogo devolveu, INSTALAR pede aquele MOD naquela versão, e a recusa vira frase |

### Os de fora, com `exigir` próprio (9)

| roteiro | `exigir` | prova hoje |
|---|---|---|
| `apelido-recusado.js` | 4 | o convite não gasto pelo apelido tomado vai na tentativa seguinte e não vaza para outro servidor |
| `apps-mais-novos.js` | 7 (+ sessão) | a linha de ESTE SERVIDOR com 0 e com 3 mensagens, e o recuo dela |
| `escolher-transmissao.js` | 1 | só que a escolha entre duas telas diz de quem é cada uma; as outras 19 linhas são relato |
| `hospedar.js` | 1 (+ sessão) | HOSPEDAR não passa pela conferência de chave |
| `nome-ao-hospedar.js` | 1 (+ sessão) | o apelido gravado vai no `connect` de quem hospeda |
| `sair-da-sala.js` | 4 (+ sessão) | SAIR DA SALA para quem já chega dentro, e a grade abre pela fileira da sala |
| `sala.js` | 14 | SEU CÓDIGO, a agenda, «receber ligações» e as frases da sala (F7) |
| `sessao.js` | 1 (+ sessão) | a entrada não fica na frente da sessão |
| `toque.js` | 19 (+ sessão) | a faixa do toque e o ATENDER, na ordem (F7) |

### Os 20 sem `exigir` próprio: a lista do §8 do W3

Treze passam pela `entrarNaSessao` da cabeça do roteiro, e ela tem um `exigir`:
reprova se HOSPEDAR AQUI não deixa a sessão na frente. **Isso, mais a lista da
carga, é tudo o que eles provam.** O resto é relato, e eu li o relato de hoje
contra o que cada cabeçalho diz provar:

| roteiro | sessão | o relato de hoje, contra o cabeçalho |
|---|---|---|
| `avatar-na-chamada.js` | sim | 3 cartões; juli com retrato e os outros com iniciais. Bate |
| `barra-mac.js` | sim | as caixas da barra, com `barra-controles[0..0]`. É medida, sem veredito |
| `clique-nos-glifos.js` | sim | o centro dos 3 botões cai dentro do botão. Bate |
| `compartilhar.js` | sim | caixa VISIVEL, mas `compartilhar-quadros: AINDA EXISTE`, e o cabeçalho diz «qual monitor, e mais nada». **Diverge; não analisei se o controle voltou de propósito** |
| `icone-do-server.js` | sim | a imagem passa de 30 para 34 chars. Bate |
| `icone-na-trilha.js` | sim | o mesmo, no ladrilho. Bate |
| `medidas.js` | sim | medidas, sem veredito |
| `palco.js` | sim | canvas 960x540 com ~259 mil pixels claros. Com o conserto da aspa aparecem duas linhas que se perdiam, e uma diz «aviso do palco: NINGUÉM ESTÁ COMPARTILHANDO A TELA» com o quadro desenhado. Provavelmente é o `tela: null` do duble; não analisei |
| `perfil-salva-sessao.js` | sim | `escolher_apelido` entre os comandos. Bate |
| `perfil-sessao.js` | sim | perfil VISIVEL |
| `rodape-do-operador.js` | sim | os rótulos do rodapé em cada passo. Bate |
| `tela-de-outra-versao.js` | sim | a frase aparece depois do `ScreenUnreadable`. Bate |
| `trocar-de-janela.js` | sim | **contradiz o cabeçalho, ver abaixo** |
| `atualizacao.js` | não | painel VISIVEL, «VOCÊ ESTÁ NA ÚLTIMA VERSÃO» |
| `botoes-da-entrada.js` | não | os três botões abrem a camada certa (VISIVEL) |
| `dispositivos.js` | não | ESCOLHIDO no padrão, EM USO no aparelho. Bate |
| `perfil-entrada.js` | não | perfil VISIVEL |
| `perfil-salva.js` | não | `escolher_apelido_local` chamado, e o perfil fecha. Bate |
| `renomear-canal.js` | não | SPAN, BUTTON, mesmo nó, SPAN. Bate |
| `versao-do-servidor.js` | não | **contradiz o cabeçalho, ver abaixo** |

**Dois roteiros estão podres e verdes, e vão continuar verdes no CI:**

- **`versao-do-servidor.js`** diz provar que «entrar num servidor que roda
  outra versão abre aquela versão» (ADR 0046). O relato de hoje diz «abriu
  outra versão: false · conectou aqui mesmo: true», o contrário. O roteiro
  responde ao `analisar_convite` com `{ versao, pode_abrir_naquela_versao }`,
  mas a casca hoje lê `convite.para_entrar` (`camada-servidores.js:196`, o
  `versoes::ParaEntrar` da emenda de 08/10/2026). Ela passa ainda por um
  diálogo de confirmação (`entrarPelaLinha`, `camada-versoes.js`). O caminho
  do ADR 0046 não é exercitado.
- **`trocar-de-janela.js`** diz provar que o botão vira TROCAR enquanto a
  transmissão é sua. Todas as linhas do relato dizem «COMPARTILHAR ·
  modo=comecar». O roteiro põe `desenharBotoesDeTela({ tela: { e_minha: true } })`,
  mas a casca lê `snapshot.minha_transmissao` (`camada-compartilhar.js:350`)
  desde `14d9c30f` (22/09/2026). É o mesmo retrato velho que deixava a
  `ajustes-v013.cjs` vermelha até 01/10. Aqui não há `exigir`, e por isso o
  roteiro fica verde.

Não mexi em nenhum dos dois. Dar a eles `exigir`, e o retrato que o produto
lê, fica fora deste lote e é decisão da orquestradora.

## 2. O aparelho: três mudanças na `carga-da-casca.py`

A carga não lê `PLAYWRIGHT` e não usa o Playwright. Ela procura o primeiro
`chrome-headless-shell` em `~/Library/Caches/ms-playwright`,
`~/.cache/ms-playwright` ou `~/.cache/puppeteer` e o abre direto. Aqui ela
achou `chromium_headless_shell-1234/chrome-headless-shell-mac-arm64/chrome-headless-shell`
(Playwright 1.62.1).

### 2.1 A aspa escondia o REPROVADO (`d2bd9d0d`): fora do brief

O Chromium escreve a linha do console entre aspas duplas sem escapar as de
dentro, e a carga lia o relato até a primeira (`SEELE-RELATO ([^"]*)`). O W3
viu o corte numa frase já reprovada e tirou os `JSON.stringify` das frases. O
caso pior não tinha sido visto: **uma aspa num relato antes de um `exigir` que
reprova faz o REPROVADO sumir, e a carga sai limpa, com 0.** Medido com uma
sonda (`relatar('com "aspas" aqui'); exigir(false, …)`):

- antes: «carga limpa», saída 0;
- depois: «o roteiro reprovou: REPROVADO: …», saída 1;
- reversão: a carga de `b101e385` volta idêntica (`cmp`) e devolve o 0; o
  conserto volta idêntico e devolve o 1.

O relato agora viaja como JSON codificado para URL, sem aspa nem espaço. Para
saber se algum REPROVADO estava escondido hoje, a saída crua do navegador de
cada um dos 34 foi comparada com o que a carga lia. **Nenhum estava.** Só o
`palco.js` perdia as duas últimas linhas. Antes e depois do conserto, a saída
dos 34 difere só aí e nas linhas com « | » dentro, que a carga partia em duas
(`clique-nos-glifos.js`, `rodape-do-operador.js`).

Pus o conserto aqui porque sem ele a promessa do lote é falsa: um passo de CI
que «reprova se algum roteiro reprovar» passaria um roteiro vermelho. Ele está
num commit só dele, e a orquestradora pode tirá-lo sem tocar no resto.

### 2.2 Vários `--roteiro` numa chamada (`a95f0463`)

Antes, o segundo `--roteiro` tomava o lugar do primeiro em silêncio, porque o
`argparse` guardava o último:
`--roteiro <vermelho> --roteiro perfil-entrada.js` saía «carga limpa», 0. Agora
o argumento se repete, e com mais de um:

- cada roteiro roda com a sua carga, e todos rodam mesmo depois de um
  vermelho;
- cada um é dito antes de rodar (`roteiro: rodando <caminho>`), com a saída
  linha a linha, para que um roteiro que pendure seja a última linha que o
  prazo deixa;
- o fim repete, numa linha por motivo, o roteiro e a frase: `roteiros: 1 de 34
  reprovaram:` e `  tools/roteiros/x.js: REPROVADO: …`;
- um arquivo que não existe vira um motivo, e não uma exceção que pararia a
  lista;
- `--foto` com mais de um é recusado.

Com um roteiro só, ou nenhum, a saída é a de antes. Os 34 rodados um a um
antes e depois da mudança saem iguais, tirando as linhas do relógio. Reversão:
a mesma prova sai 0 com a carga de `d2bd9d0d` (`cmp` idêntico) e 1 com o
conserto. Os 34 numa chamada: 0, em 9 s.

### 2.3 `--no-sandbox` (`be0ec125`): fora do brief, e sem vermelho

O Playwright passa `--no-sandbox` sempre que `chromiumSandbox !== true`
(`playwright-core/lib/coreBundle.js`, 1.62.1). É assim que as bancadas do mesmo
job abrem o Chromium. A carga abria o navegador sem essa flag. O Ubuntu 24.04
restringe por AppArmor os espaços de nome sem privilégio de que a caixa de
areia precisa. **Se o runner do Actions mantém essa restrição não foi medido**:
duas buscas não acharam a resposta nos runner-images. Daqui não há como
produzir o vermelho. O que se mediu é que a flag não muda nada no Mac: os 34
saem iguais e com 0.

## 3. O passo do CI

No job `bancadas` do `ci.yml`, no fim, depois do
`npx playwright install --with-deps chromium`:

```yaml
      - name: os roteiros da casca
        if: ${{ !cancelled() }}
        timeout-minutes: 10
        run: >-
          python3 tools/carga-da-casca.py
          --roteiro tools/roteiros/aceite-de-mods-em-corrida.js
          … (um por arquivo, 34)
          --roteiro tools/roteiros/versao-do-servidor.js
```

O navegador é o que aquele passo instala no cache do Playwright, e o Python é
o do runner (3.12 no ubuntu-24.04). Nada novo entra no job.

**Rodado aqui como o Actions roda.** O `run:` foi lido do YAML com PyYAML,
gravado num arquivo e rodado com `bash -e` na raiz:

- verde: `roteiros: os 34 passaram.`, e o passo sai 0;
- vermelho de propósito, com `exigir(false, "vermelho de propósito, do lote
  RT");` acrescentado ao `perfil-entrada.js`: os 34 rodaram, e o fim diz
  `roteiros: 1 de 34 reprovaram:` e
  `tools/roteiros/perfil-entrada.js: REPROVADO: vermelho de propósito, do lote RT`.
  O passo sai 1, e o roteiro volta idêntico (`cmp`, tocado).

**O passo nunca rodou no Actions.** O workflow é só `workflow_dispatch`, e o
primeiro disparo é a prova de verdade. A suposição de que no Linux o binário
mora em `…/chrome-headless-shell-linux64/chrome-headless-shell` vem do formato
do Mac, `…-mac-arm64/chrome-headless-shell`, e não foi medida.

## 4. A etapa do `publicar.sh`

A função `roteiros_da_casca` é uma `etapa_da_bateria` com queixa, conserto e o
prazo de sempre (`PRAZO_DA_ETAPA_S`):

- queixa: «um roteiro da casca reprovou, ou falta nesta máquina o navegador
  que a carga usa.»
- conserto: «Leia o fim da saída: ele diz qual roteiro reprovou e com que
  frase. Sem o navegador, «npx playwright install chromium» o instala.»
- o comando: a mesma chamada da carga, com a lista inteira.

A `bateria()` a chama logo depois das bancadas de MOD (`check-runtime`), porque
ela não compila nada. O `--sem-bateria` a pula com o resto, porque ela mora
dentro da `bateria()`, depois do `return` do `SEM_BATERIA`. A linha do release
continua dizendo isso:
`--linha-da-bateria "macos linux" --sem-bateria` →
«**Esta versão saiu com --sem-bateria: não foi testada antes de publicar.**».

**Rodada sozinha, sem publicar nada.** Um script tira do `publicar.sh`, por
`sed`, só `morrer`, `etapa_da_bateria` e `roteiros_da_casca`, põe `RAIZ`,
`TEMPORARIO` e `PRAZO_DA_ETAPA_S`, e chama a etapa. Nada do resto do script
roda:

- verde: volta 0 em 15 s (os 9 s da carga, mais o relógio de 5 em 5 s da
  `etapa_da_bateria`), terminando em `roteiros: os 34 passaram.`;
- com o `sala.js` vermelho de propósito: `morrer` com a queixa, o conserto e as
  20 últimas linhas, que terminam em `roteiros: 1 de 34 reprovaram:` e
  `tools/roteiros/sala.js: REPROVADO: vermelho de propósito, do lote RT`. O
  `sh` sai 1, e o roteiro volta idêntico (`cmp`, tocado).

**A bancada do `publicar.sh`** (`xtask/tests/empacotamento.rs`) roda o script
inteiro num repositório de mentira, com `curl`, `cargo`, `ssh` e `docker`
dublês. Ela é parte do `cargo test -p xtask`. Com a etapa nova, **quatro testes
dela reprovaram (medido)**: os que passam da etapa (`os_crates_do_celular_…`,
`um_check_api_…`, `um_check_versao_…`, `um_check_vetores_…`), porque o
repositório de mentira não tinha a carga. Ela ganhou um dublê da carga, que
anota a chamada e reprova com `FALSO_ROTEIROS`, e dois testes:

- `um_roteiro_da_casca_vermelho_para_a_publicacao_antes_de_empacotar`: a etapa
  roda com a lista, a publicação para antes da etapa seguinte e de empacotar, e
  o roteiro e a frase chegam a quem publica;
- `o_sem_bateria_pula_tambem_os_roteiros_da_casca`.

## 5. O guarda

Os testes estão em `xtask/src/check_runtime.rs`, no molde do
`toda_bancada_da_pasta_roda_em_algum_lugar`:

- **`todo_roteiro_da_pasta_roda_no_ci_e_no_publicar`** lê três coisas:
  1. a pasta, pelo sistema de arquivos, e não por um literal;
  2. no `ci.yml`, sem comentário, os passos do job `bancadas`, cujo `run:` (o
     bloco `>-` juntado) começa por `python3 tools/carga-da-casca.py`;
  3. no `publicar.sh`, sem comentário, a `etapa_da_bateria` dentro de
     `roteiros_da_casca`. As linhas com `\` são juntadas, as palavras são
     separadas respeitando as aspas, e o comando é o que vem depois da queixa e
     do conserto.

  Ele exige que a `bateria()` chame `roteiros_da_casca` numa linha só dela, e
  que cada arquivo da pasta seja um `--roteiro tools/roteiros/<nome>` nas duas
  chamadas, sem nome que a pasta não tenha. Uma palavra que não é um desses
  pares reprova: um guarda que pulasse o que não entende contaria errado.
- **`o_passo_dos_roteiros_roda_depois_de_uma_vermelha_tem_prazo_e_vem_depois_do_navegador`**:
  há um passo só com a carga, ele tem `if: ${{ !cancelled() }}` e
  `timeout-minutes`, e vem depois do `npx playwright install --with-deps chromium`.

Os dois foram vistos vermelhos antes do passo e da etapa existirem: «não roda
estes roteiros da pasta: [os 34]», «tem 0» e «a `bateria()` … não chama mais
`roteiros_da_casca`».

**Por nome, e não um laço sobre a pasta.** Com um laço, o guarda só provaria
que o laço existe; um filtro dentro dele (`case … continue`) passaria. Com a
lista, ele prova cada arquivo. O precedente é o das bancadas.

## 6. As reversões

Em cada linha da tabela, um arquivo foi mudado, o teste rodou, e o arquivo
voltou **idêntico (`cmp`) e tocado**. O roteiro novo foi criado e depois
apagado. A árvore ficou limpa no fim (`git status` vazio).

| | mudança | teste | resultado |
|---|---|---|---|
| R1 | tirar `toque.js` da lista do CI | `todo_roteiro_…` | vermelho: «não roda estes roteiros da pasta: ["toque.js"]» |
| R2 | o mesmo, deixado num comentário `# --roteiro …toque.js` | `todo_roteiro_…` | vermelho, a mesma frase: comentário não conta |
| R3 | um `novo-do-lote-rt.js` na pasta, fora das listas | `todo_roteiro_…` | vermelho: «["novo-do-lote-rt.js"]» |
| R4 | apagar a chamada `roteiros_da_casca` da `bateria()` | `todo_roteiro_…` | vermelho: «a `bateria()` … não chama mais `roteiros_da_casca`» |
| R4c | a chamada só num comentário da `bateria()` | `todo_roteiro_…` | vermelho, a mesma frase |
| R5 | tirar `sala.js` da lista do `publicar.sh` | `todo_roteiro_…` | vermelho: «a bateria não roda estes roteiros da pasta: ["sala.js"]» |
| R5b | `sala.js` trocado por `sala-velha.js` no `publicar.sh` | `todo_roteiro_…` | vermelho: falta `sala.js` e sobra `sala-velha.js` |
| R6 | tirar o `if` do passo do CI | `o_passo_dos_roteiros_…` | vermelho |
| R6b | tirar o `timeout-minutes` do passo do CI | `o_passo_dos_roteiros_…` | vermelho |
| R7 | apagar a chamada da `bateria()` | `um_roteiro_da_casca_vermelho_…` (bancada do `publicar.sh`) | vermelho: o diário mostra a publicação de mentira indo até os `curl` de upload com um roteiro reprovado |
| C1 | carga sem o conserto da aspa | sonda da aspa | 0 revertido, 1 consertado |
| C2 | carga sem os vários `--roteiro` | `--roteiro <vermelho> --roteiro perfil-entrada.js` | 0 revertido, 1 consertado |

Os roteiros reprovados de propósito (`perfil-entrada.js` no passo do CI,
`sala.js` na etapa) voltaram idênticos (`cmp`, tocados).

## 7. O que rodou

Tudo antes da pausa da orquestradora:

- `cargo test -p xtask`: **220 passaram, 0 falharam** (79 no `xtask`, 120 em
  `empacotamento`, 21 nos outros alvos);
- `cargo xtask check-runtime`: 0;
- `cargo clippy -p xtask --all-targets -- -D warnings`: limpo;
- `cargo fmt -p xtask --check`: limpo;
- `sh -n empacotar/publicar.sh`: ok;
- os 34 roteiros, cada um sozinho, em cinco rodadas (antes e depois de cada
  mudança da carga), e numa chamada só: todos 0.

Não rodei nenhuma conformidade, nenhum `cargo test --workspace` e nunca o
`publicar.sh` de ponta a ponta. A bancada do `publicar.sh` roda o script num
repositório de mentira, com `curl` dublê, como sempre rodou dentro do
`cargo test -p xtask`. Não fiz push nem publiquei nada, e não mexi no
`manifesto.py`.

## 8. Preocupações

1. **Dois roteiros podres e verdes**, `versao-do-servidor.js` e
   `trocar-de-janela.js` (§1). Agora eles rodam no CI e na bateria, e
   continuam sem provar o que dizem. Um terceiro diverge do cabeçalho sem eu
   ter analisado: `compartilhar.js`, com o `compartilhar-quadros` que ainda
   existe.
2. **O conserto da aspa (`d2bd9d0d`) e o `--no-sandbox` (`be0ec125`) não
   estavam no brief.** Cada um está num commit só dele. O segundo não foi
   medido no runner.
3. **O passo do CI nunca rodou no Actions.** O caminho do navegador no Linux é
   inferido, e a caixa de areia é o item 2.
4. **O prazo da `etapa_da_bateria` fala do `seele-video`.** Um roteiro que
   pendure vai receber essa frase depois de 40 min. A última linha do registro
   nomeia o roteiro, porque cada um é dito antes de rodar, mas a frase engana.
   Não mexi nela.
5. **A linha positiva do release** («Bateria: rodou aqui (fmt, clippy, testes,
   cargo deny)») não cita os roteiros, como já não citava as bancadas de MOD
   nem o `cargo xtask web`. Não mexi nela, e o teste
   `a_linha_da_bateria_diz_o_que_rodou_e_onde` a fixa.
6. **O `console.error` não reprova** (§1). O brief supõe que sim.
7. **O W3 subestimou o corte da aspa.** O relatório dele diz que se perdia «só
   o resto de uma frase já reprovada», e o caso medido aqui é uma reprovação
   inteira que some.
