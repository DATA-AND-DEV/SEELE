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
- **As duas linhas de estado.** Em `desenharOperador(snapshot)` (`tela-sessao.js`) e em `desenharContextoDaSessao()` (`tela-server.js`), a entrada `PushToTalk` virou `` `microfone abre enquanto segura ${nomeDaTecla(teclaDeFalar)}` ``. `teclaDeFalar` é a mesma variável que o `keydown` compara, então a frase e o ouvinte não têm como discordar. As linhas são redesenhadas a cada quadro, e uma troca de tecla em ATALHOS aparece em meio segundo.
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
- `docs/superpowers/sdd/2026-10-08-0.16/F5/lote-j-report.md` (este)

Não mudaram: as folhas, o Rust do apelido, `main.rs`, o fio, `api/v*.json`, `api/README.md` e os arquivos da F6. Nenhuma dependência nova.

## Para a v6

Nada. `AVISO_DE_FONE`, `ENTRAR_SEM_NOME`, `perguntarONome` e o parâmetro de `abrirPerfil` são da casca, e nada disso chega a um MOD.

## O que não rodou

- **As WebViews de verdade.** WKWebView no macOS e WebView2 no Windows, com o app nativo. A medida em tela é do dono, como o brief diz. O roteiro acima é Chromium com a ponte simulada: ele mostra que o JS carrega, desenha e segue a ordem certa, e não mostra como cada motor quebra a linha nem o foco do sistema.
- **O caminho real do `connect`** com apelido vazio até o `pessoa-…` derivado no Rust. Nada no Rust mudou, e o simulado só confirma que a casca manda vazio.
- **Linux e Windows.** Nada deste lote tem `cfg`. São JS, HTML e um teste de texto, que rodam iguais em qualquer sistema.

## Preocupações

1. **Onde a frase de quem fecha sem nome é dita.** O brief diz «fechar sem nome segue como hoje, mas dizendo». A tela de entrada some logo depois do fechamento: o `connect` leva à `#tela-auth` ou à sessão, e a `#boot-etapa` é reescrita pelas etapas da chegada. Uma frase escrita depois do fechamento quase não seria lida. Por isso ela aparece na caixa enquanto a pergunta está aberta, que é quando ainda dá para responder, e é anunciada ao leitor de tela ao fechar. Se o dono quiser a frase visível **depois** do fechamento, o lugar natural é a `#tela-auth`, que não é deste lote.
2. **O título da pergunta quebra em duas linhas apertadas.** Na caixa de 480px, «COMO OS OUTROS VÃO TE CHAMAR?» ocupa duas linhas, e a altura de linha de `.nomear-titulo` foi feita para títulos de uma linha. No Chromium as duas linhas quase se tocam. O conserto é de folha (`camada-nomear.css`), que não está na lista do lote. Fica para a F4, ou para o lote Z registrar.
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
