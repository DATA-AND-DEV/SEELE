# A 0.15.1: o que está feito, o que falta e o que é seu

Levantamento e execução de 07/10/2026, com commits locais e **sem push**. O
branch da versão é `versao/0.15.1`, e a `main` local avança para ele quando a
bateria passa.

## O que está publicado

A **v0.15.0 já saiu**, em 23/09, do commit `e2fac4d` (consulta do CLAUDE.md a
SEELE-RELEASES). A próxima é a **0.15.1**, que conversa com a 0.15.0. O
protocolo não muda: `PROTOCOL_VERSION` 8, `SEELE-ENC/1`, API de MODs 5, iguais
nas duas. A 0.16 fica reservada para a quebra de protocolo do Plano 3.

## Decisões do dono, em 07/10

1. **O número: 0.15.1.**
2. **No celular, o SEELE é um web app** (ADR 0057, aceito). O Tauri iOS deixa de
   ser caminho de produto, e o branch `mobile/ios` sai. O que serve ao web app
   e não entrou no `main` fica na etiqueta `arquivo/mobile-ios`: o
   `celular.css`, a casca de uma mão em `ui/`, o `tools/auditoria-celular.py` e
   o ícone do celular.
3. **Da `abertos-da-1.0`, a onda 1 entra na 0.15.1**: A-S1, A2, B, K1 e V,
   mais o L1 revisado e o L2. Os outros 15 lotes vão para a 0.15.2.
4. **No G1, só ganha cadeia uma correção do que ele confere.** O S1, as
   pendências, os consertos de desktop e a Task 9 não ganham. A regra está
   escrita no plano 1B.
5. **O G2 roda.** O link passa a escrever a porta do ponto de encontro (a Task 9
   do 1A, `a6c2cc0`). O preço: um cliente 0.15.0 passa a consultar o quarto e,
   na volta pela lista, conecta sem conferir a chave. As notas pedem que quem
   volta pela lista atualize.
6. **A medição do som de MOD segura a 0.15.1, nas duas metades**: o WKWebView
   no Mac e o WebView2 no Windows (pendência 49, itens 1 a 3).

## O que a 0.15.1 leva

- **O Plano 1 da 1.0**: o quarto, a impressão conferida dentro do TLS, os MODs
  no `seele.log`, a aba DIAGNÓSTICO e o modo de desenvolvedor, o som de MOD por
  WebAudio e a API congelada.
- **A onda 1 dos abertos da 1.0**:
  - o **S1**: salvar anexo grava na pasta de Downloads, nunca substitui nada e
    recusa nome perigoso;
  - a portaria que fecha quando o banco falha;
  - o registro da janela;
  - os instaladores que baixam de SEELE-RELEASES;
  - o classificador e a linha da bateria no corpo do release;
  - os jobs `deny` e `fuzz-curto`;
  - o L2: o `seeled` imprime o link para colar, e nenhum texto manda rodar o
    `connection` nem promete que quem hospeda não ouve.
- **Os quatro consertos de desktop achados no trabalho de iOS**:
  - a troca de microfone que somava quase um segundo de atraso;
  - o jitter, que se defende de remetentes 0.15.0;
  - o tema de MOD que vazava para o servidor seguinte;
  - o SAIR DA SALA.
- **A Task 9 do 1A** (o G2).
- **Consertos achados no caminho**:
  - o `install.sh` deixa de culpar a versão quando falta rede;
  - um roteiro da casca para o SAIR DA SALA;
  - o README com o rótulo certo do macOS, «Abrir Mesmo Assim».
- **Documentos**: o ADR 0057 com os spikes e as pesquisas que ele cita, o plano
  dos lotes dos abertos e as notas em `empacotar/notas/0.15.1.md`.
- **A limpeza do repositório** (`16f1df2`): saem 214 arquivos de relatórios,
  notas, planos e evidências das versões anteriores que nenhum código, teste ou
  documento vivo cita. O git os guarda. O README foi conferido contra o código:
  a contagem de testes medida, a tabela do repositório completa e o celular
  pelo navegador.

