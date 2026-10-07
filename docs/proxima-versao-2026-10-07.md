# A 0.15.1: o que está feito, o que falta e o que é seu

Levantamento e execução de 07/10/2026. O levantamento leu o git, os worktrees e
as conversas das sessões paralelas. A execução fez os passos 0 e 1 abaixo, com
commits locais e **sem push**.

## O que está publicado

A **v0.15.0 já saiu**, em 23/09, do commit `e2fac4d` (consulta do CLAUDE.md a
SEELE-RELEASES). Pelo índice da 1.0, o Plano 1 sai como «0.15.x, que conversa
com a 0.15.0», e a 0.16 fica reservada para a quebra de protocolo do Plano 3. A
próxima é a **0.15.1**. O protocolo não muda: `PROTOCOL_VERSION` 8,
`SEELE-ENC/1`, API de MODs 5, iguais na v0.15.0 e aqui.

## Decisões tomadas em 07/10

1. **O número: 0.15.1.**
2. **O iOS fica fora da 0.15.1** e entra logo depois. Os quatro consertos de
   desktop achados no trabalho dele entram na 0.15.1.
3. **O Team ID sai do `project.pbxproj`.** O Tauri o lê de
   `APPLE_DEVELOPMENT_TEAM` e o regrava no arquivo a cada build de aparelho;
   essa linha não deve ser commitada.
4. **Da `abertos-da-1.0`, a onda 1 entra na 0.15.1**: A-S1, A2, B, K1 e V,
   mais o L1 revisado e o L2. Os outros 15 lotes vão para a 0.15.2.

## O que foi feito

### Passo 0: o que só existia no disco agora está no git

| O quê | Onde | Commit |
|---|---|---|
| Os quatro consertos de desktop do trabalho de iOS | `mobile/ios` e `conserto/desktop-do-ios` | `266e6a9`, `67b5f85`, `1f04382`, `67d9069` |
| O port de iOS, sem o Team ID | `mobile/ios` | `8632668` |
| O dublê do Tauri e os roteiros da casca | `claude/nostalgic-jang-59e448` | `73bf86e` |
| O plano de lotes dos abertos da 1.0, que o `.gitignore` escondia | `conserto/abertos-da-1.0`, em `docs/superpowers/sdd/2026-10-04-abertos/` | `2c2fed8` |

`docs/monetizacao-2026-10-04.md` e `design/video-hero/` ficaram de fora de
propósito. O primeiro é estratégia de negócio num repositório público. O
segundo é da sessão do vídeo.

### Passo 1: a 0.15.1 montada no branch `versao/0.15.1`

Por cima da `main` local (Plano 1 + README), por merge sem squash:

1. a onda 1 da `abertos-da-1.0`: o S1, a portaria que fecha, o registro da
   janela, a captura parada, o classificador, os jobs `deny` e `fuzz-curto` e os
   instaladores;
2. o dublê dos roteiros;
3. os quatro consertos de desktop;
4. o L2: o `seeled` imprime o link para colar, e nenhum texto publicado manda
   rodar o `connection` nem promete que quem hospeda não ouve.

E, em commits próprios:

- a revisão do L1 aprovou com minors. O minor 1 está consertado: sem rede, o
  `install.sh` não culpa mais a versão nem manda baixar sem conferir;
- o `cargo fmt` que o conserto do `voice.rs` não tinha rodado;
- um roteiro que prende o SAIR DA SALA, que era o único conserto de desktop sem
  guarda;
- as notas, em `empacotar/notas/0.15.1.md`;
- o registro na pendência 51 e no índice.

**Provas.** Cada guarda novo foi provado por reversão: desfeito o conserto, ele
fica vermelho. O do jitter, com o conserto desfeito, reproduz o relato de 05/10:
20 → 920 → 1820 → 2720 ms. A bateria inteira roda no commit final, e a receita
do G1 imprime «as catorze cadeias estão na release, e nenhuma foi revertida».

## O que falta, e é seu

### Três decisões

5. **A regra do G1**: uma correção de fora do Plano 1 ganha cadeia? A sessão
   do README fez a mesma pergunta. O plano de lotes propõe que não. Pela letra
   de hoje, cada lote que muda código ou frase pública ganharia uma. A receita
   passa nas duas leituras, porque confere as 14 cadeias do Plano 1, e todas
   estão dentro.
