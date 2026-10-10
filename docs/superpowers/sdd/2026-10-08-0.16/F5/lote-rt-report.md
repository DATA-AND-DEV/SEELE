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
Tudo o que está nas seções 1 a 8 foi rodado **antes** dela. Este relatório foi
escrito e commitado durante a pausa, sem rodar nada.

**Rodada 1 (verde):** as decisões da orquestradora sobre as preocupações do
§8. Elas foram escritas durante a pausa de CPU e rodaram depois do aviso. Os 34
roteiros passam, e os três que estavam podres exigem agora o que dizem. A etapa
dos roteiros tem a sua frase de prazo, e a linha do release cita os roteiros.
Todas as reversões saíram vermelhas onde deviam. Ver o §9.

**Rodada 2 (verde):** os quatro achados da revisão (I1, M1, M2, M3), um commit
cada, com o vermelho antes de cada conserto. Ver o §10. **Um incidente:** numa
das sondas desta rodada eu matei os navegadores de outro agente (§10.5).

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

## 9. Rodada 1: as decisões da orquestradora (verde)

Escrita durante a pausa de CPU e commitada como «em curso, não rodado»
(`ebe6e79a`, `8a634e07`, `8b8a682a`). Ela rodou depois do aviso da
orquestradora, e o resultado está no §9.5.

### 9.1 Os três roteiros podres entram no lote

**`trocar-de-janela.js`.** O retrato agora é o de hoje:
`minha_transmissao` com o `TelaEmCurso` de `seele-ffi` (`tela`, `de`,
`e_minha`, `espectadores`, `parada`, `pedido`), e `tela` para a transmissão
de outra pessoa. São seis exigências, nos estados que o cabeçalho diz:

| | estado | exige |
|---|---|---|
| T1 | nada escolhido | `COMPARTILHAR (apagado) · modo=comecar` |
| T2 | fonte escolhida, nada no ar | `COMPARTILHAR · modo=comecar` |
| T3 | transmitindo a fonte armada | `TROCAR (apagado) · modo=trocar`, title «é esta que já está no ar…» (recusa recomeçar a mesma) |
| T4 | transmitindo, outra fonte escolhida | `TROCAR · modo=trocar`, title «a transmissão de agora para…» |
| T5 | a transmissão é de outra pessoa | `COMPARTILHAR · modo=comecar`, aceso |
| T6 | a minha acabou por fora | `fonteEmCurso === null` |

O cabeçalho dizia que a de outra pessoa ficava «recusado, com a frase de
sempre». Isso deixou de valer com o R18 da revisão da v15: o servidor guarda
várias transmissões por sala. O T5 exige o que o produto faz hoje, e o
comentário diz por quê.

Reversões preparadas:
- **T0**, o vermelho de antes: o roteiro novo com o retrato velho
  (`tela: { e_minha: true }`) no T3.
- **T1–T6**, uma por exigência, cada uma uma mudança de uma linha em
  `camada-compartilhar.js`:
  - T1: sem o `fonteArmada === null` no `disabled`;
  - T2: `disabled` por `!minha`;
  - T3: `mesmaFonte = false`;
  - T4: o title de trocar vazio;
  - T5: `minha_transmissao ?? tela`;
  - T6: sem o `fonteEmCurso = null` de quando a minha acaba.

**`versao-do-servidor.js`.** O `analisar_convite` responde agora o
`ConviteLido` de hoje (`alvo`, `token`, `versao`, `para_entrar`):

- primeiro, `para_entrar: { Abrir: { linha: "0.15", servidor: "0.15",
  versao: "0.15.2", perde_conversas: false } }`;
- depois, `para_entrar: "Aqui"`, como o `serde` serializa as variantes
  unitárias.

O roteiro aperta o ABRIR da pergunta (`versoes-confirmar-sim`), que antes não
existia para ele. São sete exigências:

| | exige |
|---|---|
| V1 | o link de outra linha abre a pergunta (`versoes-confirmar` VISIVEL), e a frase diz «Servidor 0.15» |
| V2 | antes do ABRIR, nem `abrir_versao` nem `connect` |
| V3 | o ABRIR chama `abrir_versao` com `linha: "0.15"` |
| V4 | com o link inteiro (`link === LINK`) |
| V5 | com `hospedar: false` |
| V6 | e esta janela não conecta depois |
| V7 | com `para_entrar: "Aqui"`, conecta aqui e não abre outra |

