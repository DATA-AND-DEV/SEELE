# Relatório do lote J (F5, 0.16)

A tecla de falar dita pelo nome, o aviso de fone e o nome pedido no primeiro uso. Item F1-tecla-fone-nome.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-af779c032683008c5`
- Branch: `0.16/lotes-j`, aberto em `3b0b6a7a`.
- Status: DONE_WITH_CONCERNS. As preocupações estão no fim, e nenhuma bloqueia.

## As partes e os commits que as fecham

| Parte do brief | Guarda | Commit |
|---|---|---|
| 1. A tecla: `nomeDaTecla` desce para `base.js`, as duas linhas de estado dizem «microfone abre enquanto segura ESPAÇO», e a ajuda escreve a tecla gravada | (a) `a_linha_de_estado_nomeia_a_tecla_de_falar`, (b) `a_ajuda_escreve_a_tecla_de_falar_que_esta_gravada` | `307b18e0` |
| 2. O fone: a faixa na primeira entrada numa sala de voz de cada execução, e a mesma frase na ajuda e em MICROFONE E SOM | (c) `ha_um_aviso_de_fone_em_algum_lugar_que_a_pessoa_le` | `092e6a5b` |
| 3. O nome: sem apelido, o primeiro `conectar` da execução pergunta antes do `connect` | (d) `sem_nome_o_primeiro_conectar_pergunta_antes` | `acbc1c29` |

O F1-tecla-fone-nome fecha com os três commits.

## O que foi feito

### 1. A tecla (`307b18e0`)

- **`nomeDaTecla`** saiu de `tela-server.js` e foi para `base.js`, logo depois de `digitando()`. O comentário «Este é o único lugar do produto que nomeia teclas» foi junto, com um parágrafo novo que diz por que ela mora ali: as duas telas e a ajuda a usam, e `base.js` carrega antes de todas. Em `tela-server.js` ficou uma nota de três linhas no lugar da função, como as outras notas de «saiu» do repositório.
- **As duas linhas de estado.** Em `desenharOperador(snapshot)` (`tela-sessao.js`) e em `desenharContextoDaSessao()` (`tela-server.js`), a entrada `PushToTalk` virou `` `microfone abre enquanto segura ${nomeDaTecla(teclaDeFalar)}` ``. `teclaDeFalar` é a mesma variável que o `keydown` compara. A linha do operador é redesenhada a cada quadro, e uma troca de tecla em ATALHOS aparece nela em meio segundo. **A faixa de contexto não** era: ela só era escrita ao abrir a configuração, antes de a leitura da tecla voltar, e continuava dizendo a tecla de antes depois de uma troca. A primeira versão deste relatório dizia o contrário, e estava errada. O conserto é da rodada 1 (`4daaa4c3`, abaixo).
- **A ajuda.** O span ficou `<span id="ajuda-tecla-falar" class="ajuda-tecla">ESPAÇO</span>`, com o `id` antes da classe e o texto padrão ESPAÇO. `abrirAjuda()` (`camada-ajuda.js`) escreve `nomeDaTecla(teclaDeFalar)` nele a cada abertura. A linha de baixo passou a «Segure para falar, no modo tecla. Vale com a janela do SEELE na frente e fora da caixa de mensagem.».

### 2. O fone (`092e6a5b`)

- **`frases.js`**: `const AVISO_DE_FONE = "USE FONE. O SEELE ainda não cancela eco: sem fone, quem fala com você se ouve de volta.";`. É uma constante solta, e não uma entrada de dicionário, porque não há enum por trás. Por isso os guardas de tamanho (`DICIONARIOS`) não a medem. Ela tem 88 caracteres, e o teto é 180.
- **A faixa.** `<div id="aviso-fone" class="veredito" role="status" hidden>`, com `#aviso-fone-texto` e um botão «dispensar», logo depois de `#veredito`, fora de `<main>`. Assim ela fica de pé na conversa e na chamada. Ela usa as classes do veredito, porque nenhuma folha está na lista do lote. É o mesmo tipo de coisa: uma ressalva laranja, e não o vermelho do banner.
- **Quando acende.** `desenharAvisoDeFone(snapshot)`, chamada em `desenhar`, acende a faixa quando algum `voice_rooms[].occupied_by_us` é verdadeiro e a marca ainda não foi posta. Lê do retrato, e não do clique em `alternarCanal`, para valer qualquer que seja o caminho até a sala. Isso continua certo quando o lote I passar a abrir o áudio ao entrar na sala.
- **A marca por execução.** `let foneJaAvisado = false;` em `tela-sessao.js`, com o comentário dizendo que ela é por execução e não por sessão. É um sim ou não, e não um `Map` nem um `Set`, então o guarda de mapas de topo do lote I não a alcança. O comentário está lá do mesmo jeito.
- **A ajuda**: um verbete `FONE` em AS PALAVRAS, com `<dd id="ajuda-aviso-fone" …>` e a frase.
- **MICROFONE E SOM**: um `server-campo` «FONE», o primeiro do bloco SAÍDA DE SOM, com `<p id="server-aviso-fone" class="server-frase">` e a frase. Fica na saída porque o eco nasce nela, e antes da lista de aparelhos porque nenhum aparelho dela cancela eco.

### 3. O nome (`acbc1c29`)

- **Em `conectar`** (`tela-boot.js`), logo depois da leitura de `apelido_local` e antes de `if (apelido !== "") ultimoApelido = apelido;`:

  ```js
  if (apelido === "" && !jaPergunteiONome) {
    jaPergunteiONome = true;
    apelido = await perguntarONome();
  }
  ```

  `let jaPergunteiONome = false;` mora ao lado de `ultimoAlvo`. A pergunta é só da primeira entrada da execução, porque a reconexão depois de uma queda (`tela-fim.js`) e a batida automática da espera (`baterDeNovo`, `tela-auth.js`) também passam por `conectar`. Elas não podem parar atrás de uma caixa. Quem já tem nome, o desta visita ou o desta máquina, não é parado.
- **`abrirPerfil(titulo)`** (`camada-nomear.js`) ganhou o parâmetro opcional. Com ele, o título é o pedido. Sem ele e com o diálogo fechado, volta a «SEU PERFIL» e esconde a nota. Sem ele e com o diálogo aberto, o título fica. As reaberturas de dentro do diálogo (trocar a imagem, uma recusa ao gravar) não trazem título, e não podem trocar a pergunta no meio. O anúncio ao leitor de tela diz o título que está escrito.
- **`perguntarONome()`**, nova, em `camada-nomear.js`:
  - escreve `ENTRAR_SEM_NOME` na nota `#perfil-sem-nome` e a mostra;
  - abre o perfil com «COMO OS OUTROS VÃO TE CHAMAR?» e põe o foco no campo do apelido;
  - espera o diálogo fechar;
  - relê `apelido_local` e o devolve.

  Gravar é o SALVAR de sempre: `guardarOApelido`, que fora de sessão chama `escolher_apelido_local`. Se o diálogo não abrir, ela fecha e segue, para não prender a entrada.
