# 0003 — TOFU como padrão de certificado

Status: aceito por default
Contexto: `specs/08-seguranca.md` deixa em aberto entre TOFU com certificado auto-assinado e ACME/Let's Encrypt. É critério de aceite de M0.
Decisão: TOFU com pinning por padrão. ACME como opção documentada, não como caminho principal. O aviso de troca de chave é um `Alerta · 警告` bloqueante, impossível de ignorar.
Alternativas: ACME por padrão. Descartado porque exige domínio e portas 80/443 disponíveis, o que contradiz a simplicidade de porta UDP única de `specs/01-arquitetura.md` e o perfil de operador descrito em `08` — confiável, mas não especialista em segurança. O modelo do SSH é o que o público-alvo já tem na cabeça.
Consequências: mais fácil — auto-hospedagem sem domínio, sem renovação, sem porta extra. Mais difícil — exige UX explícita de aceite e de troca de chave, e essa UX precisa existir na TUI (M4) e no app (M5), não só no papel. Não há caminho não criptografado em nenhum dos dois modos.

Custo de reverter: **médio**. Adicionar ACME depois é aditivo; tirar TOFU depois quebra clientes que já pinaram.

## Adendo — 2026-09-29 · o pino prova o endereço que a pessoa escolheu

O pino é por endereço: prova que *este* `IP:porta` já apresentou *esta* chave.
Por isso um link que discorda do pino avisa e não recusa: o pino é a prova de
continuidade, e quem discorda dele é o link.

Isso vale no **alvo**, o endereço que a pessoa escolheu (o do link, o
digitado, ou o da entrada da lista). Não vale num **candidato que ninguém
escolheu**, e a corrida de candidatos do ADR 0037 tem dois:

- **A resposta do quarto** do ponto de encontro (ADR 0022). O quarto guarda o
  endereço de onde veio o registro. Um ponto hostil, ou alguém que registre a
  marca de outro servidor a partir do socket do próprio servidor, faz o quarto
  apontar para um endereço que esta máquina já fixou com a chave de quem
  atende ali.
- **O alternativo que colide na LAN.** `192.168.x.y:8383` é o mesmo endereço
  de uma casa para outra, e ali atende o servidor de sempre da casa em que a
  pessoa está. Isso vale para um alternativo; o endereço de LAN do próprio
  link é o alvo (ver abaixo).

Nos dois, um pino que confere só diz que algum servidor já atendeu naquele
endereço. Deixá-lo passar mandava o `Hello` (o convite, o apelido e a
assinatura) a um servidor que a impressão prometida desmente.

**Num candidato que ninguém escolheu, a impressão prometida vale mais que o
pino.** A que não confere recusa dentro do TLS, antes do `Hello`, mesmo com
pino que confere, e nada é fixado nem desfeito. No alvo, a regra de cima
continua. Uma chave trocada continua recusada nos dois.

**O endereço de LAN do link é o alvo, e a colisão nele não é coberta.** O
primeiro endereço de um link é o da rede de casa do anfitrião, quando ele tem
uma, e é também a chave da entrada que o link deixa na lista de servidores.
Para quem visita pela internet, esse é justamente o endereço que se repete de
uma casa para outra. Se na rede em que a pessoa está outro servidor atende
nele, já fixado, o pino confere no alvo e a regra de cima vale: o `Hello` vai
para ele, com aviso. O que se fecha é o efeito permanente: numa volta sem
link, com um alvo de escopo local (privado, link-local, CGNAT, ULA), a lista
não passa a guardar a chave de quem atendeu (`seele_ffi::impressao_a_guardar`).
Com link, ela passa, porque é a decisão de cima (quem discorda do pino é o
link), e nesse caso ela erra sobre quem é quem. O `Hello` para quem atende no
endereço de LAN do alvo, e a chave dele na lista depois de um link, ficam como
resíduo desta decisão.

A regra está em `seele_core::tofu::TofuVerifier::decide`. O teste que a prende é
`um_candidato_que_a_pessoa_nao_escolheu_nao_toma_a_entrada_da_lista`, com o
irmão `um_candidato_que_a_pessoa_nao_escolheu_nao_bate_na_portaria_dele`, em
`crates/seele-conformance/tests/volta_pela_trilha.rs`. Os dois olham o servidor
que ninguém escolheu, e não o cliente: nenhuma conexão de pé, o convite inteiro,
a portaria vazia. A lista que não toma a chave de quem atendeu no alvo de LAN é
presa por `a_volta_por_um_alvo_de_lan_que_discorda_do_pin_deixa_a_lista_como_estava`,
em `crates/seele-ffi/src/lib.rs`.