Reversões preparadas:
- **V0**, o vermelho de antes: o roteiro novo com a resposta velha
  (`pode_abrir_naquela_versao`).
- **V1–V7**, uma por exigência:
  - V1: a pergunta pulada (`if (false)`);
  - V2: sem o `return` depois do `entrarPelaLinha`, e a janela conecta
    junto;
  - V3: `linha: null`;
  - V4: `link: null`;
  - V5: `hospedar: true`;
  - V6: um `irParaOServidor` depois do `abrir_versao`;
  - V7: `if (ir)` no lugar de `if (ir && typeof ir === "object")`.

**`compartilhar.js`: o roteiro estava desatualizado, e o produto está certo.**
A história:

- O roteiro é de `1e485bfc` (01/09/2026), quando a caixa passou a perguntar
  «uma coisa só», e ele espera ver fora a prioridade, a altura, os quadros e
  a banda.
- Os quadros voltaram em `c048f752` (21/09), de propósito, como escolha de
  teto de 30 ou 60. `Q8` e `Q15` ficaram de fora como piso automático. O
  commit diz isso, e `limitesEscolhidos()` lê `compartilhar-quadros`.
- O INCLUIR SOM entrou pelo R21.

O roteiro novo diz o que a caixa pergunta hoje e exige quatro coisas:

| | exige |
|---|---|
| C1 | o botão do rodapé abre a caixa |
| C2 | os controles de escolha da caixa (`select`, `input`, `textarea`) são exatamente `compartilhar-quadros`, `compartilhar-resolucao` e `compartilhar-som`: «e mais nada» |
| C3 | a prioridade, a altura e a banda continuam fora, em qualquer forma |
| C4 | os quadros oferecem 30 e 60, e só |

Reversões preparadas:
- **C0**, o vermelho de antes: os quadros de volta à lista dos que saíram,
  que é o que o roteiro velho esperava.
- **C1–C4**, uma por exigência:
  - C1: a caixa que não se mostra;
  - C2: um `<input>` a mais na caixa;
  - C3: um `<p id="compartilhar-altura">`, que não é controle e por isso só a
    C3 pega;
  - C4: uma opção de 15 quadros.

### 9.2 A frase de prazo vencido da etapa dos roteiros

A `etapa_da_bateria` não aceitava a frase. Ela agora aceita um
`--prazo <função>` opcional, antes da queixa, e as outras onze chamadas
continuam iguais.

- Quando o prazo vence, a função é chamada depois de o comando ser
  interrompido e morre dizendo o que travou. Sem `--prazo`, vale a frase de
  sempre, a do `seele-video`.
- A etapa dos roteiros passa `--prazo prazo_dos_roteiros`. Essa função lê no
  registro o último `roteiro: rodando <caminho>`, que a carga diz antes de
  cada um, e escreve «O roteiro que travou: <caminho>.», o comando para
  rodá-lo sozinho com `--tudo`, e as 20 últimas linhas.
- O guarda passou a pular o `--prazo <função>` antes da queixa.
- O teste `a_bateria_tem_prazo_e_nao_espera_para_sempre` continua lendo o
  `kill -9` no corpo da `etapa_da_bateria`, que não mudou de lugar.

Na bancada do `publicar.sh`, o dublê da carga ganha
`FALSO_ROTEIROS=pendura`: ele diz dois roteiros e trava no segundo,
`toque.js`. O teste novo
`um_roteiro_que_trava_e_nomeado_quando_o_prazo_da_etapa_vence` roda com
`SEELE_PRAZO_DA_ETAPA_S=5` e exige:
- a publicação parada;
- «O roteiro que travou: tools/roteiros/toque.js.»;
- nenhum `seele-video`;
- nada empacotado.

Reversão preparada: **P1**, sem o `--prazo`.

### 9.3 A linha positiva do release

Agora ela diz: «Bateria: rodou aqui (fmt, clippy, testes, cargo deny, roteiros
da casca).», e com o Windows, «… e no Windows (testes).». O teste
`a_linha_da_bateria_diz_o_que_rodou_e_onde` fixa o texto novo nas duas
asserções. Reversão preparada: **L1**, o texto de antes.

### 9.4 Fora deste lote, para o relatório

