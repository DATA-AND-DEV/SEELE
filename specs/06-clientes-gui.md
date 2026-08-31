# 06 — Clientes gráficos

## Posição no projeto

Os clientes gráficos existem para alcançar quem não vive no terminal e para dar mobilidade. Eles **imitam a TUI**, não o contrário. Nenhuma funcionalidade nasce aqui: se algo é útil, é implementado em `seele-core` e aparece nas duas interfaces.

## Desktop — Tauri

**Decisão:** Tauri, não Electron.

- Usa a webview do sistema; binário na casa de dezenas de MB, não centenas.
- O núcleo Rust roda no processo nativo, não em JS. Áudio, QUIC e estado ficam onde já estão.
- O frontend é apenas apresentação, comunicando por comandos e eventos Tauri, espelhando o contrato de `seele-core` descrito em `01`.

Frontend: **decidido em M5 — HTML, CSS e JavaScript à mão, sem framework e sem npm** (ADR 0019). O critério que decidiu não foi preferência de framework: foi não ter duas árvores de dependência com só uma auditada. O `cargo deny` cobre o produto inteiro; um `node_modules` seria a única parte fora dele.

**Custo do Tauri, medido em M5** (ADR 0020): a árvore traz 16 avisos `unmaintained` — dez bindings GTK3 alcançados só no Linux via `webkit2gtk`, cinco tabelas Unicode, um proc-macro de build — e cinco crates sob MPL-2.0. Nenhuma vulnerabilidade. Cada exceção está nomeada no `deny.toml` com o motivo; nenhum `ignore` genérico.

Requisito não negociável: **nenhuma lógica de protocolo em JavaScript**. Se o frontend precisa saber o que é um `ssrc`, algo está errado.

### A tela de entrada

Acima do formulário fica **ONDE VOCÊ JÁ ESTEVE**: os servidores visitados, com o
apelido usado em cada um e quando foi a última vez. Clicar numa linha preenche
e conecta; cada linha tem um *esquecer*. Sem Servers visitados a seção some
inteira e a tela é exatamente a de antes — o estado vazio não piora, e nada
fica escondido atrás de um clique. A lista é conveniência, como manda `05`:
ilegível ou corrompida, a seção não aparece e conectar continua funcionando.

Abaixo, o campo CONVITE aceita um `seele://` colado. Ele preenche o endereço, e
quem colou errado não perde o que já tinha digitado.

Os dois vêm de `seele-core` por comando Tauri — `conhecidos`, `esquecer`,
`analisar_convite` —, atravessando a `seele-ffi` como manda o ADR 0002. Não há
segundo analisador de URI em JavaScript, nem lista de atalhos lida do disco
pelo frontend: seriam dois conjuntos de casos de borda para discordar do
primeiro, e o requisito acima não é negociável.

**A impressão digital do convite.** Um `seele://` pode carregar a impressão
digital do certificado, e é ela que transforma o primeiro contato de cego em
verificado (ADR 0006). O app a confere. A impressão lida do link atravessa a
ponte como `ConnectConfig::expected_fingerprint`, vira a impressão esperada do
`Destino`, e a comparação acontece em `seele-core` — antes de haver sessão,
uma vez só, para as duas cascas. O `connection` lê o mesmo resultado; não há duas
conferências para discordar uma da outra.

O que volta da conferência é um veredito, não um booleano: primeiro contato
cego, primeiro contato verificado, Server já conhecido, convite que discorda de
um servidor conhecido, e convite que não confere no primeiro contato. Os dois
últimos são coisas diferentes e a tela os trata como tais. Um convite que não
confere no primeiro contato **recusa**: a conexão cai, a chave que o TLS tinha
acabado de fixar é desfeita, e a tela de entrada mostra a esperada e a ofertada
lado a lado. Um convite que discorda de um servidor já fixado **avisa**: o TOFU já
provou que é o servidor de sempre, então quem está errado é o link, a sessão
entra, e a ressalva fica visível dentro dela. Primeiro contato — verificado ou
cego — também aparece: o app diz o que acabou de fixar, porque fixar em
silêncio é fixar sem ninguém saber que havia o que conferir. Só o servidor já
conhecido, sem nada que o contradiga, não vira frase; repetir "a chave é a
mesma de sempre" a cada entrada ensina a não ler a linha no dia em que ela não
for.

Nenhuma frase dessas é escrita em Rust e nenhuma comparação é feita em
JavaScript — a fronteira é a mesma de sempre: o núcleo decide, a casca desenha.

O convite guardado vale para o servidor dele e para nenhum outro. Trocar o
endereço no campo descarta a impressão do link anterior, e a sessão que ele
abriu leva-a embora ao terminar. Sem isso, quem cola um link, entra, sai e
volta a entrar noutro endereço levaria consigo uma promessa que aquele servidor
nunca fez, e a recusa apareceria sem nada na tela que a explicasse.

### Hospedar pelo app

A tela de entrada tem **HOSPEDAR AQUI** ao lado de INSERIR PLUG. Sobe um servidor
dentro do processo do app — o mesmo `seele-server::hospedagem` do
`connection --hospedar` — e devolve o link de convite, que aparece no topo da sessão
pronto para copiar. Ele vive enquanto a janela estiver aberta.