Cada guarda novo foi provado por reversão. O do jitter, com o conserto desfeito,
reproduz o relato de 05/10: 20 → 920 → 1820 → 2720 ms. As duas receitas do G1
conferem, e a bateria inteira roda no commit que vai para a `main`.

## O que falta antes de publicar, e é seu

1. ~~A medida de campo do G1~~: **feita em 08/10**, do PC (cliente) para o Mac
   (anfitrião atrás de CGNAT). As quatro linhas estão em `docs/m1-medicoes.md`,
   «G1», e o resumo nas notas da 0.15.1. A do DNS mudo custou 2,1 s, e não
   1,5 s: o `LEVE` resolve o ponto de novo (pendência 55, explicada, não segura
   a versão). Desvio do plano: as duas máquinas estavam na mesma rede de casa.
2. ~~A medição do som de MOD~~, no WKWebView e no WebView2: **feita em 07 e
   08/10, e toca nas duas** (pendência 49 e
   `docs/evidencias/som-de-mod-wkwebview/registro.md`). Ela achou três coisas
   fora do som de MOD, que não seguram a versão: a voz segura o aparelho de
   som a sessão inteira (pendência 52), a recusa de um MOD com atalho sem
   frase (53) e a linha do padrão do aparelho de saída (54).
3. **`git push origin main`.** O `curl` do `install.sh` serve a `main` remota, e
   o CI precisa do ramo no GitHub.
4. **Disparar o `ci.yml`**, job `bancadas`, no ramo que vai sair. Ele roda só
   por `workflow_dispatch`, e os últimos runs são de 22/09.
5. **`empacotar/publicar.sh 0.15.1`**, com a bateria, inclusive a do Windows.
   É lá que o S1 grava no NTFS de verdade.
6. Opcional: corrigir o corpo da página da v0.15.0, que diz «não traz nenhuma
   mudança de produto».

## Depois da 0.15.1

1. **A `abertos-da-1.0` retoma as ondas 2 a 9 sobre o `main`**, a partir do
   lote C. Com o iOS fora, a casca continua em `main.rs`, e o conflito de
   `main.rs` com `lib.rs` deixa de existir. Entram:
   - a pendência 50 (quem sai some do roster em menos de 2 s);
   - o fechar enquanto hospeda, que pergunta antes;
   - o áudio que não abre e não prende a conexão;
   - os rascunhos e os volumes que vazam entre servidores.

   Isso é a 0.15.2.
2. **Os minors 2 a 4 da revisão do L1**, num lote futuro:
   - o aviso dos assuntos sem classe sai tarde no `publicar.sh`;
   - o fuzz sem `--locked` e sem guardar o arquivo do crash;
   - o `sysctl` fora do PATH.
3. **O web app do celular**, pelo ADR 0057. Ele precisa de WebTransport no
   `seeled`, no mesmo socket, separado pelo ALPN, e do certificado curto. Parte
   disso é mudança de protocolo: uma variante nova de `DisconnectReason` e a
   mensagem dos hashes, que o ADR põe na mesma subida de versão.
4. **O repasse do desafio**: um servidor malicioso entrar noutro com a
   assinatura de quem o visita. O teste que o demonstra ficou pela metade no
   worktree `elastic-curie-f8e98a`. O conserto é o S2 do Plano 3: a prova
   amarrada ao canal, na 0.16.

## Fora da release

- **Monetização.** São 13 decisões de negócio, em
  `docs/monetizacao-2026-10-04.md`, que segue fora do git. O ADR 0057 o cita.
  A única com relógio é a licença do repositório, que deve sair antes da
  primeira contribuição de fora.
- **A sonda `medida_do_disconnect.rs`**, no worktree `eloquent-blackburn`, deve
  rodar uma vez, gravar os números na pendência 50 e ser apagada.
- **As branches `orbita/*` que não estão contidas na `main`.** A `4790ac20`
  traz um guarda de citações de ADR que não está na `main` (resgatar ou não);
  a `858903f7` tem uma mutação temporária, e não serve.
