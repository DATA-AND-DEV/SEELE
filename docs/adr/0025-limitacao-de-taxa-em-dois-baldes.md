# 0025 — Limitação de taxa: dois baldes, e um aviso antes da porta

Status: aceito

> **Vocabulário.** Esta página é anterior ao [ADR
> 0035](0035-o-codigo-deixa-de-falar-evangelion.md) e diz `Dogma` onde o
> produto hoje diz **servidor**, `Cage` onde diz **sala de voz**, `Linha` onde
> diz **canal** e `piloto` onde diz **pessoa**. O texto fica como foi escrito:
> o 0035 preserva de propósito o registro de ontem, e o `docs/glossario.md` é
> a autoridade sobre a palavra de hoje.

Contexto: o ADR 0021 pôs um porteiro no Dogma e escreveu, na própria lista de consequências, o que ele não resolvia: *"um convidado legítimo pode inundar de mensagens; não há limitação de taxa (dívida registrada, `DisconnectReason::RateLimited` existe e nunca é enviado)"*. Metade do formato, portanto, já estava decidida havia cinco milestones — o protocolo sabia dizer "você excedeu", e o servidor nunca dizia. `specs/08-seguranca.md` pede as duas coisas nominalmente: "limite de quadros por segundo por remetente" no modelo de ameaça, e "rate limiting de tentativas, com backoff por IP e por identidade" na seção de autenticação. Era a última trava aberta antes de um Dogma poder ficar exposto na internet.

Decisão: **um balde de fichas**, em `crates/seele-server/src/taxa.rs`, consultado em três lugares — dois novos e um que já existia com mecanismo próprio.

**Antes de autenticar**, com chave no endereço de origem, no primeiro instante de `session::serve`. Trinta apertos de mão de rajada, repostos a trinta por minuto. É o balde que importa para a internet: o segredo do ADR 0021 viaja no `Hello` e é verificado em Argon2id, escolhido caro *de propósito*, então cada pacote de quem varre a rede compra dezenas de milissegundos de CPU do anfitrião. Os números saem do pior caso honesto, que não é uma pessoa e sim um NAT: a bateria interna (`crates/seele-core/src/battery.rs`) tenta reconectar com espera exponencial de 500 ms a 15 s ao longo de cinco minutos, o que dá umas vinte e quatro tentativas por cliente; três pessoas na mesma casa saindo juntas de um roteador reiniciado são setenta em cinco minutos, catorze por minuto. Trinta por minuto é o dobro disso e um centésimo do que um laço de script faz sem suar.

**Depois de autenticar**, com chave na conexão. Sessenta quadros de controle de rajada — entrar num Cage, abrir as Linhas e pedir o histórico de cada uma é uma rajada real, e acontece em toda conexão —, repostos a vinte por segundo. Chave na conexão e não no piloto porque a mesma pessoa em duas máquinas são duas conexões legítimas, e quem abre conexões em série para diluir o limite esbarra antes na portaria, que conta por endereço: os dois baldes se compõem, e nenhum dos dois sozinho fecha os dois caminhos.

**Ao estourar, avisa antes de derrubar.** O primeiro quadro excedente é descartado e responde `AlertReason::RateLimited` — variante nova no protocolo, acrescentada no fim da enumeração; os seguintes são descartados calados; e ao ducentésimo a conexão cai com `DisconnectReason::RateLimited`. Derrubar no primeiro excedente puniria um cliente que só está mal escrito, e derrubar sem explicar é como um produto passa a parecer quebrado. Duzentos descartes são dez segundos inteiros no dobro do teto: é prova de que ninguém do outro lado leu o aviso. Quem se aquieta o bastante para o balde encher de novo é perdoado, inclusive do direito de ser avisado outra vez.

**Balde de fichas e não janela fixa**, e por isso o limite de mídia que já existia em `cage.rs` foi reescrito sobre o mesmo balde em vez de ficar ao lado dele. Uma janela fixa de um segundo aceita o limite inteiro no fim de uma janela e o limite inteiro no começo da seguinte — o dobro da taxa contratada, em milissegundos, sempre no mesmo instante do relógio, que é o instante com que um atacante se sincroniza. O balde não tem borda. A mídia continua sendo **descartada** e não derrubando ninguém, como `specs/04-servidor-seele.md` manda: áudio depressa demais é gaguejo de emissor muito mais vezes do que é ataque.

**O tempo entra por parâmetro.** Nada em `taxa.rs` lê relógio; quem chama passa o instante, como já fazem `dogma::Slots` e o `battery.rs` do `seele-core`. É o que torna testável o que acontece no limite sem um único `sleep` e sem depender de a máquina de CI estar desocupada — e foi o que permitiu ver cada teste reprovar com o código sabotado antes de dá-lo por bom.

Alternativas:

1. **Janela fixa**, que é mais simples de escrever e de explicar. Recusada pela borda descrita acima, e porque manter duas disciplinas de limitação no mesmo servidor é como elas divergem.
2. **Contar por identidade** depois de autenticar, e não por conexão. Exigiria estado compartilhado com tranca para uma decisão que é puramente local, e puniria quem legitimamente usa o `plug` e o app ao mesmo tempo.
3. **Recusar no `Incoming` do quinn**, antes de qualquer criptografia. É mais barato e é o degrau seguinte se um Dogma for de fato inundado — mas recusa sem conseguir dizer por quê, e o cliente lê "não foi possível alcançar o Dogma", que é exatamente a confusão que `session::despedir` existe para evitar.
4. **Derrubar direto, sem aviso.** Metade do código e metade do valor: o convidado que estourou o limite por engano fica sem saber que existe limite.

Consequências:

- A última trava de segurança para expor um Dogma cai. O que falta para "usar com os amigos pela internet" passa a ser alcançar o anfitrião de fora (ADR 0022), que é problema de rede.
- **Sobra um degrau.** O balde é consultado depois de o QUIC ter completado o aperto de mão TLS: quem abre conexões e nunca fala ainda compra uma assinatura por tentativa. A espera pelo fluxo de controle passou a ter prazo, o que impede a tarefa de ficar parada, mas o custo do TLS continua lá.
- A tabela de endereços tem teto de 4096 e esquece primeiro quem se repôs por inteiro — um balde cheio é indistinguível de um recém-criado. Se ainda assim estiver cheia, o endereço novo é recusado: sob rede de máquinas, o custo cai sobre quem chega durante o ataque e não sobre quem já está dentro.
- Um cliente uma versão de protocolo mais velho não conhece `AlertReason::RateLimited` e falha ao decodificar o quadro. Custa uma conexão que já estava excedendo o orçamento, e nada mais.
- Os limites são constantes nomeadas com a razão escrita ao lado, e não configuração. Quando alguém precisar afrouxá-los, o número a mexer tem endereço e o argumento a rebater está escrito junto.

Custo de reverter: **baixo**. Um módulo, três chamadas e uma variante no fim de uma enumeração.