Sem isso, hospedar exigia um terminal, e num produto cujo argumento é "hospede
você mesmo" exigir linha de comando de quem hospeda exclui justamente quem mais
ganharia. É a única exceção nomeada à regra de dependência do lado do app
(`cargo xtask check-deps`): aresta lateral no topo do grafo, não inversão — este
binário contém os dois papéis.

O comando **não conecta**. Conectar continua sendo o caminho de sempre, com o
endereço que ele devolve: um servidor hospedado aqui e um do outro lado do mundo
entram pela mesma porta.

### A marca

`docs/marca.md` é normativo para as duas cascas. Os dois pontos que o app tem de
respeitar: a assinatura `ゼーレ` vem de SVG com **glifos em contorno** — o app
não embarca fonte, e texto entregaria a marca à face japonesa do sistema, que a
folha proíbe substituir — e o favicon é a forma muda, porque abaixo de 32 px de
largura do connection a forma troca.

## Mobile

**Escopo em v1: somente consumo.** Ouvir, falar, ler e responder texto. Sem administração, sem gerenciamento de canais, sem configuração avançada.

Justificativa: áudio em background no iOS e Android é trabalhoso (interrupções por chamada, foco de áudio, restrição de bateria, notificação persistente obrigatória). Fazer isso bem já é o milestone inteiro; tentar paridade completa junto atrasa tudo.

**[EM ABERTO — decisão de plataforma]**

| Opção | A favor | Contra |
|---|---|---|
| Flutter + FFI | Uma base para as duas plataformas, boa performance de UI | Ponte FFI com áudio em background é área de pouca trilha batida |
| Nativo (Swift + Kotlin) | Controle total sobre áudio em background, que é o problema difícil | Dobra o trabalho de interface |
| Tauri Mobile | Reaproveita o frontend do desktop | Imaturo; risco alto para áudio em tempo real |

Recomendação: decidir **depois de M4**, com um protótipo descartável de áudio em background em cada candidata. Não decidir por afinidade prévia.

## Camada FFI (`seele-ffi`)

Superfície mínima e estável. **O `uniffi` entra em M6**, com o primeiro consumidor de binding; em M5 a `seele-ffi` foi escrita com a forma que ele exige, sem a dependência (ADR 0018). A lista do que M6 anota sem reescrever está lá, verificável.

```
conectar(host, credencial) -> Sessao
entrar_na_sala(sala_id)
sair_da_sala()
enviar_mensagem(canal_id, corpo)
definir_mudo(bool)
assinar_eventos(callback)
estado_atual() -> Snapshot
```

Regras:
- Objetos opacos com handle; nada de expor estruturas internas.
- Eventos entregues por callback em thread própria; a casca marshala para sua thread de UI.
- Erros como enum, nunca string.
- Zero dependência da FFI em tipos de UI.

## Paridade de interface

O design gráfico (ver `PROMPT-CLAUDE-DESIGN.md`) tem liberdade tipográfica e de movimento que o terminal não tem. Mas a **composição** — quais painéis existem, o que fica em cada um, onde a telemetria vive — deve ser reconhecivelmente a mesma. Alguém que usa a TUI deve abrir o app e saber onde tudo está sem procurar.

Toda tela gráfica precisa ter uma resposta para: "como isso ficaria em 80×24 monocromático?" Se não tiver, está fora do produto.

## Acessibilidade

Estas regras vieram da `06-clientes-gui.md` quando a TUI saiu do produto
(ADR 0039). Elas nunca foram sobre o terminal: são sobre **não depender de cor
para transmitir informação**, e valem em qualquer superfície.

- **Modo alto contraste e modo sem cor** — só forma e texto. Daltonismo é comum
  no público e a palheta depende muito de vermelho e verde.
- **Nenhuma informação transmitida só por cor.** O sinal vem sempre acompanhado
  do número; o mudo tem marcador textual além da cor. É a regra que mais é
  citada de dentro do código, e a que mais silenciosamente se perde: uma cor a
  mais é fácil de acrescentar e ninguém percebe que ela virou a única fonte.
- **[EM ABERTO]** Leitor de tela. Numa TUI era limitado a ponto de a pergunta
  ficar aberta por viabilidade; numa casca web o caminho existe — ARIA — e a
  pergunta passa a ser de escopo, não de possibilidade.

O que **não** veio: as regras de renderização em células, o `NO_COLOR`, o
terminal mínimo de 80×24 e os critérios de aceite por SSH. Aquilo era sobre o
terminal, e saiu com ele.

## Critérios de aceite

- Desktop: binário abaixo de 30 MB, RSS abaixo de 150 MB, inicialização abaixo de 2 s.
  **Medido em M5, macOS aarch64:** binário 18,0 MB, `.app` 18 MB, DMG 6,3 MB, RSS ocioso 112 MB, do `exec` até a janela pronta 191 ms. Linux e Windows não foram medidos — a matriz de três SOs nunca executou por falta de repositório remoto.
- Mobile: áudio sobrevive a bloqueio de tela, chamada telefônica recebida e troca de rede.
- Ambos: mesma sessão pode ser retomada em outro cliente sem perda de histórico.