- **O `console.error` não reprova na carga.** O brief supunha que sim. A
  carga reprova em `Uncaught …Error` (inclusive `(in promise)`), em
  `REPROVADO`, em `ROTEIRO ESTOUROU` e no roteiro que não chega ao fim. A
  carga não muda.
- **Os 13 que só provam que a sessão abre** (o `exigir` do `entrarNaSessao`),
  para a pendência 58. Eram 13: estes 11, mais `trocar-de-janela` e
  `compartilhar`, que ganham `exigir` próprio na rodada 1. Ficam **11**:
  `avatar-na-chamada`, `barra-mac`, `clique-nos-glifos`, `icone-do-server`,
  `icone-na-trilha`, `medidas`, `palco`, `perfil-salva-sessao`,
  `perfil-sessao`, `rodape-do-operador`, `tela-de-outra-versao`.
- **Os 7 sem `exigir` nenhum**, também para a pendência 58. Depois da rodada
  1, que dá `exigir` a `versao-do-servidor` (o sétimo), são **6**: `atualizacao`,
  `botoes-da-entrada`, `dispositivos`, `perfil-entrada`, `perfil-salva`,
  `renomear-canal`.
- **O `--no-sandbox` e o caminho do navegador no Linux**: só o primeiro
  disparo do `ci.yml` no Actions vai dizer.

### 9.5 O que rodou na rodada 1

Tudo depois do aviso do fim da pausa:

- **Os 34 roteiros**, um a um: todos 0. Os três novos relatam o que exigem.
  - `trocar-de-janela.js`: COMPARTILHAR apagado, COMPARTILHAR aceso, TROCAR
    apagado («é esta que já está no ar…»), TROCAR aceso («a transmissão de
    agora para…»), COMPARTILHAR aceso, `fonteEmCurso=null`.
  - `versao-do-servidor.js`: a pergunta VISIVEL com «Servidor 0.15»; nada
    aberto nem conectado antes do ABRIR; depois, `abrir_versao` com a linha
    0.15, o link inteiro e `hospedar: false`, e sem `connect`; a mesma linha
    conecta aqui.
  - `compartilhar.js`: a caixa VISIVEL; ela pergunta `compartilhar-quadros`,
    `compartilhar-resolucao` e `compartilhar-som`; nenhum dos que saíram
    voltou; quadros 30 e 60.
- **O passo do CI**, rodado como o Actions roda (`bash -e`): «roteiros: os 34
  passaram.», 0. **A etapa do `publicar.sh` sozinha:** 0.
- **`cargo test -p xtask`: 221 passaram, 0 falharam** (79 no `xtask`, 121 em
  `empacotamento` com o teste novo do prazo, 21 nos outros alvos).
  `cargo xtask check-runtime`: 0. `cargo clippy -p xtask --all-targets -- -D
  warnings`: limpo. `sh -n empacotar/publicar.sh`: ok.
- `cargo fmt -p xtask` reformatou uma asserção do teste da linha, e isso entra
  no commit desta seção.

**As reversões.** Cada uma mudou um arquivo, rodou a carga ou o teste e
devolveu o arquivo **idêntico (`cmp`) e tocado**. Todas saíram vermelhas na
exigência que deviam derrubar, e só nela:

