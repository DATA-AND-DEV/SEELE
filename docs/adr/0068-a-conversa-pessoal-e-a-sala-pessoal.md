# 0068 — A conversa pessoal é a sala pessoal; a direta sai do app e fica na página, a dois

Status: **aceito** · decidido por quem responde pelo projeto em 12/11/2026
Data: 2026-11-12
Sobre o [0059](0059-a-sala-pessoal-segura-a-conexao-enquanto-toca.md) (a campainha e o teto de 6), o [0060](0060-o-codigo-de-uma-pessoa.md) (o código e a conferência antes do `Hello`) e o [0063](0063-a-linha-2-fala-webrtc.md) (hospedar é só dos nativos; o ponto de encontro e o sinal).
**Emenda** o 0060 na consequência «no celular, a 0.16 não liga pelo código»: a página passa a ligar para a sala de quem tem o app.

## Contexto

Ligar para o código de alguém tinha dois caminhos, e quem escolhia era a ordem de um `if` na casca:

- **A conversa direta** («SEELE-CHAMADA/1»): um `RTCPeerConnection` do navegador ou da WebView, com a caixa HTTP `/chamada/1` no ponto de encontro. Duas pessoas, e só duas. É o único caminho entre duas páginas, porque a página não hospeda (0063).
- **A sala pessoal**: quem recebe hospeda um `seele-server` em memória, com a campainha do 0059. É uma sessão SEELE comum (o motor nativo de voz e de tela, o texto, o `Tocando`), e a campainha já aceita **até 6 pessoas** (`TETO_DA_LIGACAO`). Só existe no app.

O app discava primeiro a direta, e só tentava a sala quando a direta respondia «ausente». Entre dois apps novos a direta sempre respondia, então **a sala nunca era usada**: o caminho com o motor nativo, com o teto de 6 e com o mesmo comportamento de um servidor ficava de pé sem que ninguém passasse por ele, e o app mantinha dois motores de voz e de tela para a mesma conversa.

## Decisão

1. **Entre apps, só a sala pessoal.** `@CÓDIGO` no app disca a sala. A conversa direta sai da casca nativa: o app não registra caixa de chamada direta, não a disca e não a atende.
2. **A página (o navegador e o celular) entra na sala de quem tem o app, pelo código**, como convidada: toca, espera quem recebe atender, e entra numa sessão comum. A página continua sem hospedar (0063).
3. **Página com página continua pela conversa direta, a duas pessoas.** O celular da 1.0 é só a página, e duas pessoas sem o app precisam conversar. Por isso a direta **não** sai da página nem do ponto de encontro (`/chamada/1` fica).
4. **Grupo na 1.0: até 6, hospedado por quem atendeu a primeira ligação.** A terceira pessoa entra **ligando para o código de quem hospeda**, que atende. Há um botão CONVIDAR na ligação, que copia esse código. A conversa acaba quando quem hospeda sai. **O protocolo não muda**: o `Tocando` e as razões `ChamadaRecusada`, `NinguemAtendeu`, `ChamadaEncerrada` e `ServerFull` já existem.

### Como a página acha e confere a sala

A página não fala UDP: ela pede o caminho pelo sinal HTTP do ponto (`POST /sinal/1`, 0063). Para a sala faltava uma coisa. O navegador só fecha o DTLS com a impressão do certificado escrita **antes**, na descrição remota, e o DTLS da sala é efêmero (0063, item 3): nenhum código nem link a carrega.

- **A sala diz a impressão DTLS dela no `ATENDE`**, numa segunda linha do mesmo datagrama (`DTLS <impressão>`). Um ponto antigo lê só a primeira linha e não a vê; a linha de sempre continua aceita por um ponto novo.
- **O ponto a repassa só na resposta HTTP do sinal**, como um terceiro campo do `AQUI`, e só quando a marca chamada é de sala. O `AQUI` do ENC/3 (UDP) não muda: o nativo não precisa dela, porque vê o certificado e decide depois.
- **A impressão que o ponto entrega é pista, e não prova.** Quem autentica é a apresentação `0x03` da sala (0063): a chave Ed25519 dela assina a impressão do DTLS **desta** conexão. A página confere, dentro do `.wasm` e **antes do `Hello`**, a assinatura sobre a impressão que o navegador exigiu e o código contra a chave. Um ponto hostil que troque a impressão consegue «não entra», e nada mais: é a mesma fronteira do 0060.

### Quem escolhe o caminho