- **`fecharPerfil()`** é a única saída do diálogo: SALVAR, CANCELAR, FECHAR, Escape e o clique fora passam todos por ela. Ela esconde a nota e solta quem espera (`aoFecharOPerfil`).
- **Fechar sem nome** segue como hoje. O `connect` sai com o apelido vazio, e o Rust deriva `pessoa-…`. A frase «Você vai entrar com um nome tirado da sua chave. Dá para trocar no perfil.» (`ENTRAR_SEM_NOME`, em `frases.js`) aparece na caixa enquanto ela está aberta e é anunciada ao fechar. Ela não diz o nome derivado, como o brief decidiu.
- **`index.html`**: `<p id="perfil-sem-nome" class="nota" hidden></p>` depois do campo do apelido. `<h2 id="perfil-titulo" …>SEU PERFIL</h2>` não mudou.
- `the_waiting_screen_says_what_happened_what_to_do_and_what_is_useless` e `the_shell_never_remembers_a_nickname_nobody_chose` seguem verdes: `levarParaAEspera(`, `erro.textContent = fraseDeErro(` e a linha do `ultimoApelido` estão onde estavam.

## TDD

### RED

Cada guarda foi escrito e rodado antes do conserto da sua parte.

Parte 1:

```
thread 'a_ajuda_escreve_a_tecla_de_falar_que_esta_gravada' panicked at apps/seele-app/tests/frontend.rs:9720:5:
a tecla de falar da ajuda perdeu o `id` por onde a tecla gravada é escrita, ou deixou de partir do padrão ESPAÇO — sem um, a ajuda ensina a tecla errada a quem trocou; sem o outro, ela abre vazia
thread 'a_linha_de_estado_nomeia_a_tecla_de_falar' panicked at apps/seele-app/tests/frontend.rs:9685:9:
a linha do operador diz que o microfone abre numa tecla sem dizer qual: quem trocou a tecla em ATALHOS segura a errada, e quem nunca trocou não sabe que é o ESPAÇO:
    PushToTalk: "microfone abre na tecla",
test result: FAILED. 0 passed; 2 failed; … 263 filtered out
```

Parte 2:

```
thread 'ha_um_aviso_de_fone_em_algum_lugar_que_a_pessoa_le' panicked at apps/seele-app/tests/frontend.rs:9771:5:
`frases.js` não tem mais o aviso de fone, e quem entra numa sala de voz sem fone faz o outro lado ouvir a própria voz de volta sem saber por quê
test result: FAILED. 0 passed; 1 failed; … 265 filtered out
```

Depois desse RED, a primeira asserção de (c) passou de um `contains` da linha inteira para um `assert_eq!` sobre os literais da constante (`literals_in`). Assim a quebra de linha do JS não decide nada. O RED da forma final é a reversão c1, abaixo.

Parte 3:

```
thread 'sem_nome_o_primeiro_conectar_pergunta_antes' panicked at apps/seele-app/tests/frontend.rs:11647:9:
a entrada não pergunta mais o nome a quem não tem nenhum, e a pessoa entra com um nome tirado da chave sem ter sido avisada:
{
  ultimoAlvo = alvo ?? ultimoAlvo;
…
test result: FAILED. 0 passed; 1 failed; … 266 filtered out
```

### GREEN

- `cargo test -p seele-app --test frontend`: 263 na base, 265 depois da parte 1, 266 depois da 2, 267 depois da 3. Todos verdes, entre eles:
  - `a_ajuda_so_promete_teclas_que_a_janela_atende`;
  - `o_padrao_da_tecla_de_falar_e_o_mesmo_nos_dois_lugares`;
  - `the_waiting_screen_says_what_happened_what_to_do_and_what_is_useless`;
  - `the_shell_never_remembers_a_nickname_nobody_chose`;
  - os de nomes de topo duplicados, de ids, de classes e do vocabulário do ADR 0033;
  - os do lote H, do P53 e da tela de versões da F6.
- `cargo xtask check-runtime`: saiu 0 depois de cada parte, com as 7 bancadas.
- `cargo fmt --all -- --check` e `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`: limpos antes de cada commit. O único aviso é o `ld: duplicate -rpath`, que é do ambiente. O fmt reflowou um `assert_eq!` de (d) antes do terceiro commit, e o `frontend` rodou de novo depois: 267 verdes.

## As reversões

Cada uma mudou um arquivo com `perl -0pi` sobre uma cópia guardada, rodou o guarda com `--exact`, e voltou da cópia. Antes e depois de cada bateria, o `shasum` de todos os `.js` de `ui/`, do `index.html` e do `frontend.rs` foi conferido com `cmp`. Saiu igual nas três baterias.

| Reversão | Arquivo | O guarda reprovou com |
|---|---|---|
| a1. `PushToTalk` de volta a `"microfone abre na tecla"` | `tela-sessao.js` | «a linha do operador diz que o microfone abre numa tecla sem dizer qual…» |
| a2. O mesmo | `tela-server.js` | «a faixa de contexto da configuração diz que o microfone abre numa tecla sem dizer qual…» |
| a3. `nomeDaTecla` renomeada em `base.js` | `base.js` | «`nomeDaTecla` saiu de `base.js`…» |
| b1. O span sem `id="ajuda-tecla-falar"` | `index.html` | «a tecla de falar da ajuda perdeu o `id`…» |
| b2. `abrirAjuda` sem a linha que escreve a tecla | `camada-ajuda.js` | «a ajuda abre sem escrever a tecla de falar que está gravada…» |
| b3. A linha de baixo sem «Vale com a janela…» | `index.html` | `left: "Segure para falar, no modo tecla."` |
| c1. Sem `AVISO_DE_FONE` | `frases.js` | «`frases.js` não tem mais o aviso de fone, ou o diz com outras palavras…» (`left: ""`) |
| c2. `desenhar` sem `desenharAvisoDeFone(snapshot)` | `tela-sessao.js` | «o desenho da sessão não passa mais pelo aviso de fone…» |
| c3. Sem a condição de `occupied_by_us` | `tela-sessao.js` | «o aviso de fone não espera a entrada numa sala de voz…» |
| c4. O «dispensar» zerando `foneJaAvisado` | `tela-sessao.js` | «a marca do aviso de fone é zerada em algum lugar além da declaração…» (`left: 2, right: 1`) |
| c5. Sem o ouvinte do «dispensar» | `tela-sessao.js` | «a faixa de fone não se fecha…» |
| c6. A ajuda sem o `dd` do fone | `index.html` | «a ajuda não tem mais o aviso de fone (`#ajuda-aviso-fone`)» |
| c7. «Use fone.» em caixa baixa na configuração | `index.html` | «CONFIGURAÇÕES · MICROFONE E SOM diz o aviso de fone com outras palavras…» |
| d1. `conectar` sem a pergunta | `tela-boot.js` | «a entrada não pergunta mais o nome a quem não tem nenhum…» |
| d2. A condição sem `!jaPergunteiONome` | `tela-boot.js` | «a pergunta do nome não é mais só da primeira entrada desta execução…» |
| d3. A condição sem `apelido === ""` | `tela-boot.js` | «a pergunta do nome não espera o apelido estar vazio…» |
| d4. `perguntarONome` sem `await fechou` | `camada-nomear.js` | «…não espera ele fechar, ou não relê o nome gravado depois…» |
| d5. `fecharPerfil` sem soltar quem espera | `camada-nomear.js` | «fechar o perfil não solta a entrada que espera por ele…» |
| d6. `ENTRAR_SEM_NOME` com outras palavras | `frases.js` | «`frases.js` não diz mais com que nome entra quem fecha a pergunta…» |
| d7. `perguntarONome` sem `ENTRAR_SEM_NOME` | `camada-nomear.js` | «quem fecha a pergunta do nome sem responder entra sem saber com que nome» |
| d8. `abrirPerfil` sem o ramo que volta a SEU PERFIL | `camada-nomear.js` | «o perfil aberto sem título deixou de se chamar SEU PERFIL…» |
| d9. `abrirPerfil()` sem o título da pergunta | `camada-nomear.js` | «a pergunta do nome não abre o perfil com «COMO OS OUTROS VÃO TE CHAMAR?»…» |

