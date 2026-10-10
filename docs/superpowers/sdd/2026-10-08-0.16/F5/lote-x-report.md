# Lote X da F5: relatório

**Base:** `cd1851ca` (`0.16/lotes`, com os lotes V e W). `versao/0.16` entrou
por merge em `55088be7`.
**Branch:** `0.16/lotes-x`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a7ae3c63cc1556c97`.
Tudo local: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `55088be7` | preparo | merge de `versao/0.16` (`94927da1`, `6483a5a1`). O único conflito foi em `docs/pendencias.md`, e ficaram a 58 do Z2 inteira, uma linha em branco e a 58b da F7 inteira, sem nenhuma palavra mudada. `git diff cd1851ca HEAD -- docs/pendencias.md` mostra só a 58b acrescentada, idêntica à de `versao/0.16` (conferido com `cmp`). |
| `0515047c` | X (o lote) | `instalar_mod_do_catalogo` ganha `uso: UsoDoCatalogo` (`"atualizar"` ou `"obter"`), obrigatório e sem padrão, e só `"atualizar"` reaplica (`reaplica_no_hospedado`). As três chamadas dizem o uso. Os guardas novos estão no `main.rs` e no `frontend.rs`, e a cena 5 da `ajustes-v013.cjs` ganha uma asserção. |

**O que não mudou:**
- O fio, o servidor, `crates/`, `api/`, a CSP e `ui/camada-versoes.js`.
- Nenhuma dependência nova, e nada foi baixado.
- `docs/pendencias.md` só mudou pelo merge.
- Nenhuma promessa nova de API, então não há seção «Para a v6». O comando não é API de MOD: `api/v1..v5.json` não o mapeiam (conferido com `grep`).
- O nome do comando ficou o mesmo.
- O QA-01 da gestão continua reaplicando, e o guarda dele está abaixo.

## 1. A medida

### O que o comando fazia na base

`instalar_mod_do_catalogo` (`apps/seele-app/src/main.rs`) lia
`mod_exigido_aqui` antes de baixar. Depois do download, se o servidor que esta
janela hospeda já exigia aquele MOD, chamava `aplicar_mod(app, session, id,
hash, true)`. Fazia isso para qualquer chamada, porque não sabia quem chamava.

`aplicar_mod` faz duas coisas:
- **Grava a exigência.** `habilitar_mod` lê o pacote pelo hash e grava a linha
  com `seele_server::persistence::mods::enable`, que é
  `INSERT … ON CONFLICT(id) DO UPDATE`, um registro por identificador. Gravar
  o hash da obtenção **substitui** a versão que o servidor hospedado exigia. Em
  seguida, `anotar_mudanca_nos_mods` acorda o anúncio, e o servidor derruba as
  sessões cujo conjunto mudou.
- **Regrava o sim.** Grava o sim desta máquina para `endereco_da_sessao` com a
  identidade do conjunto **do servidor hospedado**. Ver o efeito 2, abaixo.

### Medido

O teste `obter_o_que_outro_servidor_exige_nao_troca_a_versao_do_servidor_hospedado`
fica no módulo `so_a_gestao_reaplica_no_servidor_hospedado` do `main.rs`.

O cenário:
- o banco de verdade do servidor hospedado (`Persistence`, em memória) exige
  `autor/rpg` 1.0.0;
- a janela obtém a 2.0.0, que outro servidor exige;
- roda a decisão do comando e, se ela mandar, o que `aplicar_mod` grava.

Escrevi o teste com a decisão da base (`let _ = uso; ja_exigido`), e ele ficou
vermelho:

```
assertion `left == right` failed: obter o MOD que outro servidor exige trocou a versão exigida pelo servidor que esta janela hospeda, de 1.0.0 para 2.0.0: quem estava dentro dele cai, e ele passa a exigir uma versão que quem hospeda não escolheu (ADR 0045)
  left: ("2.0.0", "d600d3f3dcfa534dafd71c80663ced691d7324d188cbba8ddd3a6691165e3e82")
 right: ("1.0.0", "1d1de534032838e266a683079d3f333b59423f42cfcb074280dfc5f446fb08f2")