| | mudança | o que reprovou |
|---|---|---|
| T0 | o retrato velho (`tela.e_minha`) no T3 | «transmitindo a fonte armada, o botão não recusa recomeçar para a mesma: COMPARTILHAR · modo=comecar» |
| T1 | `disabled` sem `fonteArmada === null` | «sem fonte escolhida, o botão não está apagado» |
| T2 | `disabled` por `!minha` | «com uma fonte escolhida e nada no ar, o botão não é COMPARTILHAR aceso: COMPARTILHAR (apagado)» |
| T3 | `mesmaFonte = false` | «… não recusa recomeçar para a mesma: TROCAR · modo=trocar» |
| T4 | o title de trocar vazio | «… não oferece TROCAR: TROCAR · modo=trocar · title «»» |
| T5 | `minha_transmissao ?? tela` | «com a transmissão de outra pessoa na tela, o botão não é COMPARTILHAR aceso: TROCAR» |
| T6 | sem o `fonteEmCurso = null` | «a transmissão acabou por fora e a fonte em curso não foi esquecida: 8» |
| V0 | a resposta velha do `analisar_convite` | «o link de outra linha não perguntou antes de abrir: versoes-confirmar escondida» |
| V1 | a pergunta pulada | a mesma frase do V0 |
| V2 | sem o `return` depois do `entrarPelaLinha` | «antes da resposta da pessoa já houve … connect …» |
| V3 | `linha: null` | «o ABRIR não abriu o SEELE da linha do servidor: abrir_versao true, linha null» |
| V4 | `link: null` | «… não recebeu o link inteiro …: null» |
| V5 | `hospedar: true` | «entrar num servidor de outra linha pediu para hospedar: true» |
| V6 | `irParaOServidor` depois do `abrir_versao` | «depois de abrir o SEELE da outra linha, esta janela também conectou» |
| V7 | `if (ir)` | «um servidor da linha deste app não conectou aqui: abrir_versao false, connect false» |
| C0 | os quadros entre os que saíram (o roteiro velho) | «voltou à caixa o que tinha saído: compartilhar-quadros» |
| C1 | a caixa que não se mostra | «o botão do rodapé não abriu a caixa de compartilhar: escondida» |
| C2 | um `<input>` a mais | «a caixa pergunta outra coisa além da resolução, dos quadros e do som: compartilhar-prioridade-nova …» |
| C3 | um `<p id="compartilhar-altura">` | «voltou à caixa o que tinha saído: compartilhar-altura» (a C2 passou: um `<p>` não é controle) |
| C4 | uma opção de 15 quadros | «os quadros oferecem outra coisa que 30 e 60: 15 30 60» |
| P1 | a etapa dos roteiros sem `--prazo` | `um_roteiro_que_trava_…`: «o prazo dos roteiros venceu e a frase não diz qual travou». A saída mostra a frase velha, «O caso conhecido é o `seele-video`…», com «roteiro: rodando tools/roteiros/toque.js» só nas últimas linhas |
| L1 | a linha do release de antes | `a_linha_da_bateria_diz_o_que_rodou_e_onde`: «a linha não diz que a bateria rodou, nem o que ela rodou» |

Nenhum roteiro reprovou contra o produto. Nada reprovou por tempo, e nada
precisou rodar de novo. Não rodei conformidade e não rodei o `publicar.sh` de
ponta a ponta.

## 10. Rodada 2: os achados da revisão

A revisão (`lote-rt-review-report.md`) pediu conserto, com 0 crítico, 1
importante e 3 menores. Cada achado é um commit:

| commit | achado |
|---|---|
| `e5426c91` | I1: o guarda lê o `run` do CI e o `publicar.sh` como o shell lê |
| `25a06d5f` | M1: um relato que não é texto não derruba a carga, e um defeito do aparelho vira motivo |
| `392aa439` | M2: um prazo por roteiro dentro da carga, que mata o navegador que ela abriu |
| `a5a1886d` | M3: um autoteste da carga, sem navegador, que o `cargo test` roda |

### 10.1 I1: o guarda casava com uma nota dentro do bloco `>-`

No bloco `>-` do YAML, o `#` é conteúdo. O guarda o tirava linha a linha antes
de dobrar. O Actions, depois de dobrar, o lê como comentário de shell e corta o
resto da chamada.

**Medido antes do conserto**, com o guarda verde nos três casos:
- **G1**, a sonda do revisor: `--roteiro tools/roteiros/toque.js  # F7: só faz
  sentido no desktop` no CI.
- **G2**, `--roteiro tools/roteiros/sala.js \  # nota` no `publicar.sh`.
- **G3**, `--roteiro tools/roteiros/sala.js \ ` (barra e espaço) no
  `publicar.sh`.

**O `publicar.sh` tinha a mesma brecha por outro caminho.** O guarda cortava o
comentário e o espaço do fim antes de juntar as continuações. Medido com o
`sh` do macOS: depois de `\  # nota` e depois de `\ `, a linha **não**
continua, e o resto da chamada vira outro comando («b: command not found»).

**O conserto.**
- O passo do CI é lido com o texto que o YAML entrega: com o `#`, e dobrado
  como o YAML dobra. Uma linha com outro recuo dentro do bloco reprova.
- As funções do `publicar.sh` são lidas sobre o texto com os comentários.
- Os dois passam por um só leitor, `comandos_do_shell`:
  - a continuação só vale com a `\` no fim da linha;
  - `\ ` é um espaço escapado, e não continuação;
  - o `#` comenta só no começo de uma palavra, fora de aspas;
  - a quebra de linha separa comandos.
