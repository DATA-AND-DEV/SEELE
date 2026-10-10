# Lote W2 da F5: relatório

Base `d9124cea` (branch `0.16/lotes`). Branch `0.16/lotes-w2`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a4c73fc301003acb3`. Tudo
local: sem push, tag nem release. Sem conformidade.

> Rascunho escrito antes do conserto, com a medida. As seções do conserto, da
> frase nova e das reversões entram à medida que cada passo fica verde.

## 1. A medida do caminho

### A reprodução

Uma cena nova na `ajustes-v013.cjs`, `aTentativaSubstituidaNaoFala`, com um
servidor de mentira que responde por alvo, com atraso por alvo. A tentativa
velha vai a `192.0.2.17:8384` com um convite, leva 600 ms e volta
`ModsNaoAceitos`; aos 100 ms, a nova a substitui. Medido na base (`d9124cea`),
com uma cópia temporária da bancada em que as asserções escreviam em vez de
parar (apagada logo depois):

1. **O `hospedar()` substitui a velha** (o caso do achado). O `hospedar` entra
   no servidor daqui; a pessoa desconecta e volta à entrada, e lê na linha
   vermelha, sobre um servidor que ela deixou:

   ```
   ESTE SERVIDOR EXIGE MODS QUE VOCÊ AINDA NÃO ACEITOU.
   • fulano/exemplo 1.0.0
     repositório: https://github.com/seele/exemplo
     alcança: ler
   A tela para ler e aceitar esta lista ainda não existe neste app.
   ```

   Nada no `seele.log` vindo da janela (o Rust escreve «could not reach the
   server» para toda tentativa que falha, e nada diz que a tela não a mostrou).
2. **Outra escolha na lista substitui a velha**, e a nova é lenta (1,4 s). Com
   a nova ainda andando, a mesma frase aparece na linha vermelha da entrada,
   **visível**, até a nova voltar e escrever por cima. Enquanto isso, o
   CONECTAR, que a nova apagou, está aceso: o `finally` da velha o reacende.
3. **A nova parou na espera da portaria** (`AdmissionPending`), e a velha volta
   pelos MODs: `levarParaAEspera` trata «qualquer falha com a espera na
   frente», e a frase dos MODs toma o lugar da frase da espera, e o botão passa
   de AGUARDANDO PERMISSÃO a TENTANDO DE NOVO.
4. **Não só pelos MODs.** A velha recusada pela portaria (`AdmissionPending`)
   depois que o `hospedar` entrou abre a espera **por cima da sessão**, e a
   espera passa a bater no servidor desta máquina (o `ultimoAlvo` é o dele).

Ou seja: a regra «só a tentativa mais nova abre a pergunta» do lote W fechou
uma porta, e a recusa da velha caiu nas outras duas — a linha vermelha e a
espera —, e a da linha vermelha usa a única frase do `fraseDeErro` que ainda
diz que a tela de aceite não existe.

### Todo caminho que leva à frase do `ModsNaoAceitos`

`ModsNaoAceitos` só nasce num lugar: o `connect` da FFI
(`classify_connect_failure`, `crates/seele-ffi/src/lib.rs`), devolvido pelo
comando `connect` do `main.rs`. Na janela há **um** `invoke("connect"`
(`tela-boot.js`, em `conectar`), e `conectar` nunca relança: o `catch` dele
decide tudo. Então a frase só é alcançável por quem recebe o erro dentro desse
`catch`, ou por quem chama `fraseDeErro` com um valor que veio dele.

| caminho | veredito | como |
|---|---|---|
| `conectar`, tentativa velha, espera fora da frente → `#boot-erro` | **alcançável** | passos 1 e 2 acima |
| `conectar`, tentativa velha, espera na frente → `levarParaAEspera` → `fraseDeErro` em `tela-auth.js` (`#auth-espera-frase` e o registro da tela) | **alcançável** | passo 3 acima |
| `conectar`, tentativa velha, espera largada com batida no ar (`tela.hidden && batendoDaEspera`) | não | `levarParaAEspera` devolve `true` e cala |
| `conectar`, tentativa mais nova | **não** | `levarParaOAceiteDeMods` só devolve `false` quando `motivo?.ModsNaoAceitos` é falso; com ele, abre a pergunta e devolve `true`, sempre — inclusive com a espera na frente, porque a pergunta vem antes da espera |
| a volta do aceite (`aceitarOsMods` → `conectar(alvo, null, credencial)`) | não | é uma tentativa nova: um `ModsNaoAceitos` nela (o conjunto mudou entre a pergunta e a volta) reabre a pergunta |
| `camada-mods.js`: `simQueFicouGravado`, o `catch` de `aceitar_mods`, `obterOConjunto` (`mods_instalados`, `instalar_mod_do_catalogo`) | não | as falhas são de `FalhaNoMod` e do catálogo, nunca do `connect` |
| `tela-fim.js` (`reconectar`) | não | chama `conectar()`, e cai na primeira linha; `tela-fim.js` não chama `fraseDeErro` |
| `tela-auth.js`, a batida da espera (`baterDeNovo`) | não | chama `conectar()`: é uma tentativa nova |
| `tela-boot.js`: `hospedar` (e `hospedarGuardado`, que passa por ele), `cumprirAAbertura`, `abrirVersao`, o `apagar_servidor`, o alcance, a porta no firewall, o ícone do preparar | não | os `catch` deles recebem o erro de `hospedar`, `analisar_convite`, `abrir_versao`, `apagar_servidor`, `abrir_a_porta_no_firewall` e `preparar_icone_do_server`; o `conectar()` que os dois primeiros chamam não relança |
| `camada-servidores.js` (`irParaOServidor`, o `analisar_convite` do campo) e `tela-sessao.js` (`trocarDeServidor`) | não | chamam `conectar`, que não relança; o `fraseDeErro` deles é do `analisar_convite` |
| `base.js` (`atenderOMod`), `mods-regiao.js`, `camada-*.js` restantes, `tela-server.js`, `tela-sessao.js` (prévias) | não | comandos que não devolvem `ConnectionError::ModsNaoAceitos` |
| a ponte do navegador (`ponte-web.js`) | não | o `connect` dela é recusado por tarefa (`TAREFA_34`) |

Ou seja: os dois caminhos alcançáveis são **da tentativa velha**, e nenhum
caminho da tentativa mais nova alcança a frase.

## 5. A v0.15.1 publicada

Conferido pelo `CLAUDE.md` (o curl dos releases):

```
v0.15.1 · 54f542f94 · 2026-10-08T04:14:07Z
```

O commit é o que a orquestradora cita (`54f542f`). Lido só com
`git show 54f542f94:…`; nada mudou na linha 0.15.

- A frase está lá: `apps/seele-app/ui/frases.js:505`, idêntica.
- A tela de aceite existia: `camada-mods.js` tem `abrirAceiteDeMods` e
  `levarParaOAceiteDeMods(motivo, alvo)`.
- Tentativas que se substituem existiam **mecanicamente** (o `hospedar` e a
  lista de servidores não esperam o `connect` em curso), mas **não havia
  contador**: o `catch` de `conectar` era
  `if (levarParaOAceiteDeMods(motivo, ultimoAlvo ?? "")) … else if (!levarParaAEspera(…)) …`,
  sem condição.
- Por isso **a frase não é alcançável na v0.15.1**: todo `ModsNaoAceitos`,
  da tentativa velha ou da nova, abre a pergunta antes de qualquer outra
  coisa, e o único `invoke("connect"` da janela é esse. A corrida existe lá
  com outro sintoma — a pergunta da velha abre por cima da sessão, e com o
  `ultimoAlvo` (o servidor novo), que é o que o lote R (o alvo da chamada) e
  o lote W (`tentativas`) consertaram na 0.16.
- A frase passou a ser alcançável em `8effec98` (09/10, lote W, re-revisão da
  rodada 1, sonda P6), que pôs `esta === tentativas &&` antes de
  `levarParaOAceiteDeMods` e deixou o resto da cadeia como estava.