Um lugar só (`irParaOServidor`, em `camada-servidores.js`), e a regra é a mesma nas duas cascas: **a sala primeiro**. Quando o ponto diz que a sala não mora lá (`NaoLocalizou`), a página tenta a conversa direta, porque o código pode ser de quem só tem a página. O app não tenta mais nada.

## Alternativas

1. **Manter os dois caminhos no app, com a direta primeiro.** É o que havia. Recusada: a sala, que é o caminho com grupo e com o motor nativo, nunca era exercitada, e cada defeito de voz ou de tela tinha de ser consertado duas vezes.
2. **Só a conversa direta, também entre apps.** Recusada: ela é a dois por construção (um `RTCPeerConnection` por par), e o grupo pediria uma malha ou um segundo desenho.
3. **A página hospedar uma sala.** Recusada pelo 0063: o navegador não abre a escuta UDP que o anfitrião precisa.
4. **A impressão DTLS no registro assinado (`MORO3`), guardada pelo ponto.** Recusada: o ponto passaria a guardar mais um dado por sala, e a assinatura do registro não pode cobri-la sem quebrar o ponto antigo. No `ATENDE` ela é a da sala que atendeu agora, e o ponto não guarda nada.
5. **Um campo a mais na linha do `ATENDE`.** Recusada: o ponto antigo recusa a linha com um campo a mais, e toda ligação para um app novo daria `CALADO` até o ponto ser implantado.
6. **Passar a sala para outra pessoa quando quem hospeda sai.** Fora da 1.0 (já estava fora no 0059).

## Consequências

- **O app deixa de conseguir ligar para quem só tem a página.** Essa pessoa não hospeda sala, e o app não disca mais a direta. O caminho passa a ser **a pessoa da página ligar para o código de quem tem o app**. A frase de «não localizou» do app diz isso por extenso.
- **A página só entra numa sala quando o ponto de encontro já repassa a impressão.** Com o ponto antigo no ar, a resposta do sinal vem sem ela, e a página diz que não achou a sala e segue pela direta, que um app novo não atende mais. Por isso o ponto novo tem de estar no ar **antes** de a 1.0 ser publicada (`docs/ponto-de-encontro.md`).
- **Um app 0.19.6 e um app 1.0 continuam se falando**: a 0.19.6 tenta a direta, não acha a caixa do app novo, e cai na sala, que já era o recuo dela. No sentido contrário a 1.0 disca a sala, que a 0.19.6 hospeda.
- **A página 1.0 que liga para um app 0.19.6** acha a sala dele sem a impressão DTLS (a 0.19.6 não a diz) e recua para a conversa direta, que a 0.19.6 ainda atende. Uma página 0.19.6 (uma cópia guardada no aparelho) ligando para um app 1.0 não acha caixa direta nem sabe entrar na sala: não liga.
- **Entre duas páginas, a conversa direta começa depois de uma pergunta ao ponto** (a sala que não existe): uns dois segundos a mais.
- **O grupo depende de quem hospeda.** Quando quem atendeu a primeira ligação sai, todos leem o fim da chamada. Não há passagem de sala.
- **Quem entra pela página numa sala é uma sessão a mais na página**, com o que a página tem: a voz e o texto, a tela pelo canal (0067), e a reconexão pelo mesmo código.
- **A página também fixa a chave inteira** (0060, «Força»): depois de uma ligação completa, a chave de quem atendeu fica no contato daquela pessoa (o mesmo da conversa direta, no IndexedDB da origem), e a ligação seguinte confere 256 bits. Quem limpa os dados do navegador volta a conferir pelos 75 do código.
- **Quem hospeda não é avisado de quem ligou com a ligação cheia**, nem de um bloqueado: a campainha recusa sem tocar, e só o registro fica sabendo.
- O ponto de encontro ganha dois campos opcionais no fio (a segunda linha do `ATENDE` e o terceiro campo do `AQUI` do sinal) e **nenhum estado novo**.

## O que não está medido

- Duas máquinas de verdade em redes diferentes, com a página entrando numa sala atrás de um NAT de casa.
- O navegador do iPhone entrando numa sala.
- Um grupo de seis com tela.

## Custo de reverter

A ordem da discagem e a volta da direta ao app revertem com `git revert`. A segunda linha do `ATENDE` e o terceiro campo do `AQUI` são fio publicado depois que a 1.0 sair: podem deixar de ser usados, e não saem (0058).
