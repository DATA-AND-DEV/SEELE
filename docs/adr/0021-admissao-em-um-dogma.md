# 0021 — Quem entra num Dogma: convite de uso único, senha como alternativa

Status: aceito

Contexto: `specs/08-seguranca.md` fechava a seção de autenticação com **[EM ABERTO — escolher em M2]** e uma recomendação: *"chave pública como mecanismo primário, com convite por token de uso único para entrada em um Dogma. Senha como fallback opcional configurável pelo operador."* A escolha nunca foi feita, e o efeito prático só ficou visível quando surgiu a pergunta de compartilhar acesso com um amigo: **não havia porteiro nenhum.** Quem alcançasse a porta UDP completava o handshake e ganhava uma conta.

Para uma rede local entre duas máquinas isso é o comportamento certo, e é por isso que passou despercebido por cinco milestones. No dia em que alguém abre a porta no roteador, deixa de ser.

Decisão: implementar exatamente o que a spec recomendava — os dois mecanismos, com o convite como primário.

**Convite de uso único.** 160 bits de aleatoriedade em base32 de Crockford (sem `0`, `1`, `I`, `O`, `L`, porque alguém vai ditar isso por telefone). Vale sete dias e uma vez só. O consumo é um `UPDATE ... WHERE usado_em IS NULL`, então dois clientes com o mesmo convite no mesmo instante não passam os dois.

**Senha do Dogma.** Argon2id, como a spec pede nominalmente. Para quem prefere um segredo único do grupo.

**Um Dogma sem nenhum dos dois continua aberto.** É o padrão, de propósito: é o que faz o teste em rede local funcionar sem cerimônia. O `seeled` avisa em voz alta ao subir assim escutando fora do loopback — um padrão inseguro que se anuncia é diferente de um padrão inseguro silencioso.

O segredo viaja no `Hello`, portanto **antes** do desafio-resposta. Deliberado: gastar verificação de assinatura com quem nem devia estar batendo à porta é trabalho de graça para quem varre a internet. O canal já é TLS 1.3 desde o primeiro byte.

A recusa é sempre `CredentialRejected`, seja senha errada, convite gasto ou vencido. `specs/08-seguranca.md` exige falha uniforme, e um erro que distingue os casos conta a quem está adivinhando qual palpite chegou mais perto. O motivo real vai para o log do operador, que tem direito de saber o que houve na máquina dele.

Alternativas:

1. **Só senha.** Mais simples e pior: não dá para revogar o acesso de uma pessoa sem trocar para todo mundo, e uma senha compartilhada vaza pelo membro mais descuidado.
2. **Só convite.** Quase escolhido. A senha ganhou lugar porque um grupo fixo e pequeno — que é o caso de uso central — não quer gerar convite toda vez que alguém reinstala o cliente.
3. **Lista de chaves públicas autorizadas**, estilo `authorized_keys`. É o fim certo desta estrada e exige o amigo mandar a chave dele antes, o que inverte o fluxo social: quem convida é quem tem trabalho. Fica para quando houver administração de verdade.

Consequências:

- Um Dogma exposto na internet tem como se fechar, o que antes não tinha.
- O convite é o que torna uma URL compartilhável defensável: um token gasto que vaze depois não vale nada. Ver ADR 0006.
- **Não resolve abuso de quem já entrou.** Um convidado legítimo pode inundar de mensagens; não há limitação de taxa (dívida registrada, `DisconnectReason::RateLimited` existe e nunca é enviado). — *Fechado depois, pelo ADR 0025: dois baldes de fichas, um por endereço antes de autenticar e outro por conexão depois, com aviso antes da porta.*
- A senha em Argon2id custa alguns milissegundos por handshake, de propósito. Não é caminho quente.

Custo de reverter: **baixo**. Um módulo, uma migração aditiva, e um campo opcional no `Hello` que Dogmas abertos ignoram.