```

A versão e a identidade do anúncio trocam. O defeito acontece como o brief
descreve.

**A parte «depois de desistir».** O Rust não vê a desistência, porque ela é a
mesma chamada. Quem a mede é a cena 5 da `ajustes-v013.cjs`: a pessoa recusa a
pergunta durante uma obtenção de 600 ms, e a chamada termina depois do «não».
Na base, a asserção nova ficou vermelha com `usos: [null]`. A janela não dizia
o uso, e o Rust reaplicava ao fim da chamada, já depois da desistência.

### O alcance: quando esta janela hospeda e obtém para outro servidor

Medir mostrou que a condição «esta janela hospeda» é mais estreita do que
parece. O defeito acontece como descrito, mas não em todo caminho.

**Trocar de servidor de dentro da sessão não leva ao defeito.**
`trocarDeServidor` (`ui/tela-sessao.js`) chama `ejetar()`, que chama
`invoke("disconnect")`. O `disconnect` do `main.rs` derruba a hospedagem
(`hospedagem.take()` e `encerrar()`). Quando a pergunta do outro servidor
aparece, esta janela já não hospeda, e `mod_exigido_aqui` responde «não».

**O defeito precisa de uma hospedagem de pé com a janela na entrada.** Os
caminhos, lidos no código:
- **A entrada no próprio servidor falha ou é recusada.** `hospedar()`
  (`tela-boot.js`) sobe o servidor e chama `conectar()`. Se a entrada falha,
  ou a pessoa recusa a pergunta dos MODs do próprio servidor, o `catch` só
  escreve o erro. A hospedagem continua.
- **VOLTAR da tela de fim.** Usa `limparSessaoEncerrada({ pararDeHospedar:
  false })` (`tela-fim.js`), que mantém a hospedagem de pé. Se a reconexão
  falha, a janela volta à entrada hospedando.

Da entrada, `irParaOServidor` (`camada-servidores.js`) chama `conectar` direto,
sem `disconnect`, e o `connect` do Rust não toca a hospedagem. Daí:
- **A pergunta dos MODs** do outro servidor leva à obtenção, que reaplicava no
  servidor daqui.
- **Uma entrada que dá certo** põe a janela na sessão de outro servidor com a
  hospedagem de pé. Aí `buscarOQueFaltaSeJaFoiAceito` (`base.js`) também
  reaplicava.

**O efeito 2, lido e não medido.** Na sessão com o outro servidor S,
`endereco_da_sessao` é S. Então `aplicar_mod` também gravava como sim **de S**
a identidade do conjunto **do servidor daqui**. `Aceites::guardar` substitui o
sim por alvo (`crates/seele-core/src/aceites.rs`): o sim que a pessoa deu a S
era trocado por um que não descreve S, e a entrada seguinte em S perguntava de
novo. O conserto também fecha isto, porque a obtenção não chama mais
`aplicar_mod`.

## 2. O conserto

### Por que um argumento, e não um comando próprio para a obtenção

**A escolha foi um argumento obrigatório, sem padrão.** O desenho veio aprovado
pela orquestradora: «um argumento ao comando diz qual dos dois usos é». As
alternativas perdiam pelos motivos abaixo.
- **Um comando próprio para a obtenção** duplicaria o registro no
  `invoke_handler` e a ACL. Também deixaria o comando antigo reaplicando para
  quem o chamasse sem pensar, e esse é exatamente o defeito.
- **Um `Option<UsoDoCatalogo>` com padrão** cala nos dois sentidos. Com padrão
  «reaplicar», é o achado deste lote. Com padrão «não reaplicar», é o QA-01 de
  volta.
- **O argumento obrigatório** faz o Tauri recusar a chamada que não diz o uso
  antes de o corpo rodar, e portanto antes de baixar. A recusa cai no `catch`
  de cada chamada, que a escreve na tela:
  - na pergunta, `não consegui obter …`;
  - na sessão, a fase `nao-obtive`;
  - na gestão, a linha do catálogo.

### O que ficou

- `UsoDoCatalogo` (`Atualizar` ou `Obter`, serializado em minúsculas):
  `"atualizar"` e `"obter"`.
- `reaplica_no_hospedado(uso, ja_exigido) = ja_exigido && uso == Atualizar`.
- O comando decide antes de baixar, como antes, e devolve `Ok(reaplicar)`. A
  frase do QA-03, na gestão, continua lendo o que de fato aconteceu.

## 3. Os guardas e as reversões

**Os guardas novos:**
- `main.rs`, módulo `so_a_gestao_reaplica_no_servidor_hospedado`:
  - `obter_o_que_outro_servidor_exige_nao_troca_a_versao_do_servidor_hospedado`, a medida acima;
  - `a_atualizacao_pela_gestao_continua_reaplicando`, o QA-01 (a exigência passa à 2.0.0), e atualizar um MOD não exigido não o liga;
  - `a_janela_diz_o_uso_com_as_palavras_que_o_comando_entende`: `"obter"` e `"atualizar"` passam, `null` e `"Atualizar"` não;
  - `o_comando_reaplica_pela_decisao_que_le_o_uso`: o corpo do comando, sem comentários, recebe `uso: UsoDoCatalogo,`, decide por `reaplica_no_hospedado(uso,` antes de `if reaplicar {`, chama `aplicar_mod(` uma vez só e devolve `Ok(reaplicar)`.
- `tests/frontend.rs`, `so_a_atualizacao_pela_gestao_reaplica_o_mod_no_servidor_hospedado`:
  `obterOConjunto` e `buscarOQueFaltaSeJaFoiAceito` dizem `uso: "obter"`, e
  `instalarDoCatalogo` diz `uso: "atualizar"`. Também confere que a `ui/`
  inteira, sem comentários, tem exatamente essas três chamadas, todas com uso.
- `bancada/ajustes-v013.cjs`, cena 5: toda obtenção da pergunta, inclusive a
  que termina depois do «não», diz `uso: 'obter'`.

**Como as reversões foram feitas.** Cada uma troca o trecho exato por um script
no scratchpad, roda a prova, devolve o arquivo de uma cópia e confere com
`cmp`. Todas voltaram idênticas.

**Um cuidado.** `cp -p` devolve o mtime antigo, e o cargo não recompilou na
volta. O primeiro `--bin` inteiro depois das reversões rodou um binário velho e
reprovou o guarda das palavras. Tocar os arquivos e recompilar deu 288 de 288.
Todas as provas vermelhas abaixo vieram de binários recompilados: cada uma
reprovou exatamente o teste da mudança.

| reversão | prova | frase |
|---|---|---|
| decisão → `ja_exigido` (a da base) | `--bin`: `obter_o_que_outro_servidor…` | «… trocou a versão exigida … de 1.0.0 para 2.0.0 …» |
| decisão → `false` | `--bin`: `a_atualizacao_pela_gestao_continua_reaplicando` | «… deixou de reaplicar … (QA-01)» |
| comando decide por `mod_exigido_aqui` sozinho | `--bin`: `o_comando_reaplica_pela_decisao_que_le_o_uso` | «… deixou de reaplicar só pela decisão que lê o uso …» |
| sem `#[serde(rename_all = "lowercase")]` | `--bin`: `a_janela_diz_o_uso…` | «o comando deixou de entender a palavra que a janela manda …» |
| `obterOConjunto` sem `uso` | `--test frontend` e `ajustes-v013.cjs` | «a obtenção da pergunta dos MODs de outro servidor não diz `uso: "obter"` …» e `usos: [null]` |
| `obterOConjunto` com `uso: "atualizar"` | os mesmos dois | a mesma frase, e `usos: ["atualizar"]` |
| `buscarOQueFaltaSeJaFoiAceito` sem `uso` | `--test frontend` | «a obtenção da sessão que já aceitou não diz `uso: "obter"` …» |
| `instalarDoCatalogo` com `uso: "obter"` | `--test frontend` | «a atualização pela gestão (QA-01) não diz `uso: "atualizar"` …» |
| uma quarta chamada sem uso na `ui/` | `--test frontend` | `[true, true, true, false]` contra `[true; 3]` |

**Os testes no estado final:**

| o quê | resultado |
|---|---|
| `--bin seele-app` | 288 de 288 |
| `--test frontend` | 284 de 284 |
| `cargo xtask check-runtime` | verde |
| `cargo fmt --all -- --check` | limpo |
| clippy do app (`--all-targets --all-features -D warnings`) | limpo |
| `ajustes-v013.cjs` e `diagnostico-de-mods.cjs` | verdes |
| `tools/roteiros/catalogo-de-mods.js` e `aceite-de-mods.js`, pela `carga-da-casca.py` | carga limpa; o de aceite sai igual ao da base |

**A conformidade** rodou por arquivo, um de cada vez, com `--test-threads=1`:
`aceite_dos_mods` deu 15 de 15 e `volta_do_aceite` deu 2 de 2. Elas não tocam
o comando do app, e rodaram antes do aviso de pausa da orquestradora. Depois
dele, nenhum teste de `seele-conformance` rodou.

## 4. A v0.15.1 publicada

A lista de releases do SEELE-RELEASES confirma que a v0.15.1 saiu de
`54f542f94`, publicada em 08/10.

**Lido: o defeito existe lá, pelo mesmo motivo.**
- Em `54f542f`, `instalar_mod_do_catalogo` é a mesma função: `let ja_exigido =
  mod_exigido_aqui(…)`, depois `if ja_exigido { aplicar_mod(…) }`, sem saber
  quem chama.
- As três chamadas são as mesmas, sem argumento de uso: `obterOConjunto`, por
  volta da linha 239 de `camada-mods.js`; `buscarOQueFaltaSeJaFoiAceito`, por
  volta da 2359 de `base.js`; e a gestão, por volta da 2026 de `camada-mods.js`.
- `enable` também é `ON CONFLICT(id) DO UPDATE`.
- O alcance é o mesmo: `trocarDeServidor` passa por `ejetar` e pelo
  `disconnect`, que derruba a hospedagem; `hospedar()` mantém a hospedagem
  quando a entrada falha; e a tela de fim tem o VOLTAR que não para de
  hospedar.

**Depois de desistir, a v0.15.1 é pior.** Lá, `aceitarOsMods` grava o sim
**antes** de obter. Depois da obtenção, não confere se a pergunta ainda está
aberta: chama `fecharAceiteDeMods()` e `conectar()`. Quem desiste no meio da
obtenção tem o MOD reaplicado no servidor daqui e ainda vê a janela entrar
sozinha no outro. Os lotes R e W já consertaram a segunda parte na 0.16.

**Rodado: vermelho, como na base da 0.16.** Não usei `git worktree add`:
- `git archive 54f542f` foi extraído no scratchpad, com o `target` semeado por
  `cp -c`, e o build levou 1 min 17 s;
- no `main.rs` daquela árvore entrou um módulo de teste com a mesma medida,
  sem o campo `api`, que a v0.15.1 não tem;
- a decisão é a da v0.15.1 como ela está escrita: `if ja_exigido`.

```
assertion `left == right` failed: obter o MOD que outro servidor exige trocou a versão exigida pelo servidor que esta janela hospeda, de 1.0.0 para 2.0.0
  left: ("2.0.0", "d600d3f3dcfa534dafd71c80663ced691d7324d188cbba8ddd3a6691165e3e82")
 right: ("1.0.0", "1d1de534032838e266a683079d3f333b59423f42cfcb074280dfc5f446fb08f2")
```

A árvore temporária foi apagada depois.

A 0.15.2 não leva este conserto. Anotar ou não nas pendências fica com a
orquestradora.

## 5. O que mudou em cada arquivo

- **`apps/seele-app/src/main.rs`.** Duas regiões, as duas junto do comando,
  por volta das linhas 7689 a 7930. Nada perto de `aplicar_mod`,
  `habilitar_mod` ou `persistence_do_mod`, que não mudaram.
  - Antes de `instalar_mod_do_catalogo`: o enum `UsoDoCatalogo` e a função
    `reaplica_no_hospedado` (36 linhas, quase todas de doc).
  - No comando: o argumento `uso`, a decisão, `if reaplicar`,
    `Ok(reaplicar)`, um parágrafo de doc e duas linhas de comentário.
  - Depois de `mod_exigido_aqui`: o módulo de teste
    `so_a_gestao_reaplica_no_servidor_hospedado`, com cerca de 150 linhas.
- **`apps/seele-app/ui/camada-mods.js`.** Só as duas chamadas:
  - em `obterOConjunto`, `uso: "obter"` e quatro linhas de comentário;
  - em `instalarDoCatalogo`, `uso: "atualizar"` e uma linha a mais no
    comentário do QA-03.

  Longe do `NOMES_DOS_PONTOS`, onde a F3 vai mexer.
- **`apps/seele-app/ui/base.js`.** Só a chamada de
  `buscarOQueFaltaSeJaFoiAceito`: `uso: "obter"` e duas linhas de comentário.

Fora dos três:
- **`apps/seele-app/tests/frontend.rs`.** Um teste novo, logo depois de
  `o_conjunto_ja_aceito_volta_ao_disco_e_nenhum_outro`.
- **`apps/seele-app/bancada/ajustes-v013.cjs`.** Oito linhas na cena 5.

O guarda do R (`a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`)
e o do W (`o_aceite_obtem_o_conjunto_antes_de_entrar`) continuam verdes, sem
mudança. `obter.contains("versao: m.version")` ainda casa.

## Preocupações

1. **O alcance é mais estreito do que o brief supunha.** O defeito exige a
   janela hospedando enquanto está na entrada; a troca de servidor de dentro
   da sessão derruba a hospedagem. Ver 1, «O alcance». A medida confirma o
   mecanismo, mas não houve uma rodada no app de verdade: o catálogo é
   `https://mods.seele.app.br`, fixo, e não sobe em teste. A hospedagem também
   não sobe num teste de unidade, porque `Hospedagem::iniciar` abre UPnP e PCP
   para o roteador.
2. **A mesma confusão de endereço continuava em dois gravadores da gestão.**
   Esta janela pode estar na sessão de S com o servidor daqui de pé, pelos
   caminhos da seção 1. Nessa hora, dois gestos da gestão gravavam o sim de S
   com a identidade do servidor daqui (o efeito 2):
   - atualizar um MOD exigido aqui (`aplicar_mod`, o QA-01);
   - o SALVAR (`aplicar_conjunto_de_mods`).

   A primeira versão deste relatório citava só o QA-01. **Consertado na
   rodada 1**, item 1: ver a seção «Rodada 1».
3. **O teste de Rust replica duas linhas do comando.** São a leitura de
   `mod_exigido_aqui` e a gravação de `aplicar_mod`, porque o comando tem
   `AppHandle` e rede. O que amarra a réplica ao comando é o guarda de
   texto-fonte `o_comando_reaplica_pela_decisao_que_le_o_uso`.
   Esse guarda casava com o próprio literal até a rodada 1 (item 2).

## Rodada 1 (FIX_BASE `bcb7333d`)

A revisão aprovou o lote e deixou três itens: um Importante, que é o efeito 2
deste relatório, medido como alcançável por um fluxo comum, e dois Menores.

| commit | item | o que muda |
|---|---|---|
| `384d6a62` | 2 (Menor) | a agulha de `o_comando_reaplica_pela_decisao_que_le_o_uso` sai de `concat!` |
| `48a5f51a` | 1 (Importante) | o sim de quem hospeda passa por `gravar_o_sim_de_quem_hospeda` nos dois gravadores, e só é gravado na sessão do servidor daqui |
| este | 3 (Menor) | o relatório cita o SALVAR, e ganha esta seção |

### Item 1: o sim do servidor S gravado com a identidade daqui

**A medida.** Os dois gravadores, `aplicar_mod` (o QA-01) e
`aplicar_conjunto_de_mods` (o SALVAR), têm `AppHandle` e pedem uma hospedagem
de pé. Num teste, a hospedagem abriria UPnP e PCP no roteador. Por isso o
teste `com_a_sessao_em_outro_servidor_o_sim_dele_fica_como_estava` roda a
gravação que os dois fazem, extraída para `gravar_o_sim_de_quem_hospeda`.

O cenário:
- o arquivo de aceites é o de verdade, numa pasta de passagem;
- a sessão está em `198.51.100.7:8383`;
- a identidade é a que cada gravador calcula, sobre o banco de verdade do
  servidor hospedado: `definir_conjunto` para o SALVAR, `enable` em cima de
  `enable` para o QA-01.

O teste mede os dois gravadores antes de reprovar. Com o auxiliar no
comportamento da base (sempre grava), ele ficou vermelho:

```
com a sessão em outro servidor, o sim dele virou a identidade do servidor daqui.
Trocaram o sim que a pessoa tinha dado: ["o SALVAR da gestão (`aplicar_conjunto_de_mods`)", "a atualização pela gestão (`aplicar_mod`, QA-01)"], e ele pergunta de novo.
Criaram um sim que ela nunca deu: [os mesmos dois], e a lista de aceites mente; com os conjuntos iguais, a entrada nele não pergunta (ADR 0045)
```

Na base de verdade, o guarda de texto-fonte também reprovou os dois:
`["aplicar_mod", "aplicar_conjunto_de_mods"] grava o sim sem passar por
gravar_o_sim_de_quem_hospeda`.

**O conserto.**
- `endereco_do_servidor_daqui()` é o `aqui` de `hospedar`, que agora o usa:
  `127.0.0.1:{PORTA_PADRAO}`.
- `a_sessao_e_com_o_servidor_daqui(endereco)` compara as duas chaves de
  `seele_ffi::chave_do_servidor`, que é a chave em que o aceite é guardado.
  Assim `127.0.0.1` e `127.0.0.1:8383` são o mesmo servidor.
- `gravar_o_sim_de_quem_hospeda(config, session, identidade)` lê
  `endereco_da_sessao` e só grava quando o auxiliar diz que sim.
- Os dois gravadores passam por ele. O QA-01 continua reaplicando: muda só para
  onde vai o sim.

**Por que o `aqui`, e não as outras duas comparações:**
- **A impressão digital** exigiria a chave do servidor da sessão. A conexão
  não a entrega à casca, porque `Trust::Known` não a traz. Pegá-la seria mexer
  no `seele-ffi`, e a rodada é só no `main.rs`.
- **O prefixo de loopback** (`hospedado_aqui`) diz «nesta máquina», e o
  HOSPEDAR COM ESTA da F6 põe outra versão em `127.0.0.1`.
- **O `aqui`** é exatamente o endereço com que a tela entra no próprio
  servidor: `ultimoAlvo = anfitriao.aqui`, e o VOLTAR copia o alvo da sessão.
  Quem chama já sabe que esta janela hospeda (`persistence_do_mod`), e duas
  hospedagens não seguram a mesma porta.

**O limite, e para que lado ele erra.** *Corrigido na rodada 2.* A primeira
versão desta frase dizia que nenhum caminho da tela entra no próprio servidor
por outro endereço que não o `aqui`, e isso era falso.

O caminho existe:
- **a pré-condição** é a medida na seção 1: a janela está na entrada, com a
  hospedagem de pé;
- a pessoa cola o próprio link no CONECTAR;
- o convite nunca aponta para o loopback, só para o endereço da rede local e
  os alvos do alcance (`Hospedagem::convite`), então a sessão fica no
  endereço da rede local;
- a lista de conhecidos o guarda, e o VOLTAR o reusa.

Com o portão desta rodada, só o `aqui`, o SALVAR e a atualização pela gestão
deixavam de gravar o sim nesse caminho, e a volta perguntava de novo (o A04).
**O caminho está fechado na rodada 2**: o portão aceita também os endereços
que o convite da hospedagem anuncia. Ver «Rodada 2».

**Os guardas:**
- `com_a_sessao_em_outro_servidor_o_sim_dele_fica_como_estava`: a medida, que
  cobre o sim trocado e o sim criado onde não havia.
- `na_sessao_do_servidor_daqui_o_sim_continua_gravado`: o A04, na sessão do
  `aqui` o sim é gravado; e `127.0.0.1:8384` não é o servidor daqui.
- `os_dois_gravadores_passam_pelo_sim_de_quem_hospeda`: o guarda de
  texto-fonte, com agulhas de `concat!`. Cada gravador chama o auxiliar e não
  tem `seele_ffi::mods::aceitar(` nem `endereco_da_sessao` no corpo, e
  `hospedar` devolve `aqui: endereco_do_servidor_daqui()`.
- O guarda do QA-02 no `frontend.rs`
  (`o_aceite_de_quem_hospeda_e_gravado_para_o_endereco_onde_ele_esta`) ficou
  vermelho com o conserto, porque procurava `endereco_da_sessao` no corpo de
  `aplicar_mod`. Ele passou a seguir a delegação: `aplicar_mod` chama o
  auxiliar, e o auxiliar lê `endereco_da_sessao`, não `session.alvo`. A
  intenção dele não mudou.

**As reversões.** Todas por script, com o arquivo devolvido de uma cópia,
conferido com `cmp` e tocado antes de recompilar. Todas ficaram vermelhas com a
frase do guarda.

| reversão | o que ficou vermelho |
|---|---|
| auxiliar → sempre `true` (a base) | a medida (os dois gravadores) e o vizinho de loopback |
| auxiliar → `hospedado_aqui` | o vizinho de loopback: «tomou por servidor daqui um servidor em outra porta desta máquina» |
| `aplicar_mod` grava direto em `endereco_da_sessao` | o guarda de texto-fonte, `["aplicar_mod"]`, e o do QA-02 |
| `aplicar_conjunto_de_mods` grava direto | o guarda de texto-fonte, `["aplicar_conjunto_de_mods"]` |
| `hospedar` volta ao `format!` à mão | o guarda de texto-fonte, na parte do `aqui` |
| o auxiliar lê `session.alvo` | o guarda do QA-02 |

**A v0.15.1.** Confirmado: os dois gravadores gravam em `endereco_da_sessao`,
`aplicar_mod` em `:4506-4515` e `aplicar_conjunto_de_mods` em `:6181-6186`.

### Item 2: o guarda que casava com o próprio literal

Reproduzi a sonda C em `bcb7333d`: o comando renomeado para
`instalar_mod_pelo_catalogo`, o registro junto e o corpo de volta à decisão da
base. O guarda ficou verde, 4 de 4. Com a agulha de `concat!`, a mesma sonda
reprova com «`async fn instalar_mod_do_catalogo(` não está mais neste arquivo».
O arquivo voltou idêntico, conferido com `cmp`. Os guardas novos do item 1
nasceram com o mesmo cuidado.

### Item 3

A preocupação 2 passou a citar o SALVAR, e diz que foi consertada aqui.

### Os testes da rodada

| o quê | resultado |
|---|---|
| `--bin seele-app` | 291 de 291 |
| `--test frontend` | 284 de 284 |
| `check-runtime` | verde |
| fmt e clippy do app | limpos |

A `ui/` e as bancadas não mudaram nesta rodada. A conformidade está pausada
pela orquestradora: `aceite_dos_mods` e `volta_do_aceite` rodam depois do
aviso. Nenhum dos dois toca os gravadores do app.

## Rodada 2 (FIX_BASE `90a5ed44`)

A re-revisão deu os três pontos da rodada 1 como resolvidos e provados por
reversão. Sobraram dois Menores.

| commit | item | o que muda |
|---|---|---|
| `f4f557cf` | 1 | o portão do sim de quem hospeda aceita os endereços que o convite da hospedagem anuncia |
| `2daaf8cf` | 2 | o guarda da dispensa em `tests/frontend.rs` prende `ultimoAlvo = anfitriao.aqui` |
| este | relatório | a frase do limite corrigida, e esta seção |

### Item 1: o caminho do próprio link

**O caminho.** É o que a seção «O limite» descreve agora. A janela está na
entrada com a hospedagem de pé, e a pessoa cola o próprio link no CONECTAR. A
sessão fica no endereço da rede local que o convite anuncia, e o VOLTAR o
reusa pela lista de conhecidos. Com o portão da rodada 1, que só aceitava o
`aqui`, o SALVAR e a atualização pela gestão não gravavam o sim ali. Antes da
rodada 1, gravavam.

**O conserto, só no `main.rs`:**
- `Session` ganha `enderecos_do_convite_daqui`. `hospedar` o preenche com
  `enderecos_do_convite(&anfitriao.convite)`: o alvo e os alternativos do
  **mesmo** convite que devolve à tela, lidos por `seele_ffi::uri::analisar`.
- O `disconnect` esquece esses endereços junto com a hospedagem.
- `a_sessao_e_com_o_servidor_daqui(endereco, do_convite)` aceita o `aqui` e
  esses endereços, comparando sempre na chave do aceite.
- `gravar_o_sim_de_quem_hospeda` lê os dois da sessão. Nada no `seele-ffi`.

**Por que o convite devolvido, e não `Hospedagem::convite()` lido na hora.** O
devolvido é o que a hospedagem anuncia de fato. Com um nome público, o alvo
dele é o nome, e os números descem para alternativos
(`Hospedagem::convite_com_nome`). Ler `convite()` na hora perderia o nome. Ler
o devolvido também deixa o teste montar a sessão sem subir uma hospedagem, o
que abriria UPnP e PCP no roteador.

**A prova.** `pelo_proprio_link_o_sim_de_quem_hospeda_continua_gravado` monta o
convite como `Hospedagem::convite` monta: `192.168.1.20:8383`, depois
`203.0.113.9:8383`, com a impressão digital. Com a sessão em cada um desses
endereços, os dois gravadores gravam o sim. Em S (`198.51.100.7:8383`), com o
mesmo convite à mão, não gravam. Com o portão da rodada 1, o teste ficou
vermelho: «o sim de quem hospeda não foi gravado», nos dois gravadores e nos
dois endereços. O guarda de texto-fonte
`os_dois_gravadores_passam_pelo_sim_de_quem_hospeda` passou a cobrar que
`hospedar` guarde os endereços do convite que devolve e que o `disconnect` os
esqueça.

**As reversões.** As três ficaram vermelhas, o `main.rs` voltou idêntico
(`cmp`) e foi tocado antes de recompilar.

| reversão | o que ficou vermelho |
|---|---|
| o portão só com o `aqui` (o estado da rodada 1) | `pelo_proprio_link_…` |
| `hospedar` guarda uma lista vazia | o guarda de texto-fonte |
| o `disconnect` não esquece | o guarda de texto-fonte |

### Item 2: a ponta da janela

`the_flag_that_skips_the_key_check_is_cleared_before_the_attempt_and_not_after`
(`tests/frontend.rs`) já prendia `nossoServidorEm = anfitriao.aqui`, e agora
prende também `ultimoAlvo = anfitriao.aqui`. A prova foi trocar, em
`tela-boot.js`, `ultimoAlvo` por `localhost:<porta>`: o guarda reprovou. O
arquivo voltou idêntico (`cmp`) e não entra em commit nenhum, porque o
`tela-boot.js` não mudou.

**Se essa ponta ainda importa com o portão novo: sim, do mesmo jeito, no
caminho comum.** O portão aceita o `aqui` e os endereços do convite, e o
convite nunca aponta para o loopback. Se `hospedar` entrasse por `localhost` ou
`[::1]`, o endereço não estaria em nenhuma das duas listas. O SALVAR e a
atualização pela gestão deixariam de gravar o sim, e a dispensa de conferir a
própria chave (`nossoServidorEm === ultimoAlvo`) também deixaria de casar.

### Os testes da rodada 2

| o quê | resultado |
|---|---|
| `--bin seele-app` | 292 de 292 |
| `--test frontend` | 284 de 284 |
| `check-runtime` | verde |
| fmt e clippy do app | limpos |

A conformidade continua pausada. `aceite_dos_mods` e `volta_do_aceite` rodam
quando a orquestradora liberar.

## O merge da décima integração (`30057aa8`)

`versao/0.16` em `e3970b14` entrou com dois conflitos.

- **`docs/pendencias.md`.** Do nosso lado não havia nada, e a versão traz a 59
  inteira. Fiquei com a da versão, byte a byte.
- **`apps/seele-app/src/main.rs`.** A F7 trocou `hospedagem` e
  `servidor_no_ar` de `Session` por `hospedagens: Mutex<Hospedagens>`, com
  `guardado` e `pessoal: Option<Hospedado>`. O campo
  `Session::enderecos_do_convite_daqui` da rodada 2 não tinha mais onde ficar.
  - **Com autorização da coordenação**, os endereços passaram a morar em
    `Hospedado::enderecos_do_convite`, preenchido no mesmo
    `Some(Hospedado { … })` de `hospedar` com
    `enderecos_do_convite(&anfitriao.convite)`.
  - Eles morrem com a hospedagem, e o trecho do `disconnect` que os esquecia
    saiu.
  - No desenho da F7 mudou só isto: um campo a mais, e os dois `Hospedado` do
    teste `a_saida_despede_as_hospedagens` o recebem vazio.

**O que o desenho novo pediu no lote X:**
- **`gravar_o_sim_de_quem_hospeda`** lê o endereço da sessão e os endereços
  do convite do **guardado**, e passa os dois a
  `gravar_o_sim_se_a_sessao_e_daqui`.
- **O teste do próprio link** roda essa segunda função. Montar um `Hospedado`
  pediria subir uma hospedagem, e o teste da F7 faz isso com `iniciar_sala`,
  que sobe a escada de UPnP e PCP.
- **O guarda de texto-fonte** deixou de cobrar o `disconnect`. Agora cobra
  três coisas: que o `Hospedado` nasça com os endereços do convite, que o sim
  os leia do guardado e não da sala, e que os passe na chamada exata.
- **Quatro reversões no lugar novo**, todas vermelhas:
  - o `Hospedado` sem os endereços;
  - o sim lendo a sala;
  - o sim sem passar os endereços;
  - o portão só com o `aqui`.

  A terceira passou verde na primeira versão do guarda, que só procurava o
  nome da função. O guarda foi apertado para cobrar a chamada exata, e a
  reversão passou a reprovar. O arquivo voltou idêntico (`cmp`) e foi tocado
  antes de recompilar.

**O que achei no desenho novo da F7:**
- **Duas hospedagens podem estar de pé.** São o guardado, que a pessoa sobe
  com HOSPEDAR, e a sala pessoal, que hoje ninguém sobe fora dos testes.
  - «O servidor que esta janela hospeda», para o sim dos MODs, é o
    **guardado**. É nele que moram os MODs (`persistence_do_mod` usa
    `do_guardado`), e a sala é «em memória e sem MODs».
  - Os dois gravadores só chegam ao sim depois de `persistence_do_mod`.
- **O `aqui` continua sendo o endereço do guardado.** O guardado sobe na
  `PORTA_PADRAO`, a 8383, a mesma do `aqui` (`127.0.0.1:{PORTA_PADRAO}`). A
  sala sobe na `PORTA_DA_SALA`, a 8385. O portão não precisou mudar de
  critério.
- **Há agora duas respostas a «estou no meu servidor?».** A F7 trouxe
  `onde_estou` e `onde_estou_em` e os chama de «a única resposta». O critério
  deles:
  - é a porta do guardado, num endereço de loopback escrito como `127.0.0.1`,
    `localhost` ou `[::1]`;
  - não reconhece os endereços do convite, então o próprio link dá `Outro`.

  O portão do sim (`a_sessao_e_com_o_servidor_daqui`) reconhece o `aqui` e os
  endereços do convite. Não reconhece `localhost:8383` nem `[::1]:8383`, que
  `onde_estou` reconhece. Não unifiquei: seria mexer no critério da F7, ou dar
  ao sim um critério que a rodada 2 não pediu. Fica para a orquestradora
  decidir se o portão passa a ser `onde_estou_em(…) == Guardado` mais os
  endereços do convite.

**No estado mesclado:**

| o quê | resultado |
|---|---|
| `--bin seele-app` | 307 de 307 |
| `--test frontend` | 288 de 288 |
| `check-runtime` | verde |
| fmt e clippy do app | limpos |
| `ajustes-v013.cjs` e `diagnostico-de-mods.cjs` | verdes |
| conformidade `aceite_dos_mods` | 15 de 15 |
| conformidade `volta_do_aceite` | 2 de 2 |

A conformidade rodou um arquivo de cada vez, com `--test-threads=1`, depois da
liberação.

## A impressão: o portão pela chave (commits A, B e C)

A orquestradora decidiu que «estou no meu servidor?» tem uma resposta só, a
`onde_estou_em` da F7, e que o critério passa a ser **a chave**, e não o
endereço. Esta seção substitui o portão das rodadas 1 e 2 e do merge, que era o
`aqui` mais os endereços do convite. A leitura dos endereços na hora, que
chegou a ser escrita, não foi commitada; ela está em
`scratchpad/lotex-r1/leitura-na-hora.patch`.

### A medida, antes do conserto

**A casca não tinha a chave da sessão em todos os casos.**
- O `Trust` do `connect` traz a impressão em `FirstContact`,
  `FirstContactVerified` e `InviteDisagrees`. Em `Known` não traz, nem no core
  nem na FFI.
- O caso mais comum é justamente esse. Quem hospeda entra no próprio servidor
  pelo `aqui`, sem link, num servidor que a lista de conhecidos exclui. Da
  segunda vez em diante o veredito é `Known` sem impressão esperada, e a casca
  fica sem chave.
- O pin não serve: ele é do `seele-core`, e ler o arquivo `pins` na casca
  passaria por cima do ADR 0002.
- Não há loopback sem certificado: toda conexão passa pelo `TofuVerifier`.

**A FFI tinha a chave e a jogava fora.** Em toda conexão que sobe,
`client.pin_decision()` é `FirstContact { fingerprint }` ou
`Matches { fingerprint }`.

**As chaves das hospedagens já estavam à mão**, em
`Hospedagem::impressao_digital()`.

### O que mudou

| commit | o que muda |
|---|---|
| `a85fcb1b` (A) | `Shared::impressao_do_servidor`, gravada por `gravar_impressao` a partir do `pin_decision()` no único aperto de mão, e `Connection::impressao_do_servidor()`, em `crates/seele-ffi/src/lib.rs`. `Trust::Known` não muda de forma. A prova é o arquivo novo `crates/seele-conformance/tests/impressao_da_sessao.rs`. |
| `251280d1` (B) | `onde_estou_em(endereco, impressao, do_guardado, da_sala)` compara a impressão da sessão com a do guardado e a da sala, sem diferenciar maiúsculas. O endereço só diz se há sessão; com sessão e sem impressão, a resposta é `Outro`. `Hospedagens::portas` vira `impressoes`, e `onde_estou_na(&Session)` é a mesma resposta para quem não é comando. |
| `c83cd779` (C) | `gravar_o_sim_de_quem_hospeda` passa a decidir por `onde_estou_na(session)`, e `gravar_o_sim_se_for_o_guardado` grava só com `Guardado`. Saem `a_sessao_e_com_o_servidor_daqui`, `enderecos_do_convite`, `endereco_do_servidor_daqui` e o campo `Hospedado::enderecos_do_convite`. O `Hospedado`, `hospedar` e o teste da F7 voltam a ser exatamente os da versão. |

**O que isto fecha:**
- **Os nomes do mesmo servidor.** `127.0.0.1`, `localhost`, `[::1]`, a rede
  local, o endereço público e o nome são todos `Guardado`. Isso fecha também o
  link com nome público, que a leitura dos endereços do convite não
  alcançava.
- **Um endereço velho.** O mesmo endereço com outra chave é `Outro`: o servidor
  de outra versão que o HOSPEDAR COM ESTA da F6 põe em `127.0.0.1`, ou um
  endereço da rede local que hoje é de outra máquina. Aí o sim não é gravado,
  e o erro fica do lado fechado.

O guarda de `ultimoAlvo = anfitriao.aqui`, no `frontend.rs`, continua valendo
para a dispensa de conferir a própria chave (`nossoServidorEm === ultimoAlvo`).
O sim não depende mais dele.

### Os testes e as reversões

Os testes:
- **A, conformidade.** `a_conexao_diz_a_impressao_do_servidor_que_conferiu`
  sobe a hospedagem com `Hospedagem::iniciar` e entra três vezes com a mesma
  casa: o primeiro contato, a volta com `Known` e `localhost`. Nas três, o
  getter é igual a `impressao_digital()`.
- **B.** `onde_estou_pela_chave_e_nao_pelo_endereco` cobre sem sessão, os cinco
  endereços com a chave em maiúsculas, e a sala.
  `o_mesmo_endereco_com_outra_chave_e_outro_lugar` cobre S em `127.0.0.1:8383`,
  na rede local e na porta da sala, cada hospedagem fora do ar, e a sessão sem
  impressão.
- **C.** Quatro casos sobre `onde_estou_em`, mais a gravação no arquivo de
  aceites de verdade, nos dois gravadores:
  - S noutro endereço e no mesmo endereço do guardado;
  - o próprio link pela rede local, e o `aqui`;
  - a sala;
  - a ausência de guardado, também pela `Session` de verdade.

  O guarda de texto-fonte (agulhas de `concat!`, fonte sem comentários) prende
  os dois gravadores a `gravar_o_sim_de_quem_hospeda`, ela a
  `onde_estou_na(session)`, o portão a `OndeEstou::Guardado`, e o comando
  `onde_estou` à mesma resposta.

Em cada commit, o vermelho veio antes:
- **A:** o getter devolvendo `None` reprovou já no primeiro contato.
- **B:** os testes rodaram primeiro com o critério de antes, a porta de
  loopback.
- **C:** o portão ignorando a resposta.

As reversões. Em todas, o arquivo voltou idêntico (`cmp`) e foi tocado antes de
recompilar.

| commit | reversão | o que reprova |
|---|---|---|
| A | o getter devolve `None` | a conformidade, no primeiro contato |
| A | só `FirstContact` grava a impressão | a conformidade, na volta com `Known` |
| B | volta a comparar por endereço (a porta de loopback) | os dois testes novos: S em `127.0.0.1:8383` passa pelo guardado, e a rede local e o nome deixam de ser ele |
| C | o portão ignora a resposta | a sessão em S, a sala e a ausência de guardado, nos dois gravadores, e o guarda |
| C | `aplicar_mod` grava direto | o guarda, com `["aplicar_mod"]` |
| C | `aplicar_conjunto_de_mods` grava direto | o guarda, com `["aplicar_conjunto_de_mods"]` |
| C | o portão sem `onde_estou_na` | o guarda, e o caso da `Session` de verdade |

**No estado final:**

| o quê | resultado |
|---|---|
| `--bin seele-app` | 307 de 307 |
| `--test frontend` | 288 de 288 |
| `-p seele-ffi --lib` | 180 de 180 |
| `check-runtime` | verde |
| fmt | limpo |
| clippy do app e do `seele-ffi` | limpos |
| clippy da conformidade (o arquivo novo) | limpo |
| conformidade `aceite_dos_mods` | 15 de 15 |
| conformidade `volta_do_aceite` | 2 de 2 |
| conformidade `impressao_da_sessao` | 1 de 1 |

A conformidade rodou um arquivo de cada vez, com `--test-threads=1`. A `ui/`
não mudou nesta etapa, e por isso as bancadas não rodaram.

## A impressão, commit D: a ligação de `onde_estou_na` (`86c00600`)

A re-revisão (Importante) e a F7 acharam o mesmo buraco. Os testes de
`onde_estou_em` são puros, e nada provava o que `onde_estou_na` passa a eles.

**Duas regressões de uma linha passavam verdes**, com 307 no `--bin` e 288 no
`--test frontend`. Reproduzi as duas em `682780eb`:
- **M1:** a impressão do guardado no lugar da impressão da sessão. A
  re-revisão mediu de ponta a ponta que, com ela, o sim é gravado para S;
- **M2:** `Hospedagens::impressoes()` com o par trocado.

**Um teste de ponta a ponta no `main.rs` não cabe:**
- o guarda `o_ponto_de_encontro_de_quem_hospeda` proíbe `Hospedagem::iniciar(`
  no arquivo;
- uma `Connection` comum é recusada por `iniciar_sala`.

A F7 escreve esse teste na sala depois que os ramos se encontrarem.

**O conserto**, em `a_resposta_a_estou_no_meu_servidor`:
- `as_impressoes_saem_na_ordem_do_guardado_e_da_sala`. São duas hospedagens
  `iniciar_sala` com sementes diferentes, como em
  `a_saida_despede_as_hospedagens`, uma no guardado e outra na sala.
  `impressoes()` devolve a `impressao_digital()` do guardado primeiro e a da
  sala depois.
- `onde_estou_na_passa_a_chave_da_conexao_e_as_das_hospedagens`. É um guarda
  de texto-fonte sobre o corpo de `onde_estou_na`, sem espaços e sem linhas de
  comentário, com agulhas de `concat!`. Ele exige:
  - na chamada, `onde_estou_em(endereco, impressao, guardado, pessoal)`;
  - que `impressao` comece em `session.connection()` e passe por
    `.impressao_do_servidor()`;
  - que `(guardado, pessoal)` saia de `.impressoes()`.

**As reversões.** Cada uma reprova exatamente um teste. Em todas, o arquivo
voltou idêntico (`cmp`) e foi tocado antes de recompilar.

| reversão | o que reprova |
|---|---|
| M1 | o guarda |
| M2 | o teste de `impressoes` |
| M1 com a chamada certa posta num comentário logo acima | o guarda |
| a chamada intacta e a ligação trocada (`let impressao = guardado.clone()`) | o guarda |

**O Menor.** O QA-02 só é pego pelo guarda do `frontend.rs`, e fica como está,
por decisão da coordenação.

**Os testes:**

| o quê | resultado |
|---|---|
| `--bin seele-app` | 309 de 309 |
| `--test frontend` | 288 de 288 |
| fmt | limpo |
| clippy do app | limpo |

Nada fora do `main.rs` mudou, e por isso a conformidade não rodou.
