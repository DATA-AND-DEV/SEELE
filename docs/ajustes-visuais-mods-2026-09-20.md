# Ajustes visuais dos MODs — 20/09/2026

Implementação local pelo Codex, autorizada pelo usuário, a partir de SEELE
`08b3788`, PERFIS `2927986`, ESTILO `f4c8614` e MESA `634fd92`.
Não altera a versão da API nem constitui publicação de pacotes.

## Composição

- Distintivos e interruptores do renderer passam a ter geometria quadrada.
  O interruptor diferencia ligado/desligado pela posição e pela cor, mantém
  semântica de switch e indicação de foco.
- PERFIS usa retratos quadrados e cartões sem arredondamento imposto.
  ESTILO usa amostras e contêineres quadrados; MESA usa cartões de ficha,
  compêndio e registro coerentes com a composição do SEELE.
- A API continua permitindo retratos circulares e arredondamentos declarados.
  Os conjuntos de aparência do ESTILO conservam seus raios escolhidos pelo
  usuário, e peças de jogo não foram convertidas em controles quadrados.
- O MESA distribui comandos pela aba correspondente: ficha em Fichas,
  conteúdo em Compêndio, cena e dados em Tabuleiro. O cabeçalho usa o nome
  legível do sistema e do mestre, sem expor o contador de revisão.
- O cartão que substitui uma pessoa recebe teto de 180px: o limite de 96px
  do cartão complementar antigo cortava retrato e status. A lista continua
  com sua própria rolagem; conteúdo extenso continua acessível no perfil.

## Defeitos encontrados durante a validação

### Digitação e atualização

A chave implícita de um nó dependia do orçamento restante da árvore. Quando
uma prévia ganhava conteúdo, o renderer recriava ancestrais do campo editado,
perdendo foco. A posição estrutural agora identifica nós sem chave explícita;
as chaves explícitas continuam permitindo reordenação estável.

Digitar rapidamente no formulário de campanha também produziu
`too-many-requests` no laboratório real. Os três pacotes agora agrupam
repinturas: uma atualização em voo e o estado mais recente pendente. A
atualização do MESA propaga sua promessa e apresenta erros ao usuário.

### Rodapé e teclado

Mover ações fixas para fora da árvore após reconciliar fazia o próximo
redesenho recriar botões e reter recursos. Corpo e rodapé são agora dois
destinos do mesmo renderer, com a mesma contabilidade e descarte. A extração
respeita a aba ativa. Cem repinturas preservam o botão e um único recurso.

No WKWebView, clicar em Gravar pode deixar o foco no documento. Um ouvinte
apenas na raiz do diálogo não recebia Escape. A camada modal do topo agora
acompanha o teclado no documento, respeita eventos já tratados e cede à
confirmação do produto. Tab recupera o foco quando ele está fora da camada;
o ouvinte é removido na saída. O caso foi reproduzido e retestado nativamente.

### Confirmação fora da sessão

A confirmação do produto ficava dentro de `#tela-sessao`, que está oculta
quando Configurações é aberta pela entrada. Salvar o conjunto de MODs parecia
não fazer nada. `#moderar` agora pertence ao body e continua sendo um overlay.
A confirmação apareceu e salvou o conjunto na janela nativa sem sessão.

## Validação executada

Laboratórios dos três pacotes em navegador real, usando o renderer do produto,
e os três pacotes no aplicativo nativo macOS, com servidor hospedado pelo app:

| Jornada | Resultado observado |
| --- | --- |
| PERFIS: digitar, prévia, gravar, diretório e reabrir | Texto preservado e perfil gravado |
| PERFIS: Escape imediatamente após clicar em Gravar | Diálogo fechado na compilação corrigida |
| ESTILO: abrir cores e conjuntos, escolher PROFUNDO | Editor aberto e prévia atualizada |
| ESTILO: publicar e restaurar padrão | Ambas as operações confirmadas pelo MOD |
| MESA: digitar nome e criar campanha | Campanha Visual QA criada; diálogo fecha e mesa abre |
| MESA: aba Fichas, criar e abrir ficha | Personagem QA criada e editor aberto |
| Sair do servidor com os MODs ativos | Retorno à entrada, sem navegação, cartão ou superfície de MOD |

Comandos concluídos:

- `cargo test -p seele-app --test frontend`: 237 testes passaram.
- `cargo xtask check-runtime`: quatro bancadas passaram; região com 16 provas.
- `cargo clippy -p seele-app --all-targets -- -D warnings`: passou.
- `npm run build`, `npm run check` e `npm test` nos três pacotes:
  39 testes no PERFIS, 23 no ESTILO e 48 no MESA.
- `cargo run --quiet --manifest-path ferramentas/quickjs-check/Cargo.toml`
  nos três pacotes: cliente no prelúdio real e verificações de servidor passaram.
- Compilações release do aplicativo concluídas.

As reproduções de chave instável, rodapé recriado e Escape sem foco reprovam
quando os respectivos consertos são retirados na bancada. Os testes dos
pacotes verificam rajada de 200 atualizações, concorrência máxima de uma,
preservação do último estado e recuperação após rejeição.

`cargo fmt --all --check` encontrou diferenças anteriores em arquivos Rust
fora do escopo desta mudança. Elas não foram reformatadas nesta rodada.
Não foi executada novamente a suíte Rust inteira do workspace.

## Ambiente e limites

Aplicativo: `target/qa-08b3788/SEELE QA.app`, identificador próprio
`tech.datadev.seele.qa.08b3788`, usando
`SEELE_HOME=/tmp/SEELE-QA-08b3788/home`. A instalação de uso e seus dados não
foram substituídos. Os dados fictícios permanecem nessa casa para reprodução.

A entrada com áudio ficou bloqueada na abertura do dispositivo CoreAudio.
A amostra de processo está em
`/tmp/SEELE-QA-08b3788/amostra-entrada.txt`, com a espera em
`seele_audio::device::open`, `build_input_stream_raw` e `AudioUnitSetProperty`.

Para concluir a validação visual, a cópia de QA foi compilada temporariamente
com `audio: false` no pedido de conexão. O arquivo foi restaurado byte a byte;
o código de produção conserva `audio: true`, e o binário release normal foi
recompilado depois. A cópia SEELE QA permanece exclusivamente visual e sem
áudio. Esta rodada não homologa voz, mídia sonora, consumo de memória,
Windows ou Linux, nem todas as funcionalidades dos três MODs.

A última compilação incluiu o teto de 180px do cartão lateral. O Mac foi
bloqueado antes da captura final: o corte foi observado antes do ajuste,
mas a aparência desse último ajuste não foi recapturada nativamente. As
demais jornadas da tabela foram observadas antes desse ajuste exclusivo de CSS.
Os laboratórios temporários e a cópia QA iniciados nesta rodada foram encerrados.

Não houve push, criação de release ou publicação de pacotes.

## Reteste visual ampliado

Na [rodada de todas as telas](validacao-visual-completa-2026-09-20.md), o cartão
curto com o teto de 180px foi visto completo. O teste com nome, status e
biografia longos revelou recortes adicionais (V13); portanto essa altura não
encerra a correção do cartão. O rodapé do editor permaneceu visível com o
corpo rolável. A navegação entre páginas de MODs revelou outro defeito do
host (V14), que os fluxos individuais anteriores não tinham exercitado.
