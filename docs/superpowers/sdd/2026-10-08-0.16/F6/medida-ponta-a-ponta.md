# F6 — a medida de ponta a ponta (tarefa 17)

**Estado em 08/10/2026:** o roteiro está escrito, e nada foi medido ainda.

A medida tem duas partes:
- **A parte sem tela**: o pacote **publicado** da 0.15.1 passando pelo código
  novo. Ela pode rodar nesta máquina, mas precisa de permissão para baixar o
  pacote (19.846.037 bytes).
- **A parte com tela**: precisa do dono, no Mac e no Windows. Rodar apps de
  verdade na área de trabalho dele tem três riscos concretos:
  - a porta 8383 do SEELE real pode estar em uso;
  - com o identificador `tech.datadev.seele`, o armazenamento do WebView seria
    o dele;
  - o macOS pede microfone e firewall, e esses pedidos são do sistema.

## O que está publicado (conferido pela API em 08/10, como pede o `CLAUDE.md`)

| release | commit | publicado |
|---|---|---|
| v0.15.1 | `54f542f94` | 2026-10-08T04:14:07Z |
| v0.15.0 | `e2fac4dab` | 2026-09-23T00:12:13Z |

Anexos da v0.15.1 que importam aqui:
- `SEELE_0.15.1_aarch64-apple-darwin.app.tar.gz` (19.846.037 bytes) e o
  `.sig` ao lado;
- `SEELE_0.15.1_x64-instalador.exe` (15.185.408 bytes) e o `.sig`;
- o `latest.json`, de uma versão só e sem `schema` — é o formato que o
  `manifesto.py` da 0.16 diz que «não é acumulado»;
- o `SHA256SUMS`.

Não há portátil do Windows na 0.15.1, e por isso existe a 0.15.2.

## Parte 1 — sem tela (aqui, depois da permissão)

1. Baixar o `.app.tar.gz`, o `.sig` e o `SHA256SUMS` da v0.15.1 para o
   scratchpad, numa pasta própria.
2. Conferir:
   - a soma contra o `SHA256SUMS`;
   - a assinatura com a chave do `tauri.conf.json`, pelo `seele_lancador::Chave`
     (o mesmo `minisign-verify` do atualizador).
3. Montar um manifesto de schema 2 com a entrada da 0.15.1:
   - `executable` = `SEELE.app/Contents/MacOS/seele-app`;
   - `server` = `0.15`.

   Depois, instalar pelo `Deposito::instalar` com o `DesempacotarTarGz` da
   casca, num depósito temporário.
4. Afirmar:
   - a instalação foi publicada;
   - o marcador `INSTALADA` tem 4 linhas;
   - o executável declarado existe dentro do `.app`;
   - o `Resolvedor` resolve `MaisNovaDaLinha(0.15)` com `SEELE_HOME`
     apontando para `dados/servidor-0.15`.
5. Medir o custo da conferência da soma do executável a cada abertura (tarefa
   11): o tempo de ler o executável real e tirar o SHA-256.

## Parte 2 — com tela (o dono, no Mac e no Windows)

Artefatos de teste, sem publicar nada:
- uma chave de teste gerada para a medida;
- builds com o identificador trocado (`tech.datadev.seele.medida-f6`), para não
  tocar no WebView real;
- o endpoint do atualizador apontando para `http://127.0.0.1:8765/latest.json`;
- `SEELE_HOME` temporário.

| # | o que | esperado |
|---|---|---|
| 1 | a 0.15.1-teste, com servidor hospedado e conversa escrita, atualiza pelo botão | a 0.16.0-teste abre |
| 2 | a primeira abertura da 0.16 | `dados/servidor-0.15` com `PRONTA`, o `seele.db` igual ao de antes (pela soma), e a nota de uma vez na tela |
| 3 | a linha legada é baixada sozinha | `lancador/versoes/0.15.2-teste/INSTALADA` com 4 linhas |
| 4 | hospedar no Servidor 0.15 | a 0.15.2 abre hospedando, **com a mesma impressão digital de antes da atualização** |
| 5 | na 0.15.2 aberta pelo launcher | a seção de atualização não aparece, e o comando recusa |
| 6 | entrar, pela 0.16, num link `v=0.15.1` do servidor do passo 4 | o diálogo com a frase das conversas, e depois a 0.15.2 entrando |
| 7 | as duas abertas | o que acontece com a porta 8383, o microfone e o armazenamento do WebView (as 2 preferências de MOD de `camada-mods.js`) |
| 8 | a guarda | trocar o executável da 0.15.2 no depósito e abrir: `InstalacaoAlterada` na tela |
| 9 | Windows: o portátil, o firewall por caminho, o `/UPDATE` com a 0.15.2 aberta, e remover a 0.15.2 com ela aberta | o que acontecer, como está |

Os resultados entram aqui, com os números e o que não deu certo, do jeito que
aconteceu.
