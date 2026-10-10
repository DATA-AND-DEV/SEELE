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
2. **A mesma confusão de endereço continua no QA-01.** Esta janela pode estar
   na sessão de S com o servidor daqui de pé, pelos caminhos da seção 1. Se a
   gestão atualizar um MOD exigido aqui nessa hora, `aplicar_mod` grava o sim
   de S com a identidade do servidor daqui (o efeito 2). É o QA-01, que este
   lote não podia tocar. Fica para a orquestradora.
3. **O teste de Rust replica duas linhas do comando.** São a leitura de
   `mod_exigido_aqui` e a gravação de `aplicar_mod`, porque o comando tem
   `AppHandle` e rede. O que amarra a réplica ao comando é o guarda de
   texto-fonte `o_comando_reaplica_pela_decisao_que_le_o_uso`.