- O teste `o_leitor_de_shell_do_guarda_le_como_o_sh` fixa os casos que o `sh`
  respondeu.

**Depois do conserto:**
- G1 sai vermelho: «o job `bancadas` não roda estes roteiros da pasta:
  ["trocar-de-janela.js", "versao-do-servidor.js"]», os mesmos dois que o
  revisor mediu.
- G2 e G3 saem vermelhos: «a chamada da carga tem [" "], que não é um
  `--roteiro …`».
- R1–R6b continuam vermelhos. O R2, a linha inteira comentada, agora reprova
  pelo motivo certo: ela corta tudo o que vem depois («toque.js,
  trocar-de-janela.js, versao-do-servidor.js»).

### 10.2 M1: um relato que não é texto derrubava a carga

Medido antes do conserto: `relatar(42)` dá `AttributeError: 'int' object has
no attribute 'startswith'`, e `relatar(undefined)` dá o mesmo com `NoneType`.
Numa lista, o traceback parava tudo ali.

O conserto tem duas camadas, cada uma com a sua prova:

- **`relatar` empurra `String(linha)`.** As sondas passam e relatam «42» e
  «undefined».
  - Reversão M1a, sem o `String`: o roteiro reprova com «a carga caiu neste
    roteiro: AttributeError». A camada de baixo pega o erro, e a lista segue.
- **`carregar_sem_cair` envolve cada carga.** Um defeito do aparelho vira
  motivo daquele roteiro, com o traceback escrito.
  - Sonda: um `SEELE-RELATO %E0%A4%A` que não se lê. O roteiro reprova com
    «a carga caiu neste roteiro: JSONDecodeError», o seguinte roda e o resumo
    sai.
  - Reversão M1b, a lista chamando `carregar` direto: o traceback para a lista.

A falta do navegador continua parando tudo, de propósito: ela é `SystemExit`.

### 10.3 M2: um roteiro que trava deixava órfãos

Medido antes do conserto com a sonda `for (;;) {}`. A `trava.py` dá um prazo
de fora de 25 s e mata só a carga, como o `kill -9` da etapa. A carga não
terminou. Três segundos depois, sobravam 5 `chrome-headless-shell` dela, com o
renderer a 99,8% de CPU.

O conserto é `rodar_navegador`: o navegador abre num grupo de processos só
dele, com `communicate(timeout=…)`.
- O prazo vem de `--prazo-por-roteiro`, com 60 s por padrão.
- Quando ele vence, a carga mata o grupo e o roteiro reprova com «o roteiro não
  terminou em N s, e o navegador que a carga abriu foi morto com os filhos
  dele». A lista segue.
- O prazo da etapa no `publicar.sh` continua como rede de segurança.

Com `--prazo-por-roteiro 10`, a carga sai sozinha em 10,4 s, com 1. O
`perfil-entrada.js` seguinte passa, o resumo nomeia a sonda, e não sobra
navegador.

Reversões:
- **M2b**, sem o `timeout`: vermelho. A carga não termina em 40 s, e sobram 5
  navegadores dela, com um renderer a 100%.
- **M2a**, matar só o processo principal: **verde no macOS**. Com o
  `chrome-headless-shell`, matar o principal bastou. A necessidade do grupo vem
  do autoteste (§10.4, A5): um navegador de mentira com um filho segura a saída
  por 30 s quando só o principal morre. No Linux do CI não foi medido.

### 10.4 M3: o autoteste da carga, no `cargo test`

`python3 tools/carga-da-casca.py --autoteste` roda cinco casos sem abrir
navegador, sobre o mesmo código que a carga usa:
- **o relato atravessa a aspa:** uma linha do console no formato do Chromium,
  com uma aspa antes do REPROVADO, lida por `ler_saida`, que é a leitura de
  verdade;
- **vários `--roteiro` numa chamada:** pelo `argumentos_da_carga`;
- **a lista roda todos e diz quem reprovou:** `varios` com uma carga de
  mentira;
- **a carga não cai;**
- **o prazo mata o navegador e os filhos:** um «navegador» de mentira que abre
  um filho e dorme.

O autoteste leva cerca de 1 s. Quem o roda é o teste
`a_carga_da_casca_passa_no_proprio_autoteste`, em `check_runtime.rs`. Antes do
autoteste existir, esse teste saiu vermelho («unrecognized arguments:
--autoteste»).