A d8 só reprovou depois de uma correção no próprio guarda. Ele procurava `"SEU PERFIL"` em `abrirPerfil`, e a comparação do anúncio também tem essa palavra: apagar o ramo deixaria o guarda verde. Agora ele exige a atribuição `$("perfil-titulo").textContent = "SEU PERFIL"`. A correção foi feita antes do commit da parte 3, e todas as reversões dela rodaram com o guarda final.

## O que se viu num navegador

A janela de verdade não rodou: nem WKWebView, nem WebView2. O que rodou foi o Chromium sem cabeça do Playwright (`PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright`), de dois jeitos:

- `bancada/telas.cjs sessao`, como está. A faixa de fone acende sobre a sessão numa sala, e a linha do operador diz «microfone abre enquanto segura ESPAÇO · ouvindo». A bancada anota dois erros que são dos simulados dela, e não deste lote: os MODs respondem «disconnected», e `controles_da_voz` responde nulo em `desenharQualidadeDaVoz`.
- Um roteiro meu, no scratchpad e **fora do commit**. Ele reaproveita `servir`, `respostas` e `retrato` de `telas.cjs` sem mexer neles. O que ele mediu:
  - **Tecla gravada `KeyF`.** O operador diz «microfone abre enquanto segura F · ouvindo». A faixa da configuração diz «Na sala Sala principal · microfone abre enquanto segura F». A ajuda mostra `F`, e a mesma frase de fone em AS PALAVRAS. MICROFONE E SOM mostra o campo FONE no topo de SAÍDA DE SOM.
  - **A faixa de fone.** Fora de sala, ela fica escondida. Ao entrar, acende com a frase. Depois de «dispensar», sair e entrar de novo, ela continua escondida.
  - **Sem nome, fechando.** O diálogo abre antes de qualquer `connect`, com o título e a nota. Ao fechar, o `connect` sai com `nickname: ""`, e a frase é anunciada. Uma segunda chamada de `conectar` não pergunta. O perfil aberto depois volta a SEU PERFIL, sem a nota.
  - **Sem nome, respondendo só com o teclado.** O foco cai no campo. Digitar «Bia» e apertar Enter grava com `escolher_apelido_local` («Bia»), e o `connect` sai com «Bia».
  - **Com nome na máquina.** Não há diálogo, e o `connect` sai com «Alex».
  - Nenhum erro de página em nenhum dos casos. Uma ressalva: na cena da tecla, `abrirServer` lança no meio, porque o simulado não tem lista de aparelhos. A tela já estava visível, e a faixa de contexto já tinha sido escrita. O roteiro segura essa exceção, que é da bancada e não do produto.

## Arquivos mudados

- `apps/seele-app/ui/base.js`
- `apps/seele-app/ui/tela-server.js`
- `apps/seele-app/ui/tela-sessao.js`
- `apps/seele-app/ui/camada-ajuda.js`
- `apps/seele-app/ui/camada-nomear.js`
- `apps/seele-app/ui/tela-boot.js`
- `apps/seele-app/ui/frases.js`
- `apps/seele-app/ui/index.html`
- `apps/seele-app/tests/frontend.rs`
- `apps/seele-app/ui/camada-nomear.css` (só na rodada 1, por decisão do controlador)
- `apps/seele-app/bancada/ajustes-v013.cjs` (só na rodada 2, por decisão do controlador; ela já roda no job `bancadas` do `ci.yml`)
- `docs/superpowers/sdd/2026-10-08-0.16/F5/lote-j-report.md` (este)

Não mudaram: as outras folhas, o Rust do apelido, `main.rs`, `tela-fim.js`, o fio, `api/v*.json`, `api/README.md` e os arquivos da F6. Nenhuma dependência nova.

## Para a v6

Nada. `AVISO_DE_FONE`, `ENTRAR_SEM_NOME`, `perguntarONome` e o parâmetro de `abrirPerfil` são da casca, e nada disso chega a um MOD.

## O que não rodou

- **As WebViews de verdade.** WKWebView no macOS e WebView2 no Windows, com o app nativo. A medida em tela é do dono, como o brief diz. O roteiro acima é Chromium com a ponte simulada: ele mostra que o JS carrega, desenha e segue a ordem certa, e não mostra como cada motor quebra a linha nem o foco do sistema.
- **O caminho real do `connect`** com apelido vazio até o `pessoa-…` derivado no Rust. Nada no Rust mudou, e o simulado só confirma que a casca manda vazio.
- **Linux e Windows.** Nada deste lote tem `cfg`. São JS, HTML e um teste de texto, que rodam iguais em qualquer sistema.

## Preocupações

1. **Onde a frase de quem fecha sem nome é dita.** O brief diz «fechar sem nome segue como hoje, mas dizendo». A tela de entrada some logo depois do fechamento: o `connect` leva à `#tela-auth` ou à sessão, e a `#boot-etapa` é reescrita pelas etapas da chegada. Uma frase escrita depois do fechamento quase não seria lida. Por isso ela aparece na caixa enquanto a pergunta está aberta, que é quando ainda dá para responder, e é anunciada ao leitor de tela ao fechar. Se o dono quiser a frase visível **depois** do fechamento, o lugar natural é a `#tela-auth`, que não é deste lote.
2. **O título da pergunta quebra em duas linhas apertadas.** Na caixa de 480px, «COMO OS OUTROS VÃO TE CHAMAR?» ocupa duas linhas, e a altura de linha de `.nomear-titulo` foi feita para títulos de uma linha. No Chromium as duas linhas quase se tocam. O conserto é de folha (`camada-nomear.css`), que não está na lista do lote. Fica para a F4, ou para o lote Z registrar. **Consertado na rodada 1** (`9ae26fb1`): o controlador pôs a folha no lote.
3. **A linha do operador agora quebra.** Na coluna de 268px, a 1280px de largura, «microfone abre enquanto segura ESPAÇO · ouvindo» vai para duas linhas. Antes, «microfone abre na tecla · ouvindo» cabia numa. Não corta nada. Com teclas de nome longo («SHIFT ESQ», «CTRL DIR»), ela continua em duas.
4. **A faixa de fone veste `.veredito`.** Sem folha na lista do lote, ela reusa a geometria e a cor do veredito. Uma mudança futura em `.veredito` muda as duas. O comentário no `index.html` diz isso. Se a F4 quiser classes próprias, é uma troca de nome nas duas pontas.
5. **Dois acréscimos que o brief não pedia por escrito:**
   - na pergunta do nome, o foco vai para o campo do apelido, e não para ESCOLHER, onde o perfil de sempre o põe;
   - o anúncio ao abrir o perfil diz o título que está escrito.

   Os dois servem à pergunta, e o SEU PERFIL de sempre continua igual.