6. **O G2** (a Task 9 do 1A, a porta no `enc=`): registrar «não rodar», como o
   plano recomenda?
7. **A medição do som de MOD no WKWebView e no WebView2** (Tasks 1 e 9 do 1D,
   pendência 49): ela segura a 0.15.1? A decisão tem de ficar escrita na
   pendência 49.

### As ações

1. **A medida de campo do G1**, que é obrigatória. Está no plano 1B, em «A
   medida de campo». Ela pede duas máquinas em redes diferentes, o ponto do
   bilhete mudo do lado do cliente, o DNS num buraco negro e uma captura. As
   quatro linhas de resultado vão no bloco «[PREENCHER ANTES DE PUBLICAR…]» de
   `empacotar/notas/0.15.1.md`, num commit, antes do `publicar.sh`.
2. **Trazer a `main` para a 0.15.1 e empurrar.** A `main` local já foi avançada
   para o `versao/0.15.1`. Falta o `git push origin main`: o `curl` do
   `install.sh` serve a `main` remota, e o CI precisa do ramo no GitHub.
3. **Disparar o `ci.yml`**, job `bancadas`, no ramo que vai sair. Ele roda só
   por `workflow_dispatch`, e os últimos runs são de 22/09.
4. **`empacotar/publicar.sh 0.15.1`**, com a bateria, inclusive a do Windows. É
   lá que o S1 grava no NTFS de verdade.
5. **Reinstalar o app no iPhone antes de 12/10 às 13:06.** Não depende da
   release. Rode `APPLE_DEVELOPMENT_TEAM=<seu Team ID> sh tools/ios-aparelho.sh`.
6. Opcional: corrigir o corpo da página da v0.15.0, que diz «não traz nenhuma
   mudança de produto».

## Depois da 0.15.1

1. **O iOS entra na `main`.** O merge de `mobile/ios` dá conflito em
   `apps/seele-app/src/main.rs`, e é esperado. O iOS recria um `main.rs` de 10
   linhas, e o git não enxerga a troca de nome para `lib.rs`. A resolução:
   - manter o `main.rs` de 10 linhas;
   - levar para o `lib.rs` os 4 trechos do `bc734f4` (o `git merge-file` os
     aplica sem conflito);
   - trocar o `read("src/main.rs")` dos testes novos do `frontend.rs` por
     `src/lib.rs`.

   Antes de entrar, o commit do iOS precisa de clippy, `cargo test` e dos builds
   de Windows e Linux. Nada disso rodou nele.
2. **A `abertos-da-1.0` retoma as ondas 2 a 9 sobre esse `main`.** Elas começam
   pelo lote C. Entram:
   - a pendência 50 (quem sai some do roster em menos de 2 s);
   - o fechar enquanto hospeda, que pergunta antes;
   - o áudio que não abre e não prende a conexão;
   - os rascunhos e os volumes que vazam entre servidores.

   Isso é a 0.15.2.
3. **Os minors 2 a 4 da revisão do L1**, para um lote futuro:
   - o aviso dos assuntos sem classe sai tarde demais no `publicar.sh`;
   - o fuzz sem `--locked` e sem guardar o arquivo do crash;
   - o `sysctl` fora do PATH.
4. **0.16.** Os Planos 2 e 3, com a última quebra de protocolo.

## Fora da release

- **Monetização.** São 13 decisões de negócio. A única com relógio é a licença
  do repositório, que deve sair antes da primeira contribuição de fora. Ela
  também decide se a SignPath assina o Windows de graça.
- **iOS.**
  - Falta o ADR «o celular hospeda?»: a spec 06 e o M6 dizem que não, e o app de
    hoje hospeda.
  - Falta decidir a conta paga, de R$ 549,90 por ano.

## Limpeza (com o seu ok)

- Os 7 worktrees do Órbita e as branches `orbita/*` já estão contidos na
  `main`. Exceções:
  - `858903f7` tem uma mutação temporária, e não serve;
  - `4790ac20` traz um guarda de citações de ADR que não está na `main`
    (resgatar ou não).
- A sonda `medida_do_disconnect.rs`, no worktree `eloquent-blackburn`, deve rodar
  uma vez, gravar os números na pendência 50 e ser apagada.
- Depois do push, podem sair os worktrees `1.0-plano-1`, `nostalgic-jang`,
  `dazzling-einstein` e `l2-textos-publicos`. Os commits deles já estão na
  `main`.