**Por que este, e não roteiros-sonda no passo do CI.** É o mais barato que é
de verdade:
- não precisa de navegador;
- roda em todo `cargo test`, isto é, no CI, no `validar` do `release.yml` e na
  bateria do `publicar.sh`;
- os roteiros-sonda só rodariam no job de navegador, que é manual, e pediriam
  um modo de «esperar o vermelho».

O teste é só Unix: é onde a carga roda e onde o prazo mata um grupo. A metade
do JS do relato, o `encodeURIComponent`, só roda num navegador. Revertida, todo
roteiro reprova alto: medido no J1, com «a carga caiu neste roteiro:
JSONDecodeError».

Reversões, cada uma vermelha só no caso dela, todas pelo `cargo test`:

| | reversão | o caso que falhou |
|---|---|---|
| A1 | a leitura de antes (`[^"]*` e o `split`) | «o relato atravessa a aspa: o relato voltou como ['%5B%22com…'], e não como ['com "aspas" aqui', …]» |
| A2 | sem o `action="append"` | «vários --roteiro numa chamada: dois --roteiro viraram PosixPath(…perfil-sessao.js)» |
| A3 | a lista para no primeiro vermelho | «a lista roda todos e diz quem reprovou: a lista rodou [perfil-entrada.js], e não os dois» |
| A4 | sem a rede de `carregar_sem_cair` | «a carga não cai: a carga caiu: RuntimeError: um defeito do aparelho» |
| A5 | matar só o processo principal | «o prazo mata o navegador e os filhos: o prazo venceu, mas a carga esperou 30.0 s: um filho do «navegador» continuou vivo e segurou a saída» |

### 10.5 Incidente: matei os navegadores de outro agente

A primeira versão da `trava.py`, no scratchpad deste lote, listava **todos** os
`chrome-headless-shell` da máquina. Na limpeza, ela matava os que tinham
aparecido durante a sonda.

Na reversão M2b, entre cerca de 20:03 e 20:05 de 10/10/2026, isso matou um
grupo de navegador do Playwright que **não era meu**:
- os PIDs 99743 a 99746 e 252;
- o grupo vinha com `--disable-field-trial-config`, que é flag do Playwright,
  e a carga não a usa;
- logo depois vi outro agente rodando `rev.py R6b … -- node
  apps/seele-app/bancada/ajustes-v013.cjs`. É provável que fosse dele.

**Uma bancada desse agente pode ter saído vermelha por isso, e não pelo
código.** Se a reversão de alguém deu um vermelho inesperado nesse intervalo,
vale rodar de novo.

A `trava.py` agora só olha o grupo de um navegador aberto com
`--virtual-time-budget`, que só a carga passa. Na M2b, rodada de novo, ela
achou e matou só os 5 da carga, e os navegadores dos grupos 1631 e 2255, de
outros, ficaram vivos.

### 10.6 O que rodou na rodada 2

- **Os 34 roteiros**, um a um: todos com 0.
  - O passo do CI, rodado como o Actions roda (`bash -e`): «os 34 passaram»,
    0.
  - A etapa do `publicar.sh` rodada sozinha: 0.
- **`cargo test -p xtask`: 223 passaram, 0 falharam**: 81 no `xtask`, com o
  leitor de shell e o autoteste; 121 em `empacotamento`; 21 nos outros alvos.
- `cargo xtask check-runtime` saiu 0. `cargo clippy -p xtask --all-targets --
  -D warnings`, `cargo fmt -p xtask --check` e `sh -n empacotar/publicar.sh`
  passaram limpos.
- Todas as reversões voltaram idênticas (`cmp`) e foram tocadas: G1–G3,
  R1–R6b, M1a, M1b, M2a, M2b, A1–A5 e J1. O roteiro novo do R3 foi apagado, e
  o `git status` ficou vazio.

**O `palco.js` é sensível ao tempo, e nada o exige.** Em 1 das 8 rodadas que
guardei, a da rodada 1, logo depois da pausa de CPU, ele relatou «canvas:
ESCONDIDO 0x0», com a fila do decodificador em 5. Nas outras 7, relatou
«visivel 960x540». Ele é um dos 11 que só provam que a sessão abre (§9.4), e
passou nas duas. É mais um para a pendência 58.

Não rodei conformidade, não rodei o `publicar.sh` de ponta a ponta e não fiz
push.