6. **Onde a frase de fone mora na ajuda e na configuração foi escolha minha.** Na ajuda, é um verbete FONE em AS PALAVRAS. Na configuração, é o primeiro campo de SAÍDA DE SOM, e não o topo do painel. O brief só dizia «na ajuda» e «em MICROFONE E SOM».
7. **Um defeito antigo, não tocado.** Cada reabertura interna do perfil sobrescreve `focoAntesDoPerfil` com um botão de dentro da caixa. Depois de trocar a imagem, fechar não devolve o foco a quem abriu. Isso já acontecia antes, e não muda com este lote.
8. **UX, como o brief avisa.** Quem entra por link sem nome ganha um passo, uma vez por execução. Quem fecha sem responder não é perguntado de novo até reabrir o app.
9. **A F4 mexe depois em sete destes arquivos**, e o lote I divide `frases.js`, `tela-sessao.js` e `frontend.rs` com este. Os guardas novos leem por função (`js_function`) e por id. Uma mudança de forma (renomear `desenharAvisoDeFone`, `perguntarONome` ou `jaPergunteiONome`) os reprova com frase, e não em silêncio.

## Rodada 1 de conserto

A revisão deu «precisa de conserto» por três Importantes medidos e cinco menores. Por decisão do controlador, `apps/seele-app/ui/camada-nomear.css` entrou no lote. Mesmo worktree e mesmo branch.

| Conserto | Commit |
|---|---|
| Importante 1: a faixa de contexto da configuração dizia a tecla antiga | `4daaa4c3` |
| Importante 2: as duas linhas do título da pergunta se sobrepunham | `9ae26fb1` |
| Importante 3: a abertura por link dependia da ordem de carga dos scripts | `80255f64` |
| Menores 4 e 6: a marca do fone sem guarda, e a faixa atravessando sessões | `08ddec61` |
| Menores 5, 7 e 8: as notas do campo, CONECTAR ativo durante a pergunta, e o guarda (d) estourando sem frase | `39861b14` |

Depois de cada commit: `cargo test -p seele-app --test frontend` deu 267 verdes, `cargo xtask check-runtime` saiu 0 com as 7 bancadas, e `cargo fmt --all -- --check` e `cargo clippy -p seele-app --all-targets --all-features -- -D warnings` ficaram limpos. O único aviso foi o `ld: duplicate -rpath`, que é do ambiente.

**A bancada de navegador** é o roteiro do scratchpad descrito acima (`olhar.cjs`), com cenas novas. Ele roda com `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright`, reaproveita `servir`, `respostas` e `retrato` de `bancada/telas.cjs`, e a ponte dele resolve em microtarefa, como a de `telas.cjs`. Ele **não foi commitado**: um `.cjs` novo em `bancada/` tem de entrar no `run:` do job `bancadas` do `ci.yml`, ou o teste `toda_bancada_da_pasta_roda_em_algum_lugar` reprova, e o `ci.yml` não é deste lote. As medidas estão abaixo, com os números.

### Importante 1: a faixa de contexto acompanha a troca da tecla (`4daaa4c3`)

**Medido na bancada, antes do conserto.** Com a tecla ESPAÇO e a configuração aberta, a cena troca a tecla gravada para `KeyF`, chama `pararDeOuvirTecla()` e espera 1,5 s:

```
"contexto": "Na sala Sala principal · microfone abre enquanto segura ESPAÇO",
"botao": "F",
"operador": "microfone abre enquanto segura F · ouvindo"
```

**O conserto.** Em `pararDeOuvirTecla` e em `abrirServer`, `recarregarTeclaDeFalar().then((tecla) => { desenharTeclaDeFalar(tecla); desenharContextoDaSessao(); })`. O comentário de `desenharContextoDaSessao` dizia que a frase e o ouvinte «não têm como discordar». Agora diz o que as mantém juntas: as duas releituras redesenham a faixa quando a leitura volta. Depois do conserto, a mesma cena dá `"contexto": "… microfone abre enquanto segura F"`.

**O guarda (a)** cobra que, nas duas funções, o argumento do `.then` de `recarregarTeclaDeFalar()` chame `desenharContextoDaSessao()`. O recorte é por parênteses casados. `abrirServer` também chama a faixa de forma síncrona logo depois, e essa chamada, que lê a tecla de antes, não pode contar.

RED, antes do conserto:

