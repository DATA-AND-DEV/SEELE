# 0059 — A sala pessoal segura a conexão enquanto toca

Status: **aceito**. Decidido por quem responde pelo projeto em 2026-09-23 («toque
instantâneo já na 1.0», spec da sala pessoal), e escrito na 0.16.
Data: 2026-10-08
Sobre `docs/superpowers/specs/2026-09-23-sala-pessoal-por-codigo-design.md`
(«A campainha» e «O aperto de mão em três fases») e o spec-mestre da 0.16
(`docs/superpowers/specs/2026-10-08-0.16-design.md`, §2 «F7»).
**Restringe o [ADR 0030](0030-quem-bate-a-porta.md) aos servidores guardados.**

## Contexto

O ADR 0030 recusou segurar a conexão de quem bate à porta. Para ele, um pedido
pendente cai na hora com `AdmissionPending`, e o pedido fica guardado até quem
hospeda olhar. O motivo está escrito lá: «"Ninguém atendeu" só existe como
estado se alguma coisa estiver esperando». Num servidor de grupo, ninguém está
do outro lado esperando uma resposta agora. Quem hospeda decide quando olhar, e
o servidor não segura recurso por quem ainda não entrou.

A sala pessoal é o contrário disso. Ela é um telefone: quem liga está olhando
para a tela **agora**, e quem recebe está com o app aberto **agora**. O dono
pediu o toque instantâneo e a diferença entre «RECUSOU» e «NÃO ATENDEU». As duas
coisas só existem se a conexão ficar de pé enquanto toca.

## Decisão

**Na sala pessoal, e só nela, a conexão fica segurada enquanto o app de quem
recebe toca.** Nos servidores guardados, a portaria do 0030 continua como está:
nada espera.

### O aperto de mão em três fases

Em `crates/seele-server/src/session.rs`, só quando a hospedagem é a sala:

1. **`Hello` → `Challenge` → `Response`**, em 10 s como hoje, com a prova
   amarrada ao certificado (o S2). Quem não prova a chave não chega a tocar.
2. **O toque.** O servidor manda `ServerMessage::Tocando { prazo_s: 45 }` e
   espera, num `select!`, por três coisas: a decisão da campainha, o prazo e a
   conexão fechar. **A trava do banco não é segurada nessa espera.**
   - Atender segue para a fase 3.
   - Recusar manda `Disconnecting { ChamadaRecusada }`.
   - O prazo vencido manda `Disconnecting { NinguemAtendeu }`.
   - A conexão fechada para o toque como cancelado.
3. **A conta e a sessão**, em 10 s.

As três razões novas entraram no fim de `DisconnectReason`, a partir do ordinal
18, e são terminais no cliente: a bateria de reconexão não as trata como queda.

### A campainha

A decisão de cada ligação mora em `crates/seele-server/src/campainha.rs`, uma
máquina de estados pura com o tempo injetado, no molde de `taxa.rs`. Os números:

| parâmetro | valor | por quê |
|---|---|---|
| prazo do toque | 45 s | o bastante para achar o fone; curto o bastante para quem liga não ficar olhando uma barra parada |
| toques pendentes | até 3, e além disso `RateLimited` | uma faixa com dez nomes não é uma pergunta que alguém consiga responder |
| teto da ligação | 6, contando quem recebe (`ServerFull`) | é a decisão do dono, «pequeno grupo» |
| espera depois de uma recusa, por chave | 60 s (`RateLimited`) | uma recusa que se pudesse refazer na hora seria um toque em laço |
| tolerância de reconexão | 60 s, sem tocar de novo | quem caiu no meio da ligação volta para a mesma conversa |

- **Quem recebe é sempre admitido na própria sala.** A chave dele entra sem
  tocar.
- **A deduplicação é pela chave.** Os candidatos da corrida (ADR 0037) chegam com
  a mesma chave, e a conexão mais nova substitui a mais velha sem virar outro
  toque.
- **O bloqueado recebe `ChamadaRecusada` sem tocar.** Ele lê «RECUSOU», e não
  «bloqueado». A lista vem da agenda do app e muda ao vivo, e bloquear quem já
  está tocando para o toque como recusado.
- **A aprovação vale por ligação.** Quando quem recebe desliga, quem estava
  dentro recebe `ChamadaEncerrada` e as aprovações se apagam. Quem ainda estava
  tocando continua tocando: quem recebe pode atender e começar outra ligação.
- **«A sala esvazia» quer dizer quem recebe sair**, que é o desligar. Um
  convidado que cai sozinho continua aprovado durante a tolerância. Ler
  «esvazia» como «não sobrou convidado» faria a tolerância falhar justamente no
  caso de um convidado só.

### Antes do primeiro pacote

A sala sobe por `Hospedagem::iniciar`, com o gancho `antes_de_aceitar`
obrigatório (D-seg-1, `crates/seele-server/tests/janela_do_comandante.rs`).

**Na sala, o gancho cria a conta de quem recebe.** Ali não há portaria, e o dono
só conecta ao atender. Até lá, o banco em memória não tem conta nenhuma, e a
primeira conta criada vira a Comandante (`permissions::seat_the_arrival`). Sem o
gancho, o primeiro atendido ficaria com a sala.

## Alternativas

1. **A portaria do 0030 também na sala, com um pedido guardado.** Recusada, porque
   ela não toca: quem liga leria «pedido guardado», e uma ligação não é um
   pedido para depois.
2. **Segurar a conexão também nos servidores guardados.** Fora do escopo. Lá
   ninguém está esperando uma resposta agora, e os motivos do 0030 continuam
   valendo.
3. **Tocar sem segurar: a conexão cai, e quem liga tenta de novo em laço até ser
   atendido.** Recusada. Quem recebe veria um toque por tentativa, e quem liga
   não teria como distinguir «recusou» de «não atendeu».

## Consequências

- O servidor da sala segura recursos por quem ainda não entrou. São no máximo 3
  conexões tocando, sem a trava do banco e sem sessão. É o custo que o 0030 não
  quis pagar, e aqui ele é limitado por construção.
- «NÃO ATENDEU» passa a ser uma resposta sobre a qual se age: quem liga sabe em
  45 s, e não fica sem saber.
- Uma ligação não sobrevive à troca de rede de quem recebe. A bateria de
  reconexão volta ao `IP:porta` que atendeu e desiste em 60 s: é um limite
  conhecido da 1.0.
- Passar a sala para outro participante quando quem recebe sai, e ligações com
  mais de 6 pessoas, ficam fora.

## Custo de reverter

As três razões do fio e o `Tocando` não saem mais: a política do
[ADR 0058](0058-1x-nunca-quebra-o-fio.md) não remove nada do fio. A campainha e as
três fases revertem com `git revert`, e a sala volta a não existir.
