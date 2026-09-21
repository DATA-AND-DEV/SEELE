# As jornadas pendentes: moderação, anexos, admissão, queda e janela pequena

Data: 20/09/2026. Fecha as cinco linhas que a
[validação visual ampliada](validacao-visual-completa-2026-09-20.md) deixou em
branco na matriz de cobertura.

> **Onde isto foi visto.** Numa bancada nova —
> `apps/seele-app/bancada/telas.cjs` — que serve `ui/` como está e injeta um
> `window.__TAURI__` com respostas combinadas. Os mesmos scripts, as mesmas
> folhas, o mesmo `index.html`, a mesma CSP. **Não é o aplicativo**: não há
> Rust do outro lado, não há áudio e não há rede. Ela responde «esta tela, com
> este estado, cabe e é alcançável?», que é o que as cinco linhas pediam, e
> não «o produto faz o que promete».

## O que foi aprovado

| Jornada | O que se viu |
|---|---|
| **Anexos** | Nome, tamanho, SALVAR e PRÉVIA quando o tipo permite; o expirado sai em moldura tracejada com `ESTE ARQUIVO EXPIROU` no lugar das ações. Um nome de 56 caracteres quebra para a linha de baixo em vez de vazar. |
| **Moderação** | A camada abre com a pessoa já escolhida, EXPULSAR e BANIR com a consequência escrita embaixo de cada um, ATÉ QUANDO e MOTIVO entre os dois, e FECHAR ao pé. MOVER não aparece com uma sala só — não há para onde mover, e oferecer seria uma pergunta já respondida. |
| **Queda** | A faixa cobre a conversa sem apagá-la: contagem grande em vermelho, `ATÉ A SESSÃO SER ENCERRADA`, o que acontece com o áudio, e as tentativas. O histórico continua legível atrás, que é o que `specs/07` manda. |
| **Fim de sessão** | Tela própria, `ENLACE ENCERRADO`, a razão em palavra (`DESCONECTADO POR UM OPERADOR`) e uma única saída. |
| **Janela pequena** | Os dois pontos de quebra funcionam como a folha documenta: em 1240 a faixa de pessoas recolhe, em 900 a coluna de canais vira gaveta com porta própria, e a conversa nunca é a primeira a apertar. |

## O que foi corrigido

### A faixa de pessoas recolhia sem porta

Abaixo de 1240px a faixa some. A razão está escrita na folha e continua certa
— ela não é onde se trabalha —, mas a segunda metade da frase não era verdade:
«o que ela mostra não sai da tela com ela». A coluna de salas lista quem está
**em sala**; ela não lista quem está no servidor fora delas, e não mostra o
sinal por pessoa, que `specs/07` chama de elemento assinatura do produto. Nas
duas coisas, a faixa era o único lugar.

Agora há uma porta `PESSOAS` na barra do canal, espelho da de `CANAIS`: mesma
geometria, mesmo tratamento de foco, mesma regra de sumir acima do ponto em
que a coluna já está na tela.

### A portaria pedia a chave antes da pessoa

V12. A camada abre por duas razões e só uma é frequente: alguém bateu. Senha,
convite e ligar a portaria são administração de uma vez por servidor, e
estavam nos três primeiros blocos — a decisão que a pessoa veio tomar ficava
em quarto lugar, abaixo da dobra numa janela baixa. A ordem inverteu; o
conteúdo é o mesmo.

### A saída da moderação nascia abaixo da dobra

A caixa rola, e numa janela de 620px de altura a moderação inteira não cabe:
FECHAR ficava depois de EXPULSAR, BANIR e das duas frases que dizem o que cada
um faz. Sair dependia de rolar até o fim de uma lista de atos irreversíveis.
Ele passou a ficar grudado no pé da caixa, como o rodapé fixo de um diálogo de
MOD — mesma ideia, sem partir a marcação de uma camada que já estava certa.

### Metade da instrução do compositor nunca foi lida

O `placeholder` dizia «transmitir no canal — Enter envia, Shift+Enter quebra o
parágrafo». O campo tem uma linha, e na largura padrão — com a faixa de
pessoas aberta — o texto quebrava **sempre**: a segunda metade era cortada
pela altura do campo. Ela existia e ninguém nunca a viu.

Ficou a metade que precisa ser lida enquanto se digita. `Shift+Enter` continua
escrito onde uma tecla se procura: a tabela de atalhos e a ajuda.

### Um controle que nunca foi um controle

`FORÇAR RECONEXÃO` nasceu desabilitado e nunca teve como deixar de ser: quem
reconecta é o core, e não existe verbo de «tenta agora». A folha já tinha
tirado a moldura dele para que lesse como texto — e o texto continuava sendo
um imperativo, com a explicação só no `title`, que ninguém vê no toque. Ele
passa a dizer o que sabe: **o SEELE já está tentando sozinho.**

## O que fica para depois, e não é anunciado como disponível

- **Miniaturas no compartilhamento.** O seletor de superfície mostra nome e
  tipo, sem imagem. Continua assim.
- **Cancelamento real da conexão.** Não existe verbo; o que existe é a
  contagem e a tentativa automática. A interface deixou de prometer o
  contrário.
- **A abertura do áudio não tem prazo.** `seele_audio::device::open` pode
  bloquear indefinidamente na abertura do CoreAudio — foi o que a cópia QA
  mediu, com a espera em `build_input_stream_raw` e `AudioUnitSetProperty`.
  Um bloqueio sem fim é pior que uma falha nomeada: a tela espera para sempre
  sem dizer o quê. **Não foi mexido nesta rodada**: é caminho de tempo real, e
  não tenho como verificar um prazo sem captura de áudio de verdade.

## O áudio, e por que a cópia QA não capturava

O empacotamento normal está correto, e isto foi conferido no bundle montado:

```
CFBundleIdentifier           = tech.datadev.seele
NSMicrophoneUsageDescription = O SEELE usa o microfone para transmitir a sua voz…
```

e `Entitlements.plist` declara `com.apple.security.device.audio-input`.

A cópia QA foi montada com **outro identificador** —
`tech.datadev.seele.qa.<commit>` — e reassinada *ad hoc*. Para o macOS isso é
um aplicativo que ele nunca viu: o registro de TCC do `tech.datadev.seele` não
acompanha, e a permissão precisa ser concedida de novo para a cópia. É a
explicação mais simples para «a cópia QA estava sem captura de áudio», e ela
não implica defeito no produto.

**O que eu não posso afirmar:** que o áudio funciona. Rodei
`cargo run -p seele-audio --example device_smoke` desta sessão e ele bloqueou
na mesma abertura — mas o shell desta sessão é isolado, e microfone a partir
dele não é teste justo. Homologar voz continua exigindo o aplicativo normal,
numa sessão com a permissão concedida, e uma pessoa ouvindo.

## Verificação técnica

- `cargo fmt --all -- --check`: limpo. As diferenças registradas como pendência
  em `ajustes-visuais-mods-2026-09-20.md` foram aplicadas.
- `cargo xtask check-api`, `check-deps`, `check-versao`: passam.
- `cargo clippy --all-targets`: sem aviso.
- `cargo test --workspace` e `cargo xtask check-runtime`: ver o fim deste
  documento, atualizado na última execução.