```
`function pararDeOuvirTecla(` relê a tecla de falar e não redesenha a faixa de contexto quando a leitura volta: a faixa continua dizendo a tecla de antes ao lado do botão que já diz a nova:
```

| Reversão | O guarda reprovou com |
|---|---|
| r1a. `pararDeOuvirTecla` de volta a `.then(desenharTeclaDeFalar)` | «`function pararDeOuvirTecla(` relê a tecla de falar e não redesenha a faixa de contexto…» |
| r1b. `abrirServer` de volta a `.then(desenharTeclaDeFalar)`, com a chamada síncrona de pé | «`async function abrirServer(` relê a tecla de falar e não redesenha a faixa de contexto…» |

A r1b é a prova de que a chamada síncrona não satisfaz o guarda. Na primeira tentativa, a expressão da r1a casou com `abrirServer`, que tem a mesma linha depois. Ela foi refeita ancorada no `}` que fecha `pararDeOuvirTecla`, e as duas rodaram de novo.

### Importante 2: o título da pergunta (`9ae26fb1`)

**O conserto**, no seletor que já existia: `.perfil-cabeca .nomear-titulo { flex: 1; line-height: 1.15; }`, com um comentário dizendo por quê.

**A medida.** Para cada linha do título, a cena junta as letras pelo topo do retângulo de cada uma. A área de conteúdo sai de `Range.getClientRects()`. A tinta sai da linha de base (topo da área mais `fontBoundingBoxAscent`) e de `measureText` do texto daquela linha (`actualBoundingBoxAscent` e `actualBoundingBoxDescent`), com a fonte de verdade (`900 24px "Saira Condensed"`). O vermelho é a tinta de uma linha começar antes de a da linha de cima acabar.

| Folha | `line-height` | Áreas de conteúdo (linha 1 · linha 2) | Tinta (linha 1 · linha 2) | Folga entre as tintas | Cabeçalho de SEU PERFIL |
|---|---|---|---|---|---|
| a de hoje (reversão) | 16px, de `base.css` | 240,3–278,3 · 256,3–294,3 (22px sobrepostos) | 246,1–267,5 · 266,6–283,5 | **−0,9px**: vermelho, «AS LINHAS DO TÍTULO SE SOBREPÕEM» | 65px |
| a nova | 27,6px | 233,7–271,7 · 261,3–299,3 | 239,6–260,9 · 271,6–288,5 | **10,7px**: verde | 65px |

- **A primeira medida que tentei** comparava as áreas de conteúdo, e ficou vermelha também com a folha nova. Esta face tem 38px de área para 24px de letra, então as áreas se sobrepõem com qualquer altura de linha abaixo de 1,58. O defeito visível é a tinta se tocar, e a tabela registra as duas medidas.
- O cabeçalho da pergunta passa de 65 para 88px, porque ela ocupa duas linhas. O de SEU PERFIL fica em 65px.
- A reversão voltou a folha guardada, e o `cmp` saiu igual.
- `toda_classe_que_o_script_aplica_tem_regra_de_css` e os demais guardas de folha seguiram verdes.

### Importante 3: a abertura espera a página inteira (`80255f64`)

**Medido na bancada, antes do conserto.** A cena `link` tem `abertura: { entrar: "seele://192.0.2.1:8384" }`, `analisar_convite` respondendo o alvo, `apelido_local: ""`, e a ponte resolvendo em microtarefa:

```
"perguntaAberta": false,
"bootErro": "ALGO FALHOU E ESTE APP NÃO SABE EXPLICAR O QUÊ.\nSe for relatar, copie o texto abaixo:\nReferenceError: perguntarONome is not defined",
"connect": 0
```

**O conserto.** `cumprirAAbertura()` passou a ser chamada dentro de `document.addEventListener("DOMContentLoaded", …)`, que só dispara depois de todos os scripts da página. `desenharVersoes()` continua no topo, e as versões continuam saindo primeiro, como o comentário pedia.

Conferi o que dependia de a abertura rodar antes:
- nenhum script lê o que ela produz;
- nenhuma bancada simula um pedido de abertura: `telas.cjs` responde `abertura: null`;
- `contribuicoes-e-camadas.cjs`, que monta o `index.html` de verdade, passou no `check-runtime`.

Depois do conserto, a cena dá `"perguntaAberta": true`, `"titulo": "COMO OS OUTROS VÃO TE CHAMAR?"`, sem erro na entrada e sem `connect` antes da resposta.

**O guarda (d)** ganhou duas asserções: `cumprirAAbertura` é chamada uma vez só, e essa chamada está no corpo do ouvinte de `DOMContentLoaded`. O primeiro RED vinha do pânico do `js_function`, em inglês e sem a frase do guarda. O guarda passou a conferir a presença antes, e o RED ficou com a frase dele:

```
o que o launcher pede à janela roda antes de os outros scripts carregarem: aberta por um link sem nome nesta máquina, a entrada chega à pergunta antes de `camada-nomear.js` existir, e a pergunta nunca abre
```

**Reversão r3**, com a chamada de volta ao topo:
- o guarda reprova com a frase acima, já na forma final que o clippy pediu (um `if` no lugar de `.then().unwrap_or_default()`);
- a bancada dá de novo `ReferenceError: perguntarONome is not defined` e `"perguntaAberta": false`.

O arquivo voltou da cópia guardada, e o `cmp` saiu igual.

### Menor 4: as duas linhas da marca do fone (`08ddec61`)

O guarda (c) passou a exigir `if (foneJaAvisado) return;` e `foneJaAvisado = true;` no corpo de `desenharAvisoDeFone`. As linhas já existiam, então a prova é a reversão:

| Reversão | O guarda reprovou com |
|---|---|
| m4a. Sem `if (foneJaAvisado) return;` | «o aviso de fone não confere se já avisou: a faixa volta a acender a cada quadro dentro da sala…» |
| m4b. Sem `foneJaAvisado = true;` | «o aviso de fone não marca que avisou: a faixa volta a acender a cada quadro dentro da sala…» |

### Menor 6: a faixa não atravessa sessões (`08ddec61`)

- **`ejetar`** apaga a faixa logo depois de `mostrarVeredito(null)`, como o veredito.
- **A outra saída**, a do fim de sessão, mora em `tela-fim.js`, que não é deste lote. Para ela, a faixa guarda a geração da sessão em que acendeu (`geracaoDoAvisoDeFone`), e `desenharAvisoDeFone` a apaga no primeiro quadro de uma geração nova (`geracaoDaSessao`, de `base.js`). A marca por execução não muda: a faixa não volta a acender na sala seguinte.
- **RED do guarda**, antes do conserto: «sair do servidor não apaga a faixa de fone, e ela reaparece sobre a sessão seguinte».
- **Na bancada** (cena `sessoes`): acesa na geração 7, apagada no primeiro quadro da geração 8, fora da sala. Na mesma geração, fora da sala, continua acesa até «dispensar». Depois de `ejetar()`, apagada.

| Reversão | O guarda reprovou com |
|---|---|
| m6a. `ejetar` sem apagar a faixa | «sair do servidor não apaga a faixa de fone, e ela reaparece sobre a sessão seguinte» |
| m6b. Sem a comparação de geração | «a faixa de fone não sabe de que sessão é: uma sessão encerrada pelo servidor a deixa acesa para a próxima» |

### Menor 5: as notas do campo do apelido (`39861b14`)

- **O campo** ganhou `aria-describedby="perfil-sem-nome"`. O leitor de tela diz a nota quando o foco chega ao campo, e não só ao fechar.
- **`desenharNotasDoApelido()`**, nova, desenha as duas notas:
  - a de quem fecha sem nome só aparece na pergunta e com o campo vazio;
  - escondida, ela fica **vazia**, porque a descrição lê até o texto de um nó escondido;
  - a nota de sempre («O que você já disse continua com o nome de então.», agora `#perfil-nota-do-apelido`) sai durante a pergunta.
- **Quem chama a função**: o `input` do campo, `abrirPerfil` (antes de a caixa aparecer e de o foco chegar) e `fecharPerfil`. A marca `perguntandoONome` é posta por `perguntarONome` e tirada por `fecharPerfil` e pela abertura do SEU PERFIL.
- **Na bancada**, antes do conserto: a nota continuava embaixo de «Bi» digitado, e o campo não tinha descrição. Depois:
  - aberta, a nota é visível, a nota de sempre está escondida, e `aria-describedby` vale `perfil-sem-nome`;
  - com «Bi» digitado, a nota some, e volta com o campo apagado;
  - no SEU PERFIL depois da pergunta, a nota está escondida e vazia, e a nota de sempre está visível.

### Menor 7: CONECTAR apagado durante a pergunta (`39861b14`)

- **Em `conectar`**: `$("botao-conectar").disabled = true` antes de `await perguntarONome()`, e `disabled = false` num `finally`. Logo abaixo, sem `await` no meio, a entrada o apaga de novo até o `connect` responder.
- **RED do guarda**, antes do conserto: «CONECTAR fica ativo enquanto a pergunta do nome está aberta, e uma segunda entrada por baixo dela entra com o nome vazio».
- **Na bancada**, antes: `"conectarApagado": false`. Depois: `true` com a pergunta aberta, e `false` quando a entrada termina, nas duas saídas (fechar e salvar).

### Menor 8: o guarda (d) diz a frase dele

`assert!(local < pergunta, …)` antes de `conectar[local..pergunta]`. A reversão n8 renomeia a leitura de `apelido_local` em `conectar` e põe outra depois da pergunta. Antes, a fatia estourava com o pânico de índice do Rust. Agora o guarda reprova com «a entrada pergunta o nome antes de ler o desta máquina, e para na porta quem já tem um».

| Reversão | O guarda reprovou com |
|---|---|
| n7a. Sem apagar CONECTAR | «CONECTAR fica ativo enquanto a pergunta do nome está aberta…» |
| n7b. Sem o `finally` que o reacende | «CONECTAR não volta a valer depois da pergunta, inclusive quando ela falha» |
| n5a. O campo sem `aria-describedby` | «o campo do apelido não é descrito pela nota de quem fecha sem nome…» |
| n5b. Sem o ouvinte de `input` | «a nota de quem fecha sem nome não acompanha o campo…» |
| n5c. A nota sem a condição do campo vazio | «as notas do campo do apelido erram: a de quem fecha sem nome aparece fora da pergunta, ou embaixo de um nome já digitado» |
| n5d. A nota com o texto mesmo escondida | «as notas do campo do apelido erram: a de quem fecha sem nome guarda o texto quando escondida…» |
| n5e. A nota de sempre visível na pergunta | «as notas do campo do apelido erram: a de quem troca de nome aparece na pergunta, para quem nunca disse nada» |
| n8. A leitura de `apelido_local` depois da pergunta | «a entrada pergunta o nome antes de ler o desta máquina…» |

As mensagens de n5c a n5e foram reescritas depois da primeira rodada de reversões, porque «não só aparece» se lia como «não apenas aparece». As três rodaram de novo com o texto final.

Antes e depois de cada bateria de reversões, o `shasum` de todos os `.js` e `.css` de `ui/`, do `index.html` e do `frontend.rs` foi conferido com `cmp`. Saiu igual em todas. Foram 16 reversões nesta rodada.

### Notas, como o controlador pediu

- **A pergunta abre por cima do diálogo da porta, ao hospedar**, como a revisão anotou. É o caminho de HOSPEDAR AQUI: `hospedar()` chama `conectar()`, e a pergunta abre sem que o diálogo de baixo feche. Não mexi nisso nem medi nesta rodada.
- **A linha do operador ocupa duas linhas** na coluna de 268px: «microfone abre enquanto segura ESPAÇO · ouvindo» (preocupação 3).
- **A faixa de fone usa as classes do veredito** (preocupação 4). Só `camada-nomear.css` entrou no lote.

## Rodada 2

A re-revisão reproduziu no navegador os oito consertos da rodada 1. Sobraram um Importante e um Menor, os dois sobre prova versionada, e um defeito antigo da mesma classe, medido igual no base e no head. Por decisão do controlador, `apps/seele-app/bancada/ajustes-v013.cjs` entrou no lote. Ela já roda no job `bancadas` do `ci.yml`, e por isso o `ci.yml` não mudou.

Tudo está num commit só, `72b8c7cb`. Os três itens mexem na mesma bancada e no mesmo `frontend.rs`, e separar os pedaços pediria `git add -p`, que é interativo.

| Item | O que mudou | Commit |
|---|---|---|
| 1 (Importante). O `line-height` do título sem guarda | guarda novo `o_titulo_do_perfil_tem_altura_de_linha_propria`, e a medida das tintas na `ajustes-v013.cjs` | `72b8c7cb` |
| 2 (Menor). As provas dos Importantes 1 e 3 só no scratchpad | a tecla e a entrada por link viraram casos da `ajustes-v013.cjs` | `72b8c7cb` |
| 3. A troca de modo deixava a faixa de contexto no modo antigo | `atualizarServer` redesenha a faixa, e ele é o único caminho; os dois `.then` da rodada 1 saíram | `72b8c7cb` |

### O caminho único da faixa de contexto (item 3)

**O defeito**, medido pela re-revisão e reproduzido pela bancada antes do conserto:

```
AssertionError [ERR_ASSERTION]: trocado o modo do microfone para VOZ, a faixa de contexto continuou dizendo o modo de antes — #server-contexto diz «Na sala Sala principal · microfone abre enquanto segura F»
```

**O conserto.** `desenharContextoDaSessao()` passou para dentro de `atualizarServer`, que roda ao abrir a configuração e a cada 500 ms com a tela na frente. Ele cobre os três casos de uma vez:
- a troca de tecla: `recarregarTeclaDeFalar` atualiza `teclaDeFalar`, e o próximo ciclo redesenha;
- a troca de modo: a sessão redesenha o quadro, e o ciclo seguinte lê o modo novo;
- mudo e sala, que antes também só eram ditos ao abrir.

Com isso, os dois redesenhos que a rodada 1 pôs no `.then` de `pararDeOuvirTecla` e de `abrirServer` ficaram desnecessários e saíram. Saiu também a chamada síncrona de `abrirServer`, porque o `atualizarServer` que ele aguarda logo depois já desenha. Se a lista de aparelhos falhar antes dele, o ciclo de 500 ms desenha a faixa do mesmo jeito. Sobra um caminho só, e o comentário de `desenharContextoDaSessao` diz qual é.

**O guarda (a)** foi ajustado a esse caminho:
- `atualizarServer` chama `desenharContextoDaSessao()`;
- em `tela-server.js` sem comentários, há exatamente uma chamada fora da declaração. Um segundo caminho é o que deixou a troca de modo de fora na rodada 1.

RED, antes do conserto:

```
`atualizarServer` não redesenha a faixa de contexto: o que muda com a configuração aberta — a tecla de falar, o modo do microfone — fica dito como era ao abrir:
```

### O que a `ajustes-v013.cjs` passou a medir

Os casos novos ficam na mesma corrida dos de antes. A bancada continua saindo 0 só se todos passam, e a mensagem final ganhou uma linha:

```
v0.13: contexto, resolução, navegação, página imersiva/Escape e avisos verificados em Chromium
lote J: a faixa de contexto acompanha a tecla e o modo, a entrada por link pergunta o nome antes do connect, e o título da pergunta não se sobrepõe (folga de 10.7px entre as tintas)
```

- **A tecla e o modo**, na página de sempre, com a configuração aberta sobre a sessão. Antes de tudo, a faixa tem de dizer «segura ESPAÇO». A tabela da ponte troca `tecla_de_falar` para `KeyF`, e `pararDeOuvirTecla()` roda; a faixa tem de dizer «segura F». O retrato passa a `voice_mode: VoiceActivated`, e `escolherModo` roda; a faixa tem de dizer «microfone abre por voz». A espera é de até 3 s, e a falha diz o que a faixa diz.
- **A entrada por link**, numa segunda página. A tabela tem `abertura: { entrar: "seele://192.0.2.1:8384" }`, `analisar_convite`, `apelido_local: ""`, e a ponte resolve em microtarefa. Ela também guarda o que `escolher_apelido_local` grava, porque a pergunta relê o nome depois de fechar. As asserções:
  - a pergunta abre, e a falha diz o que está escrito na entrada;
  - o título é «COMO OS OUTROS VÃO TE CHAMAR?»;
  - nenhum `connect` sai antes da resposta;
  - respondendo «Bia» pelo teclado (o foco está no campo, e Enter salva), a ordem é `escolher_apelido_local` e depois `connect`, e o `connect` leva «Bia».
- **A folga entre as linhas do título**, na mesma pergunta, pela tinta, como no scratchpad. A função `tintasDoTitulo` está na bancada, com o comentário de por que a área de conteúdo não serve. O título tem de quebrar em duas linhas (senão a medida não mede nada), e a folga tem de ser positiva.

### O guarda de texto do `line-height` (item 1)

`o_titulo_do_perfil_tem_altura_de_linha_propria` lê a regra `.perfil-cabeca .nomear-titulo` de `camada-nomear.css`, sem comentários, e exige `line-height` nela. É a metade que roda em toda máquina; a medida de verdade é a da bancada. As duas são provadas pela mesma reversão, abaixo.

### As reversões da rodada 2

Os guardas de texto, com `cargo test -p seele-app --test frontend -- --exact`:

| Reversão | O guarda reprovou com |
|---|---|
| t1. `line-height: 1.15` tirado da regra | «o título do perfil voltou a herdar a altura de linha de `base.css` (16px para letra de 24px)…» |
| t2. `atualizarServer` sem a chamada | «`atualizarServer` não redesenha a faixa de contexto…» |
| t3. Os dois `.then` da rodada 1 de volta, com a chamada de `atualizarServer` de pé | «a faixa de contexto é redesenhada em 3 lugares de `tela-server.js`; ela tem um caminho só…» |

A bancada, com `ajustes-v013.cjs`. Todas saíram 1, e o arquivo voltou com `cmp` igual:

| Reversão | A bancada reprovou com |
|---|---|
| b1. `line-height: 1.15` tirado da regra | «as duas linhas do título da pergunta se sobrepõem: folga de -0.9px entre as tintas, com line-height 16px» |
| b2. O estado de antes da rodada 1: sem a chamada em `atualizarServer`, com a chamada síncrona de `abrirServer`, sem redesenho no `.then` | «trocada a tecla de falar para F, a faixa de contexto continuou dizendo a tecla de antes — #server-contexto diz «… segura ESPAÇO»» |
| b3. O estado da rodada 1: o mesmo de b2, mais os dois `.then` com o redesenho | a tecla passa, e reprova o modo: «trocado o modo do microfone para VOZ, a faixa de contexto continuou dizendo o modo de antes — #server-contexto diz «… segura F»» |
| b4. `cumprirAAbertura()` de volta ao topo de `tela-boot.js`, fora do `DOMContentLoaded` | «aberta por um link sem nome nesta máquina, a janela não perguntou o nome — a entrada diz «ALGO FALHOU E ESTE APP NÃO SABE EXPLICAR O QUÊ. … ReferenceError: perguntarONome is not defined»» |

A b2 é a reversão que o controlador pediu para a tecla, «o `.then` sem o redesenho», no caminho que sobrou: sem `atualizarServer` desenhando, a tecla volta a ficar velha. A b3 mostra que o conserto da rodada 1 não bastava para o modo, que é o defeito antigo.

A primeira corrida de b4 guardou só duas linhas da saída e cortou o `ReferenceError`. Ela foi rodada de novo guardando quatro linhas, e o texto acima é o dessa corrida.

O `cargo fmt` reflowou uma linha de `frontend.rs` depois das reversões, e t2 e t3 rodaram de novo sobre o arquivo formatado, com as mesmas frases. Antes e depois da bateria, o `shasum` dos `.js` e `.css` de `ui/`, do `index.html`, do `frontend.rs` e da `ajustes-v013.cjs` foi conferido com `cmp`, e saiu igual. Foram 7 reversões nesta rodada.

### O que rodou

- `cargo test -p seele-app --test frontend`: 268 verdes. Os 267 de antes e o guarda novo do `line-height`.
- `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright node apps/seele-app/bancada/ajustes-v013.cjs`: saiu 0, com as duas linhas acima.
- `cargo xtask check-runtime`: saiu 0, com as 7 bancadas.
- `cargo fmt --all -- --check` e `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`: limpos.

### O que muda no que a rodada 1 disse

- **O Importante 1 da rodada 1** (`4daaa4c3`) foi substituído por este caminho único. Os `.then` que ele pôs saíram, e as reversões r1a e r1b daquela rodada passaram a valer como t3 e b2 desta.
- **A bancada do scratchpad** (`olhar.cjs`) continua fora do commit. As provas dos Importantes 1, 2 e 3 da rodada 1 e da troca de modo estão agora na `ajustes-v013.cjs`. As cenas das notas do campo, do CONECTAR apagado e da faixa de fone entre sessões continuam só no scratchpad, com os guardas de texto de `frontend.rs` como a parte versionada.

## Rodada 3

A rodada segue o brief `lote-j-rodada3.md` (FIX_BASE `da16d3a`): quatro itens, e nada além deles. Tudo está num commit só, `2e64af2c`, porque os quatro dividem o `frontend.rs` e a `ajustes-v013.cjs`. Ficaram de fora, como o brief manda, a observação sobre o caso do modo e a faixa regravada a cada 500 ms sem mudança.

| Item | Commit |
|---|---|
| 1. A faixa velha por uma ida à ponte ao abrir (regressão da rodada 2) | `2e64af2c` |
| 2. Duas asserções sem frase na `ajustes-v013.cjs` | `2e64af2c` |
| 3. A brecha do guarda (a) | `2e64af2c` |
| 4. A quebra do título dependia da largura da fonte | `2e64af2c` |

O que rodou depois do conserto:
- `cargo test -p seele-app --test frontend`: 268 verdes;
- a `ajustes-v013.cjs`, com `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright`: saiu 0;
- `cargo xtask check-runtime`: saiu 0, com as 7 bancadas;
- `cargo fmt --all -- --check` e `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`: limpos.

### 1. A faixa certa no primeiro quadro

**O vermelho medido.** Caso novo na `ajustes-v013.cjs`. A ponte da página principal ganhou atraso por comando (`testAtraso`). A cena:
1. fecha a configuração, põe o microfone mudo no retrato e o desenha;
2. atrasa `microfones` em 400 ms e `snapshot` em 150 ms;
3. reabre e lê `#server-contexto` no `requestAnimationFrame` seguinte, que é o primeiro quadro.

No head da rodada 2:

```
AssertionError [ERR_ASSERTION]: reaberta com o microfone já mudo, a configuração mostrou no primeiro quadro a faixa da abertura anterior: «Na sala Sala principal · microfone abre enquanto segura ESPAÇO»
```

**O conserto, e por que este.** A faixa não depende do que `atualizarServer` busca: ela lê o quadro da sessão (`desenhado`) e a tecla. Então:
- `atualizarServer` escreve a faixa **antes do primeiro `await` dele**, e continua sendo o único lugar que a escreve;
- `abrirServer` chama `atualizarServer()` **sem `await`, antes do primeiro `await` seu**. A faixa é escrita na mesma tarefa do `hidden = false` que mostra a tela;
- o `await atualizarServer()` de depois da lista de aparelhos continua.

As duas alternativas do brief ficaram de fora:
- **Rodar `atualizarServer` antes da lista e esperá-lo** deixaria o primeiro quadro certo só com uma ponte instantânea, porque a escrita ainda vinha depois da ida ao `snapshot`. E ele apagaria o EM USO da lista por até 500 ms: a lista nasce marcada sem retrato (`marcarLinhas(null)`), e só o `atualizarServer` de depois dela põe o EM USO.
- **Esconder a faixa no `fecharServerMesmo`** seria um segundo escritor, e a tela abriria sem faixa em vez de com a certa.

O caso de bancada atrasa o `snapshot` para separar as duas coisas. Com a escrita depois da ponte (reversão q2), ele reprova mesmo com o `atualizarServer` chamado cedo.

**O guarda (a)** ganhou a metade de texto: em `atualizarServer`, `desenharContextoDaSessao()` vem antes do primeiro `await`; em `abrirServer`, `atualizarServer()` vem antes do primeiro `await`. Antes do conserto ele reprovava assim:

```
`atualizarServer` escreve a faixa de contexto depois de esperar a ponte, e a configuração aparece com a faixa da abertura anterior até a resposta chegar:
```

| Reversão | Reprovou com |
|---|---|
| q1. Sem a chamada antecipada em `abrirServer` (bancada) | «reaberta com o microfone já mudo, a configuração mostrou no primeiro quadro a faixa da abertura anterior: «… segura ESPAÇO»» |
| q1g. A mesma, no guarda | «`abrirServer` mostra a configuração e espera alguma coisa antes de `atualizarServer` escrever a faixa de contexto…» |
| q2. A escrita de volta para depois da ponte, em `atualizarServer`, com a chamada antecipada de pé (bancada) | a mesma frase de q1 |
| q2g. A mesma, no guarda | «`atualizarServer` escreve a faixa de contexto depois de esperar a ponte…» |

### 2. As duas asserções com frase

**O vermelho medido** é o que a re-revisão apontou: o título «COMO OS OUTROS VÃO TE CHAMAR?» e `assert.deepEqual(errosDoLink, [])` estavam sem mensagem. **O conserto** deu a cada uma a frase do que quebra. Para mostrar a diferença, cada uma foi feita reprovar duas vezes: uma com a frase e outra com a frase revertida.
- e1/e2: o produto abre a pergunta com outro título («QUAL É O SEU NOME?»).
- e3/e4: o produto lança um erro na página da entrada por link.

| Reversão | Reprovou com |
|---|---|
| e1. Título errado, com a frase | «a caixa que abriu pela entrada por link não é a pergunta do nome: quem chega sem nome lê outro título…» |
| e2. Título errado, frase revertida | «Expected values to be strictly equal» (a frase do Node, que não diz o que quebra) |
| e3. Erro na página, com a frase | «a entrada por link lançou erro na página: o que está nesta lista quebrou sem ninguém ver…» |
| e4. Erro na página, frase revertida | «Expected values to be strictly deep-equal» |

### 3. O guarda (a) conta toda referência, em todo `.js` de `ui/`

**O vermelho medido** é a brecha. Antes do conserto, as duas mudanças do brief passaram verdes pelo guarda:

| Mudança | Guarda de antes |
|---|---|
| m3a. `.then(desenharTeclaDeFalar).then(desenharContextoDaSessao)` em `abrirServer` | `test result: ok. 1 passed` |
| m3b. Uma chamada em `desenhar` (`tela-sessao.js`) | `test result: ok. 1 passed` |

**O conserto.** O guarda percorre `ui_files(".js")` sem comentários. Ele conta `desenharContextoDaSessao` como palavra inteira, chamado ou passado, e desconta a declaração. A lista dos arquivos que sobram tem de ser exatamente `["tela-server.js"]`.

| Reversão | O guarda novo reprovou com |
|---|---|
| m3a | «a faixa de contexto é alcançada em 2 lugares de `ui/` (["tela-server.js", "tela-server.js"]); ela tem um caminho só…» |
| m3b | «… em 3 lugares de `ui/` (["tela-server.js", "tela-sessao.js", "tela-sessao.js"])…». São duas referências: o `typeof` e a chamada. |

### 4. A quebra do título em qualquer plataforma

**O risco**, medido em vez de suposto. Com o título mais estreito, o que se espera dos avanços de glifo do Linux, a cena de antes não quebra. A emulação foi `letter-spacing: 0` no título, cerca de 55px a menos numa linha de 364px. Sem a quebra forçada (f2):

```
AssertionError [ERR_ASSERTION]: o título da pergunta deixou de quebrar em duas linhas, e esta medida não mede mais nada: {"tintas":[{"texto":"COMO OS OUTROS VÃO TE CHAMAR?", …}],"folga":null,"lineHeight":"27.6px"}
```

**O conserto.** Só nesta cena, o título ganha largura máxima de 70% da própria largura numa linha, medida ali mesmo com `white-space: nowrap` e um `Range`. A conta não depende da fonte:
- 70% é menos que a linha inteira, então o título quebra;
- a primeira linha ocupa mais de 30% da largura, então o resto cabe em 70% e ele quebra uma vez só.

A asserção de duas linhas ficou, e a folha não mudou.

| Reversão | Saída |
|---|---|
| f1. Título estreitado, com a quebra forçada | verde: duas linhas, folga de 10,7px |
| f2. Título estreitado, sem a quebra forçada | o vermelho acima, «deixou de quebrar em duas linhas» |
| f3. `line-height: 1.15` tirado da folha, com a quebra forçada | «as duas linhas do título da pergunta se sobrepõem: folga de -0.9px entre as tintas, com line-height 16px» |

A f3 mostra que a reversão do `line-height` continua vermelha com a quebra forçada.

### As reversões, no conjunto

- **15 corridas**: 13 reversões (q1, q1g, q2, q2g, m3a, m3b, e1 a e4, f1 a f3) e as duas mudanças da brecha, medidas com o guarda de antes. As de bancada rodaram com um roteiro de scratchpad que muda um ou dois arquivos, roda a `ajustes-v013.cjs` e volta cada um da cópia guardada, com `cmp`.
- **Antes e depois da bateria**, o `shasum` dos `.js` e `.css` de `ui/`, do `index.html`, do `frontend.rs` e da `ajustes-v013.cjs` saiu igual.
- **Depois do `cargo fmt`**, que reflowou um fechamento em `frontend.rs`, q1g, q2g, m3a e m3b rodaram de novo sobre o arquivo formatado, com as mesmas frases.
