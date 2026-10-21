// SEELE — enum → frase, para toda tela.
//
// A fronteira erro→texto do produto fica aqui, e é por isso que nenhuma
// mensagem para gente é escrita em Rust. O protocolo carrega enums justamente
// para que cada casca escreva as suas (`specs/02-protocolo.md`).
//
// Compartilhado porque já é: a entrada lê `fraseDeErro` em três caminhos, a
// sessão lê `AVISOS`, e o fim lê `MOTIVOS`. Uma tela nova que precise dizer por
// que algo falhou acrescenta a frase aqui, e não um dicionário próprio.

"use strict";

/**
 * A frase para um motivo de fim de sessão.
 *
 * O protocolo carrega enums justamente para que cada casca escreva as suas
 * (`specs/02-protocolo.md`). Este é o mesmo conjunto de frases do `connection`, em
 * português, porque é o mesmo produto.
 */
const MOTIVOS = {
  // **A frase diz o que fazer, e passou a dizer na v0.11.0.**
  //
  // Ela era só «VERSÃO INCOMPATÍVEL COM ESTE SERVIDOR» — verdadeira, e sem
  // saída. A v0.11.0 é a release em que ela vai aparecer para todo mundo: a
  // publicada anterior fala protocolo 3, esta fala 6, e a janela alcança uma
  // versão. Os dois lados se recusam.
  //
  // «Um dos dois» e não «o seu», porque o motivo é o mesmo nos dois sentidos:
  // `negotiate` devolve `PeerTooOld` ou `PeerTooNew`, e as duas chegam aqui
  // como `Incompatible`. Uma frase que mandasse *você* atualizar estaria errada
  // metade das vezes, e erraria justamente com quem já está atualizado.
  Incompatible:
    "VERSÃO INCOMPATÍVEL COM ESTE SERVIDOR.\n" +
    "Um dos dois lados está desatualizado — atualize o SEELE nas duas máquinas e tente de novo.",
  CredentialRejected: "CREDENCIAL RECUSADA",
  // **Esta é o servidor dizendo que desistiu de esperar por nós.** Ela chega
  // pelo fio, num `Disconnecting`, então o outro lado recebeu o que mandamos e
  // decidiu parar — o oposto do `SemResposta` da tabela de baixo, onde nada
  // chegou a lugar nenhum. As duas diziam a mesma frase, e ela descrevia só uma.
  HandshakeTimeout: "O SERVIDOR DESISTIU DE ESPERAR O NOSSO APERTO DE MÃO",
  Kicked: "DESCONECTADO POR UM OPERADOR",
  Banned: "ACESSO BARRADO POR UM OPERADOR",
  ServerFull: "SERVIDOR LOTADO",
  ScheduledMaintenance: "MANUTENÇÃO PROGRAMADA",
  ServerShuttingDown: "O SERVIDOR ESTÁ ENCERRANDO",
  Timeout: "ENLACE PERDIDO",
  ProtocolViolation: "PROTOCOLO VIOLADO",
  RateLimited: "LIMITE DE MENSAGENS EXCEDIDO",

  // Faltava, e a falta era silenciosa: a ponte manda `FellBehind` desde que a
  // pendência #1 foi fechada, e a tela de fim não tinha frase para ele — o
  // motivo saía em branco justamente na queda que **não** é culpa de ninguém.
  // Encontrado pelo guarda que confere `EndReason` contra esta lista.
  //
  // Diz o que houve com a conversa, e não o que houve com o barramento: quem lê
  // quer saber se perdeu alguma coisa. Perdeu, e voltar é o que a traz de volta
  // — «repõe o que faltou» diz as duas coisas numa frase só.
  FellBehind:
    "ESTE ENLACE FICOU PARA TRÁS DO SERVIDOR.",

  // ---- a portaria do ADR 0030 ----
  //
  // As duas únicas frases desta lista sobre uma entrada que ainda pode dar
  // certo, e por isso as duas únicas que dizem o que fazer em seguida.
  //
  // Elas são separadas de propósito, e a separação é o ADR inteiro em duas
  // linhas: «não decidiram ainda» e «decidiram que não» pedem coisas opostas de
  // quem lê. Dobrar as duas em CREDENCIAL RECUSADA — que é onde caíam antes de
  // existirem — mandaria embora quem só precisava esperar.
  //
  // Nenhuma das duas fala em esperar. Nada está esperando: a conexão caiu no
  // mesmo instante, e o que ficou de pé é o pedido, do outro lado. Uma frase
  // que dissesse «aguarde» descreveria uma barra girando que não existe.
  AdmissionPending:
    "AGUARDANDO APROVAÇÃO DE QUEM HOSPEDA.\n" +
    "Seu pedido está guardado no servidor.",
  // «Não foi a senha nem o convite» fica porque muda o que a pessoa faz: sem
  // ela, a primeira reação é conferir os dois, e não há nada errado com eles.
  AdmissionDenied:
    "QUEM HOSPEDA RECUSOU A SUA ENTRADA.\n" +
    "Não foi a senha nem o convite: fale com quem hospeda por outro canal.",

  // A única recusa desta lista que a pessoa conserta sozinha, e a que mais
  // custou até ganhar frase própria: ela vestia CREDENCIAL RECUSADA, e o
  // conselho que vinha junto — confira o convite — mandava mexer na única coisa
  // que não era o problema. Nem ser aprovado, nem reinstalar o app resolvia.
  NicknameTaken:
    "ESTE APELIDO JÁ É DE OUTRA PESSOA NESTE SERVIDOR.\n" +
    "Não é o convite nem a senha: escolha outro apelido e entre de novo.",

  // ---- os MODs do ADR 0045 ----
  //
  // As três dizem a mesma coisa por dentro — o que este servidor exige não bate
  // com o que esta máquina aceitou — e pedem coisas diferentes de quem lê, que
  // é por isso que são três.
  //
  // Nenhuma delas fala em credencial, e a diferença não é cosmética: o primeiro
  // instinto de quem lê CREDENCIAL RECUSADA é conferir convite e senha, e aqui
  // os dois estão certos.
  ModsRecusados:
    "ESTE SERVIDOR EXIGE MODS QUE VOCÊ NÃO ACEITOU.\n" +
    "Entrar de novo mostra a lista outra vez: o que cada MOD é, de onde ele vem, " +
    "e o que ele alcança.",
  // Esta é a única que descreve uma sessão que estava funcionando. A pessoa não
  // fez nada — quem mudou foi o servidor —, e dizer isso evita que ela procure
  // erro no lado dela.
  ModsMudaram:
    "OS MODS DESTE SERVIDOR MUDARAM.\n" +
    "O que você aceitou não é mais o que ele exige: entre de novo para ler a lista nova.",
  // Nada a fazer do lado de quem lê, e a frase diz isso em vez de sugerir uma
  // tentativa que vai falhar igual. Quem conserta é quem hospeda, e o motivo
  // está no log da máquina dele.
  ModsIndisponiveis:
    "ESTE SERVIDOR EXIGE MODS QUE ELE NÃO CONSEGUE DESCREVER.\n" +
    "Não é a sua entrada: avise quem hospeda.",
  // A sala pessoal (spec de 23/09, «Erros e frases»). Provisórias e sem nome:
  // a spec as escreve com o nome de quem foi chamado («ALICE NÃO ATENDEU»), e
  // quem as reescreve com ele é a casca da ligação. «RECUSOU» e «NÃO ATENDEU»
  // continuam distintas, que é a decisão do dono de 23/09.
  ChamadaEncerrada: "A LIGAÇÃO FOI ENCERRADA DO OUTRO LADO.",
  NinguemAtendeu: "NINGUÉM ATENDEU.",
  ChamadaRecusada: "A LIGAÇÃO FOI RECUSADA.",
  // D-fio-7: o servidor roda MODs de uma API que esta versão não roda. Nada a
  // mudar do lado de quem hospeda; quem resolve é atualizar.
  ModsDeOutraApi:
    "ESTE SERVIDOR RODA MODS QUE ESTA VERSÃO DO APP NÃO RODA.\n" +
    "Atualize o app para entrar.",

  // ---- a volta da bateria, recusada dentro do TLS ----
  //
  // As duas vestiam CREDENCIAL RECUSADA, e a frase mandava conferir convite e
  // senha que não chegaram a quem atendeu: a recusa é antes do `Hello`. Do
  // aperto de mão sai só o `ClientHello` do TLS, como na recusa pela impressão
  // de `fraseDeErro`, e por isso nenhuma das duas diz que nada saiu.
  //
  // A primeira cobre duas recusas da volta: a impressão esperada que não
  // confere (`InviteMismatch`) e quem mostrou o certificado do servidor sem
  // provar a chave dele (`ChaveNaoProvada`). E a impressão esperada pode ter
  // vindo do link colado ou da lista de servidores. Esta tela só recebe o
  // `EndReason`, sem a origem, sem as impressões e sem saber qual das duas foi,
  // então a frase diz o que vale nos quatro casos: quem atendeu não é o
  // servidor desta sessão, o convite, a senha e o apelido não chegaram a ele,
  // e o remédio é falar com quem hospeda por outro canal. Nada de «convite»
  // como dono da impressão, nem de «link»: é o que `fraseDeErro` separa por
  // `deOnde`, e aqui não dá.
  //
  // A segunda é o alarme de chave trocada do ADR 0003. As impressões não
  // atravessam a ponte aqui (o motivo do fim é um nome só), e ficam no
  // `seele.log`, na linha da volta recusada.
  ServidorTrocouNaVolta:
    "QUEM ATENDEU NA VOLTA NÃO É O SERVIDOR DESTA SESSÃO.\n" +
    "O convite, a senha e o apelido não chegaram a ele. " +
    "Confirme com quem hospeda, por outro canal, antes de entrar de novo.",
  ChaveMudouNaVolta:
    "A CHAVE DO SERVIDOR MUDOU DURANTE A SESSÃO.\n" +
    "Confirme por outro canal antes de entrar de novo.",

  LinkLost: "ENLACE PERDIDO",
};

const AVISOS = {
  Mentioned: "VOCÊ FOI CHAMADO",
  SubsystemChanged: "UM SUBSISTEMA MUDOU DE ESTADO",
  SyncDegraded: "SINAL EM QUEDA",
  VoiceRoomEntryRefused: "ENTRADA NA SALA DE VOZ RECUSADA",
  PermissionDenied: "PERMISSÃO NEGADA",
  VoiceRoomFull: "SALA DE VOZ LOTADA",
  OperatorNotice: "AVISO DO OPERADOR",
  // O aviso que o servidor manda **antes** de derrubar. É o único da lista que
  // pede uma mudança de comportamento de quem o lê, e por isso ele existe:
  // derrubar sem ter avisado é o que faz o produto parecer quebrado.
  RateLimited: "VOCÊ ESTÁ FALANDO RÁPIDO DEMAIS PARA O SERVIDOR",

  // Uma transmissão de tela por sala de voz. Chega como aviso a quem tentou
  // começar a segunda, e a frase é a mesma da recusa em `FRASES` sem a segunda
  // linha: aqui ninguém apertou nada agora, então não há o que fazer em
  // seguida além de ler.
  ScreenShareTaken: "ALGUÉM JÁ ESTÁ COMPARTILHANDO A TELA NESTA SALA",

  // O servidor parou a transmissão desta pessoa porque a sala cresceu além da
  // subida de quem hospeda (§5.1: o teto é o caminho do anfitrião ÷ quem
  // assiste). A frase diz **de quem** é o caminho que faltou, e é para isso que
  // ela não é `SyncDegraded`: «sinal em queda» mandaria quem lê conferir a
  // própria conexão, que está boa. Não manda tentar de novo — o que mudaria a
  // resposta é a sala esvaziar, e isso não está na mão de quem lê.
  ScreenShareOverHostUplink:
    "A TELA PAROU: A SALA CRESCEU ALÉM DA CONEXÃO DE QUEM HOSPEDA O SERVIDOR",

  // Alguém com a permissão de mover pessoas pôs esta numa sala de voz. Sem a
  // frase, a pessoa se via numa sala sem saber por quê — indistinguível de um
  // defeito. Diz quem fez, onde ela está agora e o que dá para fazer; não acusa
  // ninguém pelo nome, porque o aviso não traz o nome.
  //
  // **Não supõe uma sala de antes** (M-5 da rodada 1 do lote I-b): o servidor
  // move também quem estava fora de toda sala, e «volte à de antes» seria um
  // conselho sem como seguir.
  MovedByOperator:
    "ALGUÉM QUE MODERA ESTE SERVIDOR PÔS VOCÊ NUMA SALA DE VOZ. " +
    "Você já está nela; se não quer ficar, saia da sala ou entre na que preferir.",

  // ADR 0038: uma sala passou do que a internet de quem hospeda aguenta com
  // todos falando juntos. Só chega a quem administra o servidor, que é quem
  // pode fazer algo. **Não para nada** — ninguém é barrado —, e a frase diz o
  // risco e a saída. Os dois números vêm no motivo e entram em `fraseDoAviso`,
  // em kbps, como a retenção põe a contagem dela.
  //
  // **«Com folga», e não «consegue enviar»** (rodada 1 do lote I-b): o aviso
  // dispara quando a sala passa de 60% da subida medida, e não dela inteira. A
  // sala que pede 3.000 kbps de uma internet que envia 4.000 dispara, e a frase
  // de antes dizia que ela passou do que a internet consegue enviar, com os
  // números ao lado desmentindo.
  VoiceRoomOverHostUplink:
    "UMA SALA DE VOZ PASSOU DO QUE A INTERNET DESTE SERVIDOR AGUENTA COM FOLGA",

  // ---- uma sala deixou de existir ----
  //
  // Três frases e não uma, porque pedem coisas diferentes de quem lê. As duas
  // primeiras chegam a quem estava dentro: quem estava lá já saiu e a conversa já saiu
  // da tela quando isto aparece, e sem a frase o que sobra é uma sala que sumiu
  // sozinha — indistinguível de uma janela que perdeu a conta de onde estava.
  //
  // A terceira não é sobre uma sala que foi: é uma recusa, e a única desta
  // lista que ensina o que fazer em seguida.
  VoiceRoomDeleted: "A SALA DE VOZ EM QUE VOCÊ ESTAVA FOI APAGADA",
  ChannelDeleted: "O CANAL DE TEXTO QUE VOCÊ LIA FOI APAGADO, COM TUDO QUE HAVIA NELE",
  LastVoiceRoom:
    "ESTA É A ÚNICA SALA DE VOZ DO SERVIDOR, E ELA FICA.\nFaça outra sala antes de apagar esta.",

  // ---- a retenção apagou histórico ----
  //
  // R15. Quem hospeda escolheu uma janela de retenção, e o servidor a aplicou:
  // não é defeito e não é recusa, é uma decisão do servidor acontecendo.
  //
  // A frase diz **que foi a retenção**, e não só «mensagens foram apagadas»: a
  // segunda faria quem lê procurar quem apagou. Não diz o número aqui porque o
  // número vem no campo — ver `desenharAviso`, que o insere.
  RetencaoApagouHistorico: "A RETENÇÃO DESTE SERVIDOR APAGOU HISTÓRICO ANTIGO",
  // PROVISÓRIA (F1, 09/10/2026), para a casca trocar pela frase por pedido e
  // causa. O pedido chega pelo nome (`"SetNickname"`), e a causa completa a
  // frase em `fraseDoAviso`, por `CAUSAS_DE_RECUSA` logo abaixo. Sem ela, a
  // faixa dizia só AVISO.
  PedidoRecusado: "NÃO DEU",
};

// PROVISÓRIA, com a entrada `PedidoRecusado` de `AVISOS`: o que segue o «NÃO
// DEU:» para cada causa. As três são as que a pessoa consegue resolver.
const CAUSAS_DE_RECUSA = {
  NomeTomado: "O NOME JÁ É DE OUTRA PESSOA",
  NaoExiste: "ISSO NÃO EXISTE MAIS",
  FalhaDoServidor: "O SERVIDOR FALHOU",
};

/**
 * Onde uma conexão está, enquanto ela acontece.
 *
 * Uma entrada por etapa de `seele_ffi::ConnectStage`, e a lista tem de ser
 * completa: o guarda `every_stage_of_an_arrival_has_a_sentence_in_the_page`
 * cobra cada nome que o núcleo publica. Uma etapa sem frase é a tela muda que
 * esta lista existe para acabar — quando o teste de campo das duas casas
 * falhou, quatro candidatos tinham sido tentados em série atrás de um spinner e
 * ninguém soube dizer em que ponto, porque não havia ponto nomeado.
 *
 * Os nomes vêm do Rust e as frases são daqui, como em toda a lista deste
 * arquivo (ADR 0012 e 0023). Nenhuma promete nada: `CaminhoAberto` diz que o
 * caminho abriu, e não que a conexão vai dar certo — a marca que o abre não
 * autentica ninguém, e o aperto de mão ainda tem de acontecer inteiro.
 */
/**
 * O que o sistema respondeu sobre o microfone, e o que fazer com isso.
 *
 * Os nomes vêm de `seele_ffi::PermissaoDeMicrofone`. Só os casos que **negam**
 * têm entrada: permitido não é notícia, e não saber não é acusação a fazer.
 *
 * `pedir` não existe aqui, ao contrário da permissão de tela, e a ausência é a
 * decisão: no Windows não há a quem pedir para um app de área de trabalho. Não
 * existe caixa de consentimento para disparar — nem para nós, nem para o
 * Discord. O que existe é um interruptor, e o botão só leva até ele.
 */
const PERMISSAO_DE_MICROFONE = {
  NegadaNaMaquina: {
    diz: "O MICROFONE ESTÁ BLOQUEADO NESTA MÁQUINA.",
    nota:
      "O bloqueio é de administrador e vale para todas as contas — ligar nos " +
      "seus ajustes não vai pegar. Quem consegue mudar é quem administra este " +
      "computador.",
  },
  NegadaParaTudo: {
    diz: "O WINDOWS ESTÁ BLOQUEANDO O MICROFONE PARA TODOS OS APLICATIVOS.",
    nota: "Ligue «acesso ao microfone» na página que o botão abaixo abre.",
  },
  NegadaParaAreaDeTrabalho: {
    diz: "O WINDOWS BLOQUEIA O MICROFONE PARA APLICATIVOS COMO ESTE.",
    nota:
      "O microfone está ligado, e o interruptor separado dos aplicativos de " +
      "área de trabalho não — ele fica no fim da mesma página, depois da lista.",
  },
  // **Só no navegador**, e não do Rust: a ponte da F4 a devolve quando a
  // Permissions API diz que o navegador negou o microfone a esta página (matriz
  // da F8, achado 3). Sem ela, o bloco ficava escondido justamente quando a
  // página não pode usar o microfone. Não há botão: uma página não abre os
  // ajustes do sistema, e as duas cascas do navegador o tiram. A nota serve às
  // duas, porque a tabela é uma só.
  NegadaPeloNavegador: {
    diz: "O NAVEGADOR NÃO DEIXA ESTA PÁGINA USAR O MICROFONE.",
    nota:
      "Libere o microfone para esta página nos ajustes do navegador: no " +
      "computador, no ícone ao lado do endereço da página; no iPhone, em " +
      "Ajustes › Apps › Safari › Microfone; no Android, em Configurações do " +
      "site › Microfone. Depois, entre de novo na sala de voz.",
  },
};

/**
 * O que a tela diz sobre a abertura do áudio, por estado. Pendência 52.
 *
 * Os nomes vêm de `seele_ffi::AberturaDoAudio`, como o serde os escreve. Na
 * 0.16 o áudio abre quando a pessoa entra numa sala de voz e fecha quando ela
 * sai, e `audio_available` falso deixou de querer dizer «falta áudio»: fora de
 * sala ele é falso sem faltar nada. É daqui, e não dele, que sai a palavra.
 *
 * Três lugares, e cada um pega a sua:
 * - `rodape`, a palavra do CODEC no rodapé da sessão. `null` é o travessão: fora
 *   de sala não há o que medir, e com a voz de pé quem fala é o codec;
 * - `medidor`, a linha do medidor em CONFIGURAÇÕES · MICROFONE E SOM;
 * - `frase`, a faixa da sessão, quando há o que fazer.
 *
 * **Fora de sala, nenhuma das três acusa falta de áudio.** É o guarda
 * `fora_de_sala_a_tela_nao_diz_sem_audio_e_a_frase_de_quando_nao_abriu_sai_do_estado`.
 *
 * A frase de quando não abriu a tempo **não manda escolher o microfone**: com a
 * voz que não veio, não há o que trocar, e escolher um aparelho só grava a
 * escolha para a próxima abertura.
 */
const ABERTURA_DO_AUDIO = {
  fora_de_sala: {
    rodape: null,
    medidor: "— FORA DE SALA",
    frase: null,
  },
  abrindo: {
    rodape: "ABRINDO",
    medidor: "— ABRINDO O ÁUDIO",
    frase: null,
  },
  aberto: {
    rodape: null,
    medidor: null,
    frase: null,
  },
  sem_aparelho: {
    rodape: "SEM ÁUDIO",
    medidor: "— SEM ÁUDIO NESTA SALA",
    frase: null,
  },
  nao_abriu_no_prazo: {
    rodape: "SEM ÁUDIO",
    medidor: "— SEM ÁUDIO NESTA SALA",
    frase: "O ÁUDIO NÃO ABRIU A TEMPO. A conversa por escrito já funciona; para tentar a voz de novo, saia da sala e entre de novo.",
  },
};

/**
 * O microfone que entrega zero exato, no macOS. F1-microfone-macos.
 *
 * O macOS entrega um microfone que ele bloqueia — o SEELE desligado em
 * Privacidade e Segurança — sem erro nenhum: o aparelho abre e manda zeros. O
 * laço de voz conta três segundos disso e a ponte o diz em
 * `telemetry.microfone_entrega_silencio_digital`.
 *
 * **«Pode estar»**, e não «está»: uma interface USB com o ganho no mínimo também
 * entrega zero exato. A frase diz onde conferir e o que fazer depois — entrar de
 * novo, porque a permissão só vale para o aparelho aberto depois dela.
 *
 * Só no macOS: no Windows o consentimento é lido antes de abrir
 * (`PERMISSAO_DE_MICROFONE`, acima), e este caminho de Ajustes não existe lá.
 */
const MICROFONE_EM_SILENCIO_NO_MACOS =
  "O MACOS PODE ESTAR BLOQUEANDO O MICROFONE.\nAjustes → Privacidade e Segurança → Microfone → ligue o SEELE, e entre de novo.";

/**
 * O aviso de fone, dito ao entrar numa sala de voz.
 *
 * O SEELE não cancela eco — o ADR 0007 tirou DSP em C/C++ do v1 e fez do fone
 * um requisito. Sem ele, o alto-falante volta para o microfone, e quem paga é o
 * outro lado: ouve a própria voz de volta e não tem como saber de onde vem.
 *
 * Uma frase só, e não um dicionário: não há enum por trás, e ela é a mesma na
 * faixa da sessão, na ajuda e em CONFIGURAÇÕES · MICROFONE E SOM — as duas
 * últimas escritas no `index.html`, e conferidas contra esta pelo
 * `ha_um_aviso_de_fone_em_algum_lugar_que_a_pessoa_le`.
 */
const AVISO_DE_FONE =
  "USE FONE. O SEELE ainda não cancela eco: sem fone, quem fala com você se ouve de volta.";

/**
 * Com que nome entra quem fecha a pergunta do nome sem responder.
 *
 * **Não diz o nome**, e não é descuido: ele é derivado em Rust, dentro do
 * `connect`, da impressão da chave desta máquina, e nenhum comando de tela o
 * expõe. Dizê-lo pediria um comando novo só para montar uma palavra que o
 * `connect` monta de qualquer jeito. A lista de pessoas o mostra logo depois.
 */
const ENTRAR_SEM_NOME = "Você vai entrar com um nome tirado da sua chave. Dá para trocar no perfil.";

/**
 * O que um texto colado em CONECTAR não deixa fazer, dito sem consultar a rede
 * (tarefa 30 da F7; spec de 23/09, «Do lado de quem liga», 1). Quem lê é o Rust
 * (`ler_entrada`), e é ele quem decide «é o seu código?»: a tela só diz, pelo
 * nome da variante de `ligar::EntradaColada`.
 */
const ENTRADA_COLADA = {
  CodigoComErro: "ESSE CÓDIGO TEM UM ERRO DE DIGITAÇÃO",
  SeuCodigo: "ESSE É O SEU CÓDIGO",
};

const ETAPAS = {
  Parada: "LENDO O CONVITE",
  // Provisória: a frase de uma ligação é da tarefa 7f (CHAMANDO e CANCELAR),
  // que troca esta. Até lá nada no app disca um código.
  Localizando: "LOCALIZANDO PELO PONTO DE ENCONTRO",
  Avisando: "AVISANDO O PONTO DE ENCONTRO",
  Tentando: "TENTANDO UM ENDEREÇO DO CONVITE",
  // Diz o que abriu e não o que vem: quem lê isto ainda pode não entrar.
  CaminhoAberto: "O CAMINHO ATÉ AQUI ABRIU",
  // Sem nome: numa ligação da tela, quem escreve é `fraseDaEtapaDaLigacao`,
  // com o nome de quem é chamado. Diz que chama, e não que alguém atendeu.
  Tocando: "CHAMANDO",
  Dentro: "DENTRO",
  // Neutra de propósito. `Desistiu` carrega o `ConnectError` inteiro — o núcleo
  // o guardou assim para não achatar `PinChanged` e `InviteMismatch`, os dois
  // erros que **não são de rede** (ADR 0003) — e afirmar aqui que nenhum
  // endereço atendeu apagaria esse alarme na tela justamente quando ele é a
  // coisa mais importante escrita nela. Esta linha diz onde a chegada parou; a
  // causa é de quem a tem, que é `fraseDeErro`.
  Desistiu: "A CHEGADA PAROU AQUI",
  // Quem liga desistiu (tarefa 16 da F7): não é falha, e a frase não procura
  // culpado. A tela de quem liga é da tarefa 7f, que pode trocá-la.
  Cancelada: "LIGAÇÃO CANCELADA",
};

/**
 * A frase de uma etapa de chegada.
 *
 * `Tentando` é a única montada, e o número é o motivo: «tentando um endereço»
 * repetido quatro vezes é indistinguível de uma tela travada, e «o endereço 3
 * de 4» é a informação que faltava a quem esperava sem saber quanto faltava.
 *
 * `Desistiu` **não** diz aqui o porquê, e não é só para não repetir: a etapa
 * não sabe o porquê. O motivo é um `ConnectError`, e dois dos seus valores —
 * `PinChanged` e `InviteMismatch` — não são falha de rede nenhuma. Uma frase de
 * etapa que dissesse «nenhum endereço atendeu» afirmaria causa de rede sobre o
 * alarme do ADR 0003, ao lado de um `fraseDeErro` que compõe esse alarme com as
 * duas impressões digitais. Uma linha diz onde a chegada parou, a outra diz o
 * que houve.
 */
function fraseDeEtapa(etapa) {
  if (typeof etapa === "string") return ETAPAS[etapa] ?? desconhecida(etapa);
  if (etapa && typeof etapa === "object") {
    const nome = Object.keys(etapa)[0];
    const base = ETAPAS[nome];
    if (!base) return desconhecida(etapa);
    const dados = etapa[nome];
    if (nome === "Tentando" && typeof dados?.candidato === "number") {
      const conta = base.replace("UM ENDEREÇO", `O ENDEREÇO ${dados.candidato + 1} DE ${dados.de}`);
      // **E qual endereço**, que é a metade que faltava. «2 de 4» diz que a
      // tela não travou; não diz por que a espera é longa. Um endereço de rede
      // local de outra casa demora porque ninguém responde, e ver
      // `192.168.68.104` ali é a diferença entre esperar sem saber e entender
      // que aquele candidato não tinha chance — relatado como «falta mensagem
      // que está testando os endereços da URL, por que às vezes demora».
      //
      // O endereço já atravessava a ponte em `Etapa::Tentando`; era esta linha
      // que o descartava.
      return dados.onde ? `${conta} · ${dados.onde}` : conta;
    }
    return base;
  }
  return desconhecida(etapa);
}

/**
 * Por qual caminho a conversa saiu.
 *
 * Uma entrada por `seele_core::chegada::Caminho`, e a lista tem de ser completa:
 * o guarda `every_path_a_connection_can_take_has_a_sentence_in_the_page` cobra
 * cada nome contra `seele_ffi::caminhos()`, que é derivada do enum.
 *
 * **Dicionário próprio, e não `FRASES`.** Dois destes nomes existem lá com o
 * mesmo texto de variante e outro significado: os degraus `FuroDeNat` e
 * `Ipv6Direto` do ADR 0022 são até onde **o link de quem hospeda** alcança,
 * decididos antes de existir par nenhum. Estes quatro são por onde **esta
 * conexão** de fato passou. Dobrar os dois conjuntos num dicionário só faria uma
 * frase sobre um link aparecer descrevendo uma conversa, e o defeito seria
 * invisível: a chave casa.
 *
 * Nomes, e não frases. Isto é uma métrica no rodapé, ao lado de ATRASO e
 * JITTER: é escrita uma vez quando a sessão sobe e cala depois. Só a degradação
 * vira frase, e a frase diz o que fazer.
 *
 * `FURO DE NAT` não promete nada, e é para não prometer que ela é um nome: o
 * núcleo é explícito em que um aviso enviado é evidência forte de que o furo
 * abriu, e não prova. É o mesmo grau de certeza do degrau homônimo da escada,
 * cuja frase por isso diz «deve funcionar».
 */
const CAMINHOS = {
  RedeLocal: "REDE LOCAL",
  Ipv6Direto: "IPv6 DIRETO",
  EnderecoPublico: "ENDEREÇO PÚBLICO",
  FuroDeNat: "FURO DE NAT",
};

/**
 * Por que uma transmissão de tela está parada.
 *
 * Os nomes vêm de `seele_ffi::motivos_de_parada_da_tela`, e é o mesmo acordo dos
 * caminhos: o núcleo enumera e a frase é daqui. Uma frase pronta em português
 * atravessando a ponte seria a única sentença desta janela que o guarda de
 * vocabulário não vê.
 *
 * Frases e não nomes — ao contrário de `CAMINHOS` — porque estas duas chegam
 * quando algo deixou de funcionar, e nenhuma pessoa deduz «o vídeo cedeu o lugar
 * para a voz» de `SINAL CRÍTICO`. As duas terminam dizendo que a volta é
 * sozinha, que é a única coisa a fazer: não há botão de tentar de novo, porque o
 * núcleo já está tentando.
 */
const PARADAS = {
  SinalCritico:
    "A TELA PAROU PARA A VOZ NÃO PICOTAR.\n" +
    "Ela volta sozinha quando o sinal melhorar.",
  AbaixoDoPiso:
    "A TELA PAROU: O QUE SOBROU DA CONEXÃO NÃO SUSTENTA NEM A MENOR IMAGEM.\n" +
    "Ela volta sozinha quando o caminho abrir.",
};

/**
 * O nome do caminho, ou nada.
 *
 * `null` e não «DIRETO», e não «DESCONHECIDO»: **sem informação a tela não
 * escreve nada**. A escada tem cinco degraus e a palavra que se inventaria
 * apagaria a distinção que importa — num furo de NAT a conversa é direta *e*
 * alguém soube que ela existe. Inventar um nome quando não se sabe é a mentira
 * confiante que o ADR 0022 existe para não produzir.
 *
 * Um nome que este arquivo não conhece também vira `null`, e é a única
 * divergência deliberada com `desconhecida`: aquela existe para uma falha, que
 * é quando dizer o nome cru ainda ajuda quem relata. Esta é uma métrica, e uma
 * métrica que imprime o nome de uma variante do Rust ao lado de `RTT 41ms` é
 * ruído. Quem pega esse caso é o guarda, antes de sair daqui.
 */
function fraseDeCaminho(caminho) {
  if (typeof caminho !== "string") return null;
  return CAMINHOS[caminho] ?? null;
}

/**
 * A frase para uma falha de conexão.
 *
 * O erro chega como enum — nunca como texto — e é aqui que ele vira uma frase.
 * Um `PinChanged` carrega as duas impressões digitais porque a coisa toda é um
 * humano compará-las (ADR 0003).
 */
/**
 * Por que um nome público não virou link, com o conserto na própria frase.
 *
 * Uma por recusa, e não uma frase genérica: quem digita este campo acabou de
 * mexer num DNS e erra por um detalhe de cada vez. «Nome inválido» manda a
 * pessoa procurar o que está errado; estas dizem.
 *
 * `ComEsquema` é a mais comum de todas, porque o gesto natural é copiar da
 * barra do navegador — e é por isso que o Rust a confere antes da barra: um
 * `http://casa.exemplo` tem as duas coisas, e ler «tire a barra» mandaria
 * apagar o caractere errado.
 */
/**
 * Por que uma versão de MOD foi retirada de circulação.
 *
 * A lista é fechada e mora no indexador (`listas.json`); aqui ficam as frases,
 * como o ADR 0012 manda. Um motivo que chegue sem frase aparece como o
 * identificador cru — o que é feio e honesto, e melhor que uma frase genérica
 * que esconda qual dos sete defeitos aconteceu.
 */
const MOTIVOS_DE_REVOGACAO = {
  "credencial-vazada": "uma credencial vazou no código publicado",
  "leitura-de-disco-fora-da-pasta": "ele lia arquivos fora da pasta dele",
  "rede-nao-declarada": "ele falava com a rede sem declarar isso",
  "dado-enviado-a-terceiro": "ele mandava dados para um terceiro",
  "nao-corresponde-ao-commit": "os arquivos publicados não batem com o commit avaliado",
  "pedido-do-autor": "o autor pediu a retirada",
  "migracao-que-corrompe": "uma migração dele corrompia os dados guardados",
};

/**
 * Por que um pacote instalado não serve, depois do «não serve:» da gestão de
 * MODs (tarefa 13 do plano da F3, Step 6b).
 *
 * As chaves são os nomes de `refusal_name` (`crates/seele-core/src/mods.rs`),
 * e o guarda `todo_pacote_recusado_e_dito_com_uma_frase` cobra uma frase para
 * cada um. Antes, a gestão dizia o nome cru — «não serve: empty» —, e um
 * código não diz a ninguém o que fazer. Um nome que chegue sem frase continua
 * aparecendo cru, como em `MOTIVOS_DE_REVOGACAO`: feio, e honesto.
 */
/**
 * **As APIs de MOD que o Servidor 0.15 roda**: a linha `0.15` de
 * `APIS_POR_LINHA` (`crates/seele-proto/src/mods.rs`). Escritas aqui uma vez
 * só, para as três frases que as dizem (o `api-too-old`, o `ModsDeApiAntiga` e
 * a linha do pacote no cache, em `camada-mods.js`). O JavaScript não tem ponte
 * para a tabela, e a bancada `hospedar-com-mod-antigo.cjs` confere esta cópia
 * contra ela. Uma API mais velha que todas estas nenhum servidor de hoje roda.
 */
const APIS_DO_SERVIDOR_0_15 = Object.freeze([3, 4, 5]);
/** As mesmas, como as frases as dizem: «3 a 5». */
const APIS_DO_SERVIDOR_0_15_DITAS = `${APIS_DO_SERVIDOR_0_15[0]} a ${APIS_DO_SERVIDOR_0_15.at(-1)}`;

const RECUSAS_DE_PACOTE = {
  "malformed": "o mod.json dele não é um JSON que este app lê, ou tem uma chave que ele não conhece",
  "schema-too-new": "o mod.json dele é de um formato mais novo que este app; atualize o aplicativo",
  "api-too-new": "ele é de uma API mais nova que a deste app; atualize o aplicativo",
  "api-too-old": `ele é de uma API mais velha que a 6, a deste app; procure a versão dele para a API 6, ou use o Servidor 0.15 pelo launcher, que roda as APIs ${APIS_DO_SERVIDOR_0_15_DITAS}`,
  "malformed-id": "o identificador dele não é «autor/nome»",
  "empty": "ele não declara nem a metade de janela nem a de servidor, e não faria nada",
  "tema-sem-metade-de-servidor": "ele declara a porta do tema sem a metade de servidor, que é quem pinta a aparência: o tema nunca apareceria",
  "arquivo-invalido": "ele declara um arquivo fora da pasta do pacote",
  "arquivos-demais": "ele declara mais arquivos do que o app carrega",
};

const NOMES_RECUSADOS = {
  Vazio: "ESTE NOME É SÓ ESPAÇO.\nDeixe o campo em branco para usar o endereço numérico de sempre.",
  ComEspaco:
    "UM NOME DE SERVIDOR NÃO PODE TER ESPAÇO.\nQuem receber o link o veria partido no meio da mensagem.",
  ComEsquema:
    "TIRE O «http://» DA FRENTE.\nAqui vai só o nome — casa.exemplo.br —, sem o que o navegador põe na barra.",
  ComBarra:
    "TIRE O QUE VEM DEPOIS DA BARRA.\nAqui vai o endereço do servidor, e não um caminho dentro dele.",
  ComCaractereQueNaoServe:
    "ESTE NOME TEM CARACTERE QUE ENDEREÇO NÃO TEM.\nValem letras, números, hífen e ponto — e os dois-pontos da porta.",
};

/**
 * A etapa de uma ligação, com o nome de quem é chamado (tarefa 28 da F7; spec de
 * 23/09, «Erros e frases»), ou `null` para a frase de sempre.
 */
function fraseDaEtapaDaLigacao(etapa, nome) {
  if (etapa && typeof etapa === "object" && etapa.Tocando) {
    return `CHAMANDO ${nomeNaFrase(nome)}…`;
  }
  return null;
}

/** O nome de quem é chamado, como as frases o escrevem. */
function nomeNaFrase(nome) {
  return String(nome ?? "").toLocaleUpperCase("pt-BR");
}

/**
 * **A frase de quem liga** (tarefa 28 da F7; spec de 23/09, «Erros e frases»,
 * quem liga), com o nome de quem foi chamado, ou `null` quando o desfecho não é
 * de uma ligação e a frase de sempre (`fraseDeErro`) serve.
 *
 * A trilha decide uma linha só: «está no ar, mas o caminho não abriu» é quando o
 * ponto de encontro deu onde a sala mora (houve tentativa) e nenhum candidato
 * respondeu.
 */
function fraseDaLigacao(erro, nome, trilha) {
  const quem = nomeNaFrase(nome);
  const razao = erro?.Refused?.reason;
  if (razao === "NinguemAtendeu") return `${quem} NÃO ATENDEU`;
  if (razao === "ChamadaRecusada") return `${quem} RECUSOU`;
  if (razao === "ChamadaEncerrada") return `${quem} ENCERROU A LIGAÇÃO`;
  if (razao === "RateLimited") return `ESPERE UM POUCO PARA LIGAR DE NOVO PARA ${quem}`;
  if (razao === "ServerFull") return `A LIGAÇÃO DE ${quem} ESTÁ CHEIA (6 PESSOAS)`;
  const localizar = erro?.NaoLocalizou?.motivo;
  if (localizar === "PontoMudo" || localizar === "PontoNaoResolve") {
    return (
      "O PONTO DE ENCONTRO NÃO RESPONDEU: sem ele, código não funciona; " +
      "link com endereço continua funcionando"
    );
  }
  if (localizar === "NinguemMora") {
    return (
      `${quem} NÃO ESTÁ DISPONÍVEL: o SEELE do outro lado está fechado ` +
      "ou não está recebendo ligações"
    );
  }
  if (erro?.CodigoNaoBate) return "QUEM ATENDEU NÃO É O DONO DESTE CÓDIGO. Nada foi enviado.";
  if (erro?.SePassouPorCodigo) return `ALGUÉM TENTOU SE PASSAR POR ${quem}`;
  if (erro?.OutraLinha) return `${quem} USA UMA VERSÃO QUE NÃO CONVERSA COM A SUA`;
  const semCaminho = erro === "Unreachable" || erro === "SemResposta" || erro === "HandshakeTimeout";
  const tentou =
    Array.isArray(trilha) &&
    trilha.some((passo) => passo?.etapa && typeof passo.etapa === "object" && passo.etapa.Tentando);
  if (semCaminho && tentou) return `${quem} ESTÁ NO AR, MAS O CAMINHO NÃO ABRIU`;
  return null;
}

function fraseDeErro(erro, apelido, deOnde) {
  if (typeof erro === "string") return FRASES[erro] ?? erro;
  if (erro && typeof erro === "object") {
    if (erro.Recusado?.motivo) {
      const motivo = erro.Recusado.motivo;
      // Um formato que o produto reconhece e não aceita, pelo nome dele: as
      // Fotos do iPhone entregam HEIC (rascunho da API 6, §7.1).
      if (motivo === "tipo-recusado:heic") {
        return "Este arquivo é HEIC, que o SEELE não lê. Use uma imagem PNG, JPEG, WebP ou GIF.";
      }
      return {
        "arquivo-grande-demais": "Escolha uma imagem de até 10 MB.",
        "midia-grande-demais": "A imagem excede 10 MB.",
        "arquivo-ilegivel": "Não foi possível ler o arquivo escolhido.",
        "formato-desconhecido": "Use uma imagem PNG, JPEG, WebP ou GIF.",
        "exigido-por-este-servidor": "Desligue o MOD e salve antes de apagar.",
        // A escolha de quem pinta o servidor (tarefa 17f.0 da F3): o servidor
        // confere de novo o que a tela mostrou, e nada mudou.
        "mod-desligado": "Este MOD não está ligado neste servidor: ligue-o e salve antes de escolhê-lo.",
        "sem-porta-de-tema": "Este MOD não declara a porta do tema, e não pode pintar a aparência do servidor.",
        // Apagar um pacote do cache (`FalhaAoApagarPacote`, em `mods.rs`).
        "nao-esta-aqui": "Este pacote já não estava no cache.",
        "nao-apaguei": "O disco não deixou apagar este pacote, e o motivo está no seele.log. Feche o que usa o MOD e tente de novo.",
        "hash-torto": "Este pedido não nomeia um pacote do cache, e nada foi apagado.",
      }[motivo] ?? motivo;
    }
    // **Um MOD ligado que este app não roda** (o corte da API 6; tarefa 13 do
    // plano da F3, Step 5b): o servidor não sobe calado. Cada MOD com o nome e
    // a API que o pacote declara; o que não tem o pacote nesta máquina sai sem
    // número — «API 0» seria «não sei» escrito como um fato. As três saídas são
    // as do dono (08/10), nesta ordem.
    if (erro.ModsDeApiAntiga) {
      const mods = erro.ModsDeApiAntiga.mods ?? [];
      // Zero também é «não sei»: o Rust manda `None`, e um zero que chegasse
      // por outro caminho não vira número escrito.
      // Uma API fora das do Servidor 0.15 nenhum servidor de hoje roda, e a
      // saída de hospedar por ele só vale para quem ele roda (lote 58-1,
      // rodada 2).
      const ditos = mods.map(([id, api]) => (!api
        ? `${id}: o pacote deste MOD não está nesta máquina — baixe-o de novo, ou desligue o MOD.`
        : APIS_DO_SERVIDOR_0_15.includes(api)
          ? `${id}: feito para a API ${api}, que roda no Servidor 0.15.`
          : `${id}: feito para a API ${api}, que nenhum servidor de hoje roda.`));
      const algumComApi = mods.some(([, api]) => Boolean(api));
      const algumDo015 = mods.some(([, api]) => APIS_DO_SERVIDOR_0_15.includes(api));
      return (
        "ESTE SERVIDOR TEM MOD QUE ESTE APP NÃO RODA.\n" +
        ditos.join("\n") +
        (algumComApi
          ? "\nAtualize o MOD para a versão da API 6; " +
            (algumDo015 ? "ou hospede pelo Servidor 0.15, no launcher; " : "") +
            "ou desligue o MOD e hospede sem ele — os dados dele ficam."
          : "")
      );
    }
    // **As saídas de quem hospeda com um MOD de API antiga**, as recusas com
    // campo (`FalhaAoResolverModAntigo`, em `mods.rs`; as sem campo estão no
    // `FRASES`). O motivo de um pacote que não serve é um nome: os do núcleo
    // têm frase em `RECUSAS_DE_PACOTE`, e dois vêm da leitura por hash.
    if (erro.PacoteNaoServe) {
      const motivo = erro.PacoteNaoServe;
      const porque = {
        "conteudo-de-outro-mod": "o pacote baixado é de outro MOD",
        "conteudo-nao-bate": "o conteúdo do pacote não é o que o catálogo publicou",
      }[motivo] ?? RECUSAS_DE_PACOTE[motivo] ?? motivo;
      return `A VERSÃO NOVA DESTE MOD NÃO SERVE: ${porque}.\nO MOD continua como estava.`;
    }
    // A de unidade com o mesmo nome é da `FalhaNoMod`, e chega como texto: esta
    // chega como objeto, com o que o banco disse.
    if (typeof erro.BancoNaoRespondeu === "string") {
      return (
        `O BANCO DESTE SERVIDOR NÃO RESPONDEU: ${erro.BancoNaoRespondeu}.\n` +
        "O MOD continua como estava."
      );
    }
    // ADR 0046: esta versão foi revogada. A frase diz o defeito e o conserto,
    // como a lista assinada os escreve — nunca uma recusa genérica. Serve a
    // `FalhaAoHospedar` e a `conferir_esta_versao`, que usam o mesmo nome.
    if (erro.EstaVersaoRevogada) {
      const { defeito, corrigida_em: corrigida } = erro.EstaVersaoRevogada;
      return (
        "ESTA VERSÃO DO SEELE FOI REVOGADA.\n" +
        `O defeito: ${defeito}\n` +
        (corrigida
          ? `A ${corrigida} corrige. Atualize antes de hospedar ou de entrar.`
          : "Atualize antes de hospedar ou de entrar.")
      );
    }
    if (erro.PinChanged) {
      return (
        "A CHAVE DO SERVIDOR MUDOU.\n" +
        `fixada:   ${erro.PinChanged.pinned}\n` +
        `ofertada: ${erro.PinChanged.offered}\n` +
        "Confirme por outro canal antes de continuar."
      );
    }
    // **O apelido gravado que o `Hello` não leva** (pendência 58, item 55;
    // lote C55 da F5). A FFI o recusa antes de discar, com o teto em bytes; era
    // NADA RESPONDEU NESSE ENDEREÇO, com o servidor de pé. O remédio é do
    // perfil, e a tentativa seguinte lê o apelido de lá (`conectar` esquece o
    // recusado).
    if (erro.ApelidoGrandeDemais) {
      const teto = erro.ApelidoGrandeDemais.limite_bytes;
      return (
        `SEU APELIDO PASSA DE ${teto} BYTES.\n` +
        "Abra o PERFIL, encurte o apelido e entre de novo: uma letra acentuada ocupa dois bytes, e um ideograma, três."
      );
    }
    // A impressão prometia uma chave e quem atendeu ofertou outra. Não é
    // troca de chave — nenhum pino prova o servidor ali — então a frase acusa
    // quem prometeu, e não a continuidade do servidor.
    //
    // **A recusa é dentro do TLS, antes do `Hello`** (S2b da análise de 22/09):
    // o verificador do núcleo confere a impressão no aperto de mão, e o convite,
    // a senha e o apelido não chegam a quem atendeu com a chave errada. A frase
    // diz isso porque muda o que a pessoa faz: esse servidor não ficou com o
    // convite, e não há por que pedir outro por causa dele.
    //
    // **Diz só isso: nada de «nada», de «nenhum» nem de «esta máquina».**
    // - O `ClientHello` do TLS (nome, ALPN, chave efêmera) sai antes de haver o
    //   que conferir.
    // - Os candidatos de um convite correm em paralelo, cada um com o próprio
    //   `Hello` depois do próprio TLS, e quando todos falham o núcleo mostra a
    //   primeira falha de quem respondeu, na ordem de conclusão. Um candidato
    //   que recusou no TLS pode terminar antes de outro que já entregou o
    //   convite, a senha e o apelido ao servidor verdadeiro. O escopo que
    //   sempre vale é «a ele»: a quem atendeu com a chave errada.
    //
    // Vale também para quem volta pela lista de servidores, que confere pela
    // impressão que a lista guardou (S3). É `#boot-erro`, e não o veredito
    // laranja da sessão, porque não há sessão nenhuma.
    //
    // **E quem prometeu muda a frase.** `deOnde` (`deOndeVeio`, em
    // `tela-boot.js`) diz se a impressão veio do link colado ou da lista. Da
    // lista, não há link nenhum a confirmar com ninguém: o título fala da
    // lista, e o remédio é colar de novo o link do servidor (a volta confere
    // pela impressão do link, e a lista passa a guardar a que ele confirmou) ou
    // removê-lo da lista. Serve a uma lista gravada pela 0.15.0 com a impressão
    // de outro servidor, que passa a ser recusada em todo endereço onde nenhum
    // pino prova o servidor (um alvo de LAN, um endereço sem pino), e a um
    // servidor que trocou de chave. **O remédio não diz que a lista está
    // errada**, porque nem sempre está: numa colisão de LAN (outro servidor,
    // já fixado, atendendo no endereço de casa do anfitrião) ou num quarto
    // hostil, a lista guardou a impressão certa, e quem atendeu é que não é o
    // servidor dela. Por isso ele diz quando colar o link. Sem `deOnde`, a
    // frase é a do link.
    if (erro.InviteMismatch) {
      const comparacao =
        `esperada: ${erro.InviteMismatch.expected}\n` +
        `ofertada: ${erro.InviteMismatch.offered}\n`;
      if (deOnde?.origem === "Lista") {
        return (
          "ESTE NÃO É O SERVIDOR QUE A LISTA GUARDOU.\n" +
          comparacao +
          "O convite, a senha e o apelido não chegaram a ele.\n" +
          "Se a chave do servidor mudou, ou a lista guardou a errada, cole de novo o link dele; ou remova-o da lista."
        );
      }
      return (
        "ESTE NÃO É O SERVIDOR DO CONVITE.\n" +
        comparacao +
        "O convite, a senha e o apelido não chegaram a ele.\n" +
        "Confirme o link com quem o mandou."
      );
    }
    // **O servidor fala uma versão que este app não fala** (P8 do plano F1b;
    // ADR 0058). Até a 0.15 isto era NADA RESPONDEU NESSE ENDEREÇO, de um
    // servidor que tinha respondido, e respondido justamente a versão dele, no
    // primeiro byte da resposta. A frase diz a versão do servidor, e não o
    // número do fio.
    //
    // **Pelos dois campos que a FFI manda, e não pela forma do texto da
    // versão** (decisão da orquestradora de 15/10):
    // - `servidor`, só quando é uma versão que o launcher abre («0.15»);
    // - `mais_nova`, quando o número está acima do deste app: toda 1.x fala o
    //   primeiro número da 1.0 (ADR 0058), então quem o recusa é mais novo, e o
    //   remédio é atualizar;
    // - nenhum dos dois: um número de antes da 0.15, que nenhum app oferecido
    //   fala.
    if (erro.OutraLinha) {
      const { servidor, mais_nova: maisNova } = erro.OutraLinha;
      if (servidor) {
        return (
          `ESTE SERVIDOR É O SERVIDOR ${servidor}.\n` +
          `Esta versão do app não conversa com ele. Para entrar, use o SEELE ${servidor}.`
        );
      }
      if (maisNova) {
        return (
          "ESTE SERVIDOR É DE UMA VERSÃO MAIS NOVA DO SEELE.\n" +
          "Atualize o app para entrar nele."
        );
      }
      return (
        "ESTE SERVIDOR É DE UMA VERSÃO ANTIGA DO SEELE.\n" +
        "Esta versão do app não conversa com ele, e nenhuma versão oferecida hoje conversa."
      );
    }
    // **O servidor exige MODs que esta máquina não aceitou.** ADR 0045.
    //
    // Não é uma falha: é uma pergunta, e ela tem tela (`camada-mods.js`). O
    // `catch` de `conectar` leva à pergunta toda recusa destas da tentativa
    // mais nova, e cala a de uma tentativa que outra já substituiu, que vai ao
    // `seele.log` (lote W2 da F5). Medido no lote W2, nenhum caminho de hoje
    // chega aqui. O ramo fica para o caminho que um dia chegar: sem ele, a
    // lista viraria a frase genérica do fim desta função, e «o produto sabe e
    // não conta» é o defeito que mais caro custou neste repositório.
    //
    // **A última linha diz o que esta versão faz**, e não o que faltava nela.
    // Ela dizia «A tela para ler e aceitar esta lista ainda não existe neste
    // app.», escrita antes da tela e falsa desde que ela existe. Entrar de novo
    // é uma tentativa nova, e a tentativa mais nova abre a pergunta: é o que a
    // pessoa pode fazer, e a `ajustes-v013.cjs` confere que funciona.
    //
    // Cada MOD sai com o que o ADR 0045 manda mostrar antes de qualquer byte
    // ser baixado: quem é, de onde veio, e o que alcança. A linha de `world` é
    // separada das outras de propósito — ela não é mais um alcance na lista, é
    // código de terceiro rodando na máquina de outra pessoa.
    if (erro.ModsNaoAceitos) {
      const lista = (erro.ModsNaoAceitos.mods ?? [])
        .map((mod) => {
          const linhas = [`• ${mod.id} ${mod.version}`, `  repositório: ${mod.repo || "não declarado"}`];
          linhas.push(
            `  alcança: ${mod.reach && mod.reach.length ? mod.reach.join(", ") : "nada declarado"}`,
          );
          if (mod.no_servidor) {
            linhas.push(
              "  roda na máquina de quem hospeda: rede de saída, relógio e registro",
            );
          }
          if (mod.tema) {
            linhas.push("  pode mudar a aparência deste servidor para todos que entram");
          }
          return linhas.join("\n");
        })
        .join("\n");
      return (
        "ESTE SERVIDOR EXIGE MODS QUE VOCÊ AINDA NÃO ACEITOU.\n" +
        lista +
        "\nPara aceitar ou recusar esta lista, entre de novo neste servidor."
      );
    }
    // **O nome público que não pode virar link**, e cada recusa com o conserto
    // dentro. «Inválido» não diz o que fazer, e este campo é digitado à mão por
    // alguém que acabou de configurar um DNS: a chance de errar por um detalhe
    // é alta, e o detalhe é sempre um só.
    // As recusas do catálogo que carregam detalhe dentro.
    // A de um MOD que o catálogo entregou certo e esta máquina não instalou
    // (pendência 58, item 9) é a recusa de instalar: nem o catálogo nem a rede.
    if (erro.NaoInstalou) {
      const dentro = erro.NaoInstalou;
      // Um MOD do catálogo não vem de pasta que alguém escolheu: a de passagem
      // já foi apagada, e «desta pasta» apontaria para lugar nenhum.
      if (dentro.ManifestoRecusado) {
        return (
          `ESTE SEELE RECUSOU O «mod.json» DESTE MOD: ${RECUSAS_DE_PACOTE[dentro.ManifestoRecusado] ?? dentro.ManifestoRecusado}.\n` +
          "Nada foi instalado."
        );
      }
      return fraseDeErro(dentro, apelido, deOnde);
    }
    if (erro.NaoRespondeu) {
      return (
        "O CATÁLOGO NÃO RESPONDEU.\n" +
        "Pode ser a sua rede, ou o indexador fora do ar. Tente de novo mais tarde."
      );
    }
    if (erro.NaoEUmCatalogo) {
      return (
        "O CATÁLOGO VEIO QUEBRADO.\n" +
        "A assinatura passou, então ele é nosso — e o conteúdo não é o que este " +
        "SEELE sabe ler. Avise quem publica."
      );
    }
    if (erro.MaisNovoQueEsteBuild) {
      return (
        "ESTE SEELE É VELHO DEMAIS PARA ESTE CATÁLOGO.\n" +
        "Nada de errado com o catálogo: atualize o SEELE em CONFIGURAÇÕES."
      );
    }
    if (erro.NaoAplicou) {
      // O pacote novo está no disco; o que não mudou foi o que o servidor
      // exige. Dizer as duas coisas, porque o próximo passo depende de saber
      // qual delas ficou para trás.
      return (
        "A VERSÃO NOVA FOI BAIXADA, MAS O SERVIDOR CONTINUA EXIGINDO A ANTERIOR. " +
        "DESLIGUE E LIGUE O MOD PARA APLICÁ-LA."
      );
    }
    if (erro.NaoEstaNoCatalogo) {
      return `O CATÁLOGO NÃO LISTA «${erro.NaoEstaNoCatalogo}».`;
    }
    if (erro.ConteudoDivergente) {
      return (
        "OS ARQUIVOS QUE CHEGARAM NÃO SÃO OS QUE O CATÁLOGO DECLARA.\n" +
        "Quase sempre é um download que veio pela metade — tente de novo.\n" +
        `esperado: ${erro.ConteudoDivergente.esperado}\n` +
        `obtido:   ${erro.ConteudoDivergente.obtido}`
      );
    }
    // **A revogação não é falha de rede: é um aviso de segurança.** Ela diz o
    // defeito e, quando há, onde ele foi consertado — porque saber que não pode
    // instalar sem saber o que fazer é meia resposta.
    if (erro.Revogada) {
      const conserto = erro.Revogada.corrigido_em
        ? `\nCorrigido na ${erro.Revogada.corrigido_em}: instale aquela.`
        : "\nAinda não há versão corrigida.";
      return (
        "ESTA VERSÃO FOI RETIRADA DE CIRCULAÇÃO.\n" +
        `Motivo: ${MOTIVOS_DE_REVOGACAO[erro.Revogada.motivo] ?? erro.Revogada.motivo}` +
        conserto
      );
    }
    // **Instalar um MOD de uma pasta**, as recusas que trazem detalhe — a
    // pendência 53. A do atalho é a que mais se encontra: a pasta de quem
    // desenvolve o MOD tem `node_modules/.bin/`, cheia de atalhos, e a recusa
    // está certa (um atalho publicaria como do MOD um arquivo de outro lugar do
    // disco). Ela chegava aqui como `NaoCopiei`, junto das falhas do disco, e
    // caía em «ALGO FALHOU» com o JSON cru. Agora diz qual é o atalho e o que
    // fazer. O caminho também fica no `seele.log`.
    //
    // O remédio é tirar o atalho, onde quer que ele esteja: mandar instalar
    // «sem node_modules» apontava o conserto errado para um `assets/x → ../y`.
    // `node_modules` fica como o lugar mais provável, e não como a regra.
    if (erro.TemAtalho) {
      return (
        `ESTA PASTA TEM UM ATALHO, E O SEELE NÃO INSTALA MOD COM ATALHO: ${erro.TemAtalho}.\n` +
        "Tire o atalho e instale de novo. Num MOD em desenvolvimento, os atalhos costumam estar em «node_modules»."
      );
    }
    if (erro.ManifestoRecusado) {
      return (
        `ESTE SEELE RECUSOU O «mod.json» DESTA PASTA: ${RECUSAS_DE_PACOTE[erro.ManifestoRecusado] ?? erro.ManifestoRecusado}.\n` +
        "Nada foi instalado."
      );
    }
    if (erro.NaoCopiei) {
      return (
        `NÃO CONSEGUI COPIAR ESTE MOD PARA O SEELE: ${erro.NaoCopiei}.\n` +
        "Nada foi instalado. Confira o espaço e as permissões do disco, e tente de novo."
      );
    }
    if (erro.NomeRecusado) {
      return NOMES_RECUSADOS[erro.NomeRecusado] ?? "ESTE NOME NÃO PODE VIRAR LINK";
    }
    if (erro.Refused) {
      // **O apelido recusado entra na frase**, e é conserto de uma coisa cruel.
      //
      // A recusa por nome tomado é a única desta lista que a pessoa conserta
      // sozinha, e a frase mandava «escolha outro apelido» sem dizer qual foi
      // recusado. Quando o nome é o recurso automático — que ninguém digitou —,
      // ela lia uma ordem sobre um nome que nunca escolheu, e não tinha como
      // saber disso. Foi assim que um relato de campo virou meia hora de
      // «troquei o nick várias vezes e continua dando o mesmo erro».
      //
      // O servidor já manda o nome no detalhe da recusa, e ele não atravessa a
      // ponte. A casca, essa, sabe exatamente o que mandou — dizer daqui custa
      // um argumento e não um campo novo no fio.
      if (erro.Refused.reason === "NicknameTaken" && apelido) {
        return (
          `O APELIDO «${apelido}» JÁ É DE OUTRA PESSOA NESTE SERVIDOR.\n` +
          "Não é o convite nem a senha: escolha outro apelido e entre de novo."
        );
      }
      return MOTIVOS[erro.Refused.reason] ?? "SESSÃO RECUSADA";
    }
  }
  return FRASES[erro] ?? desconhecida(erro);
}

/**
 * A frase para uma falha que este arquivo não sabe nomear.
 *
 * Ela **diz o que era**, e isso não é preguiça de escrever a frase certa: é o
 * reconhecimento de que a lista acima vai ficar para trás. O Rust ganha
 * variantes de erro — três entraram só hoje — e a cada uma que chega sem frase
 * a tela escrevia "FALHA DESCONHECIDA", que é um beco sem saída para quem lê e
 * para quem conserta. Uma pessoa relatando "não consigo reconectar" não tinha o
 * que me contar além disso.
 *
 * O conteúdo é seguro de mostrar: os erros que atravessam esta ponte são enums
 * de protocolo e endereços, nunca segredo — o convite e a chave nunca viram
 * erro, viram veredito.
 */
function desconhecida(erro) {
  let detalhe;
  try {
    // **`Error` e `DOMException` primeiro, e é o caso que aparecia como `{}`.**
    //
    // `JSON.stringify` de um erro do navegador devolve o objeto vazio: `name` e
    // `message` são propriedades do protótipo, e o `stringify` só enumera as
    // próprias. Toda falha de `VideoDecoder`, de área de transferência ou de
    // mídia chegava à tela como duas chaves e nada dentro — o pior detalhe
    // possível, porque parece que o app tem a informação e não a mostra.
    //
    // Foi relatado em campo assim: «o erro de compartilhar tela trouxe {}».
    if (erro instanceof Error || (erro && typeof erro.message === "string")) {
      detalhe = [erro.name, erro.message].filter(Boolean).join(": ") || String(erro);
    } else {
      detalhe = typeof erro === "object" ? JSON.stringify(erro) : String(erro);
    }
  } catch {
    // Um objeto com ciclo. Raro, e ainda assim melhor dizer o tipo que nada.
    detalhe = Object.prototype.toString.call(erro);
  }
  // Sem a palavra «linha» aqui: neste produto ela é o canal de texto, e o
  // guarda do ADR 0033 reprova o uso coloquial — com razão, porque quem lê
  // aprendeu o outro sentido na tela ao lado.
  return `ALGO FALHOU E ESTE APP NÃO SABE EXPLICAR O QUÊ.\nSe for relatar, copie o texto abaixo:\n${detalhe}`;
}

/**
 * A frase de um campo que passa do teto do fio (pendência 58, item 55).
 *
 * Diz o teto e o que fazer, e diz quantos bytes o texto tem: o teto é em
 * **bytes**, e quem escreve conta letras. Vinte letras `á` são 40 bytes, e sem
 * o número quem lê conta vinte, acha que cabe em 32, e não entende a recusa.
 * A segunda linha é a explicação que falta a essa conta.
 */
function fraseDoTeto(teto, bytes) {
  return (
    `PASSA DE ${teto} BYTES — ENCURTE.\n` +
    `Este tem ${bytes}: uma letra acentuada ocupa dois, e um ideograma, três.`
  );
}

/**
 * Enum → frase. A fronteira erro→texto do produto fica aqui, e é por isso que
 * nenhuma mensagem para gente é escrita em Rust.
 */
const FRASES = {
    NotConnected: "SEM CONEXÃO",
    AlreadyConnected: "JÁ HÁ UMA SESSÃO ABERTA",
    UnresolvableHost: "NÃO CONSEGUI RESOLVER ESSE ENDEREÇO",
    Unreachable: "NADA RESPONDEU NESSE ENDEREÇO",
    // **Duas frases onde havia uma.**
    //
    // As duas falhas usavam esta, e ela descreve bem só a primeira: um servidor
    // que recebeu o pedido e demorou a responder. A segunda — os pacotes saírem
    // e nada voltar — não é sincronização nenhuma, e chamá-la assim mandou quem
    // leu conferir protocolo, versão e servidor. Perguntado assim: «o Mac só dá
    // tempo esgotado na sincronização. Como isso, se os PCs estão na mesma LAN e
    // na mesma versão?» — e as duas coisas estavam certas.
    //
    // A segunda linha diz onde olhar, porque a causa quase sempre é a mesma: no
    // Windows, quem hospeda precisa da regra de firewall, e ela nasce
    // desmarcada no instalador.
    HandshakeTimeout:
        "O SERVIDOR RECEBEU E NÃO RESPONDEU A TEMPO\n" +
        "Ele está no ar e demorou. Tente de novo.",
    // **Dois sistemas numa frase só**, pela mesma razão que a recusa de gravar
    // tela nomeia os dois: eles negam coisas diferentes, e quem lê está num
    // deles. Antes esta linha nomeava só o firewall de quem hospeda, e mandava
    // consertar a máquina certa metade das vezes — no macOS o silêncio no
    // caminho da LAN é desta máquina, que recusa calada o que sai daqui para a
    // própria casa até alguém dizer que sim nos Ajustes.
    SemResposta:
        "OS PACOTES SAÍRAM E NADA VOLTOU\n" +
        "Confira o endereço e a porta UDP de quem hospeda. No Mac, confira também " +
        "a permissão de rede local em Ajustes.",
    // **O impostor que repete o certificado do servidor**, e que chegava aqui
    // como versão incompatível: a frase mandava atualizar o SEELE nas duas
    // máquinas. O certificado é público; a chave que o assina, não, e o TLS a
    // cobrou antes do `Hello`. Por isso a segunda linha diz o que não saiu,
    // com o escopo «a ele» da recusa pela impressão (ver `InviteMismatch`, em
    // `fraseDeErro`).
    // Quem liga desistiu antes de entrar (tarefa 16 da F7). A casca sabe que
    // foi ela quem cancelou; a frase existe para nunca sair o nome cru.
    Cancelada: "VOCÊ CANCELOU A LIGAÇÃO",
    ChaveNaoProvada:
        "QUEM ATENDEU NÃO PROVOU SER ESTE SERVIDOR.\n" +
        "Ele mostrou o certificado do servidor sem a chave que o assina. " +
        "O convite, a senha e o apelido não chegaram a ele.",
    IdentityUnavailable: "NÃO CONSEGUI LER OU GRAVAR A IDENTIDADE EM DISCO",
    NoAudioDevice: "SEM DISPOSITIVO DE ÁUDIO",
    UnknownPerson: "NÃO CONHEÇO ESSA PESSOA",
    UnknownChannel: "NÃO CONHEÇO ESSE CANAL",
    LinkLost: "ENLACE PERDIDO",

    // A recusa de quem apertou COMPARTILHAR com outra pessoa já transmitindo.
    // **Não** é permissão que falta, e a segunda linha é o que separa as duas:
    // esta pessoa pode compartilhar, só não agora — mandá-la procurar um papel
    // que ela já tem seria mandá-la procurar o que não existe.
    ScreenShareTaken:
      "ALGUÉM JÁ ESTÁ COMPARTILHANDO A TELA NESTA SALA.\n" +
      "Cabe uma por vez: dá para começar assim que a pessoa parar.",

    // A metade que captura a tela ainda não está neste executável, e a frase
    // não manda tentar de novo por isso: não há nada que quem lê possa fazer
    // para mudar a resposta. Dizer que não é a conexão nem permissão é o que
    // impede a caça ao defeito que não existe — foi o que a frase de
    // `NadaPublicado` aprendeu, dois recursos atrás.
    ScreenShareUnavailable:
      "ESTA VERSÃO NÃO SABE COMPARTILHAR TELA.\n" +
      "Não é a sua conexão nem permissão: a parte que captura a tela ainda não está neste app.",

    // O arquivo veio e não ficou. É a única das três em que tentar de novo
    // resolve às vezes, e por isso é a única que manda tentar — e, quando não
    // resolver, a frase já diz onde olhar em vez de deixar a pessoa procurando.
    ScreenModuleRefused:
      "O MÓDULO DE VÍDEO NÃO INSTALOU.\n" +
      "Ou o download veio quebrado, ou a pasta de configuração não aceitou o arquivo. " +
      "Tente de novo; se insistir, é a pasta.",

    // Separada da de cima porque elas mandam a pessoa fazer coisas diferentes,
    // e enquanto foram uma só quem lia parava de tentar: a de cima diz que o
    // recurso não existe, e esta diz que falta um arquivo.
    //
    // O módulo do OpenH264 não vem no pacote por licença — a cobertura de
    // patente do Cisco acompanha o binário que o Cisco entrega. A frase diz o
    // tamanho porque um megabyte é uma decisão fácil de tomar, e «baixar algo
    // da internet» sem número é uma difícil.
    ScreenModuleMissing:
      "FALTA O MÓDULO DE VÍDEO.\n" +
      "Este app captura a tela, e o codec que comprime a imagem não vem junto: " +
      "a licença dele não deixa. São cerca de 1 MB, baixados uma vez.",

    // Por que um texto colado não é um convite. O Rust devolve o nome da
    // falha; a frase é daqui, como todas as outras.
    EsquemaDesconhecido: "ISTO NÃO PARECE UM CONVITE SEELE",
    SemEndereco: "ESTE CONVITE NÃO TRAZ ENDEREÇO NENHUM",
    EnderecoInvalido: "O ENDEREÇO DENTRO DESTE CONVITE NÃO É UM ENDEREÇO",
    // Frase própria, e não o `EnderecoInvalido` acima, porque esta falha tem
    // conserto na mão de quem lê: falta pontuação, não falta endereço. Mandar
    // procurar um caractere errado seria mandar procurar o que não existe.
    EnderecoIpv6SemColchetes:
      "FALTAM OS COLCHETES NESTE ENDEREÇO IPV6.\nEle vai assim: seele://[2001:db8::1]:8383",
    // Degrau 4 do ADR 0022: o `enc` do link veio pela metade. Frase própria e
    // não `EnderecoInvalido` porque o que falta é outra coisa, e porque o resto
    // do link continua bom — o que se perde é o furo de NAT, não o servidor.
    BilheteInvalido:
      "ESTE CONVITE CHEGOU CORTADO NO FIM.\n" +
      "Dá para entrar por ele na mesma rede; de fora, peça o link inteiro de novo.",
    ImpressaoDigitalInvalida: "ESTE CONVITE CHEGOU CORTADO OU ADULTERADO",
    TokenInvalido: "O CONVITE DENTRO DESTE LINK NÃO É UM CONVITE",
    VoiceRoomInvalido: "A SALA DE VOZ DESTE CONVITE NÃO É UM NÚMERO",

    // Hospedar aqui dentro.
    JaHospedando: "JÁ ESTOU HOSPEDANDO NESTA JANELA",
    PortaOcupada:
      "A PORTA 8383 JÁ ESTÁ EM USO.\nQuase sempre é outro SEELE aberto — feche o outro e tente de novo.",
    NaoSubiu: "NÃO CONSEGUI SUBIR O SERVIDOR AQUI",
    // ADR 0046: a lista de revogação que chegou não abriu, ou é mais velha que
    // a guardada nesta máquina — a forma de alguém esconder uma revogação é
    // servir uma lista antiga. O ato para em vez de seguir calado. A outra
    // recusa, a da versão revogada, traz o defeito e o conserto e é montada em
    // `fraseDeErro`, que é onde as recusas com campos são lidas.
    ListaDeRevogacaoRecusada:
      "A LISTA DE VERSÕES REVOGADAS QUE CHEGOU NÃO CONFERE.\n" +
      "Ela não abriu, ou é mais antiga que a que esta máquina já viu. Tente de novo mais tarde.",

    // O catálogo de MODs — ADR 0045.
    //
    // `AssinaturaRecusada` é a única desta lista em que a frase manda **parar**.
    // As outras convidam a tentar de novo, e esta não pode: um catálogo que não
    // passa na assinatura é um catálogo adulterado ou um endereço sequestrado,
    // e insistir é exatamente o comportamento que o atacante quer.
    AssinaturaDoCatalogoRecusada:
      "O CATÁLOGO NÃO É O NOSSO.\n" +
      "A assinatura não confere com a chave deste SEELE. Não tente de novo: " +
      "avise quem publica.",

    // As saídas de quem hospeda com um MOD de API antiga, com o servidor parado
    // (`FalhaAoResolverModAntigo`, em `mods.rs`; tarefa 13 do plano da F3,
    // Step 5b). As que trazem o motivo são montadas em `fraseDeErro`.
    ServidorNoAr:
      "PARE A HOSPEDAGEM PRIMEIRO.\n" +
      "Com um servidor no ar nesta janela, o SEELE não atualiza nem desliga MOD de servidor guardado.",
    ServidorDesconhecido:
      "ESTE SERVIDOR NÃO ESTÁ ENTRE OS GUARDADOS DESTA MÁQUINA.\nNada foi mudado.",
    NaoEDeApiAntiga:
      "ESTE MOD JÁ NÃO IMPEDE DE HOSPEDAR.\nNada foi mudado: tente hospedar de novo.",

    // Instalar um MOD de uma pasta (`FalhaAoInstalarMod`, em `mods.rs`). A
    // única recusa sem campo; as outras três, que trazem o motivo ou o caminho,
    // são montadas em `fraseDeErro`. O engano mais provável de todos é apontar
    // para a pasta que **contém** o MOD, e a frase diz isso.
    SemManifesto:
      "ESTA PASTA NÃO TEM «mod.json».\nEscolha a pasta do MOD, e não a pasta que a contém.",

    // A portaria — ADR 0030. As duas falam de uma porta que não existe daqui.
    //
    // `NaoEstaHospedando` não é engano de quem apertou: os comandos da portaria
    // mexem no servidor **desta** máquina, e quem está no servidor de outra pessoa não
    // tem porta nenhuma para mexer. A frase diz isso em vez de deixar a pessoa
    // procurando o que fez de errado.
    NaoEstaHospedando:
      "ESTA JANELA NÃO ESTÁ HOSPEDANDO NENHUM SERVIDOR.",
    BancoNaoRespondeu: "O SERVIDOR DESTA MÁQUINA NÃO RESPONDEU",

    // Até onde o convite chega — a escada do ADR 0022. Vai junto do link, e não
    // numa tela de diagnóstico, porque é aí que a informação vale: um link que
    // só funciona na rede de casa e um link que funciona pela internet são o
    // **mesmo texto**. Sem estas frases o anfitrião manda o primeiro achando
    // que mandou o segundo, e quem descobre é o amigo, como "não conecta".
    //
    // Nenhuma promete alcance. Mesmo com a porta aberta o firewall do outro
    // lado pode recusar, e "deve funcionar" é o que dá para prometer.
    //
    // Uma frase, e uma segunda só quando ela muda o que a pessoa faz. Que o
    // link leva também o endereço da rede de casa era verdade em três destas
    // quatro e não fazia ninguém agir: quem está perto entra do mesmo jeito,
    // sem ler nada. Foi para `docs/alcance-pela-internet.md`, com o resto do
    // que estas frases carregavam — as marcas de VPN, o NAT simétrico, o
    // Tailscale.
    // Degrau 1 com endereço próprio: VPS, IP fixo, porta já encaminhada à mão.
    // O único degrau em que nada foi pedido a ninguém — nem ao roteador, nem a
    // um ponto de encontro —, e por isso o único sem ressalva nenhuma na
    // segunda linha. Nasceu de um defeito: antes dele uma VPS lia «ESTE LINK SÓ
    // FUNCIONA NA SUA REDE», que manda encaminhar a porta num roteador que não
    // existe, embaixo de um link que alcança o mundo inteiro.
    EnderecoDireto:
      "ESTA MÁQUINA TEM ENDEREÇO PRÓPRIO.",
    PortaNoRoteador:
      "O ROTEADOR ABRIU A PORTA.",
    // Degrau 4, o que faz «manda o link e funciona» valer numa casa com CGNAT
    // ou com o UPnP desligado. «Deve funcionar» como as outras: com NAT
    // simétrico dos dois lados o furo não abre, e o ADR 0022 deixou
    // retransmissão fora de escopo por decisão.
    //
    // A **única** da escada com um terceiro no meio. Um produto que se vende como
    // «sem serviço no meio» ganhou um serviço no meio, opcional, e o que ele
    // aprende é dito aqui — na tela em que o link aparece — porque a alternativa
    // é a pessoa descobrir depois.
    //
    // Este comentário dizia «a única que ainda gasta duas linhas cheias» e que
    // «isto não encolhe mais». Encolheu, em 2026-08-20, junto com as segundas
    // linhas do resto da escada. O que **não** encolheu é a divulgação: o guarda
    // `the_nat_punching_rung_names_its_cost_where_the_cost_is_paid` continua
    // cobrando que a frase diga o que o ponto de encontro aprende, e passou a
    // cobrar que ela não mande ninguém ler documentação para saber disso.
    // A segunda linha entrou no lugar de «você pode apontar para outro», e a
    // troca é deliberada: o guarda de tamanho aceita duas frases, e das três
    // que cabiam esta é a que muda o que a pessoa **faz hoje**. Apontar para
    // outro ponto de encontro é escolha de quem opera, e está em
    // `docs/ponto-de-encontro.md`.
    //
    // O que ela diz custou uma tarde de teste de campo: o endereço deste link
    // não é um endereço, é um buraco no roteador que existe enquanto este app
    // estiver aberto. Fechou e abriu, o buraco é outro, e todo link já mandado
    // aponta para o vazio — sem ninguém ser avisado. Três amigos bateram numa
    // porta morta enquanto o servidor estava no ar ao lado.
    FuroDeNat:
      "UM PONTO DE ENCONTRO ABRIU O CAMINHO: SABE QUEM FALOU, NUNCA O QUE FOI DITO, " +
      "E DÁ PARA APONTAR PARA OUTRO.\n" +
      "O link vale enquanto o app estiver aberto; se fechar, gere outro.",
    // Enquanto a pendência 26 estiver aberta, esta é a única frase da escada
    // que anunciava alcance sem ter alcance. A medição de 2026-08-24 mostrou o
    // firewall do roteador barrando a entrada, e o anfitrião não tem como saber
    // daqui — então a frase diz o que se sabe, e não o que se esperava.
    Ipv6Direto:
      "ESTE LINK PODE NÃO FUNCIONAR DE FORA DA SUA REDE.\n" +
      "Muitos roteadores bloqueiam a entrada. Teste com alguém antes de contar com ele.",
    // O degrau que nasceu de um defeito de campo: um Windows com Cloudflare
    // WARP tinha IPv6 global — do túnel —, e a escada declarava «alcança de
    // qualquer lugar» embaixo de um link que não aceita entrada nenhuma. Frase
    // própria porque a causa é diferente das outras três — é a VPN, e não o
    // roteador —, e é a única coisa que este degrau sabe e o `SoRedeLocal` não.
    // A frase dizia também o que fazer («desligue a VPN»); essa metade saiu em
    // 2026-08-20, com as segundas linhas de toda a escada.
    RedeLocalOuVpn:
      "ESTE LINK SÓ ALCANÇA A SUA REDE, OU QUEM ESTIVER NA MESMA VPN.",
    SoRedeLocal:
      "ESTE LINK SÓ FUNCIONA NA SUA REDE.",

    // Escolher microfone **ou saída de som**, no Terminal Server. Duas frases e
    // não uma porque pedem coisas diferentes de quem lê: a primeira não tem
    // conserto na tela, e a segunda tem — a lista está logo acima, e o que
    // falhou pode ser trocado por outro sem sair daqui.
    NaoGravei: "NÃO CONSEGUI GRAVAR ESSE AJUSTE NESTA MÁQUINA",
    // **Ela deixou de falar só de microfone, e deixou de falar só de sumiço.**
    //
    // O mesmo desfecho cobre a saída de som, e cobre um segundo caso que antes
    // não dizia nada: a troca voltar sem erro e o dispositivo aberto continuar
    // sendo o de antes. Era assim que ela aparecia em campo — «mostra EM USO num
    // e ESCOLHIDO no que eu escolhi, mas não muda» —, e um produto que grava a
    // ordem e não a cumpre, calado, é pior que um que recusa.
    //
    // Sem nomear a causa, porque desta tela não dá para distinguir: um
    // dispositivo desligado no meio do clique e um que o sistema não entrega
    // terminam no mesmo lugar. O que a frase precisa dar é o próximo passo, e
    // ele é o mesmo nos dois.
    DispositivoSumiu:
      "NÃO CONSEGUI PASSAR PARA ESSE DISPOSITIVO.\n" +
      "Ele pode ter sido desligado, ou estar preso a outro programa. " +
      "O de antes continua valendo; escolha outro na lista acima.",

    // ---- atualizar (ADR 0026) ----
    //
    // Seis variantes e seis frases, e a divisão não é zelo: elas pedem coisas
    // diferentes de quem está na frente da tela. Duas delas mandam **não**
    // tentar de novo — uma porque não há o que tentar neste executável, outra
    // porque tentar de novo é justamente o que não se faz com um pacote que
    // chegou assinado por outra pessoa. Escrever «não deu» nas seis mandaria
    // todo mundo apertar o botão de novo, inclusive nesses dois casos.
    //
    // Uma só ainda diz que **esta máquina continua como estava**: a que falha
    // depois de mexer em arquivo instalado. Nas outras seis nada chegou a ser
    // instalado, e dizê-lo era tranquilizar sobre um susto que a própria frase
    // inventava. Por que o pacote é conferido inteiro antes de qualquer arquivo
    // ser tocado: `docs/assinatura-e-atualizacao.md`.
    NaoConfigurado:
      "ESTE SEELE SAIU SEM CHAVE DE ATUALIZAÇÃO.\n" +
      "Não adianta tentar de novo: baixe a versão nova da página de releases.",
    NaoAlcancei:
      "NÃO CONSEGUI PERGUNTAR SE HÁ VERSÃO NOVA.\n" +
      "A página de releases não respondeu; tente de novo daqui a pouco.",
    // Separada da de cima, e a diferença é tudo para quem lê: ali a rede
    // falhou e tentar de novo faz sentido; aqui a rede funcionou e a resposta
    // foi «não há nada publicado». Mandar conferir a conexão seria mandar
    // procurar defeito onde não há — e foi assim que este caso apareceu, com o
    // botão dizendo que a página não respondeu sobre uma página que respondeu.
    NadaPublicado:
      "AINDA NÃO HÁ VERSÃO PUBLICADA PARA BAIXAR.\n" +
      "Não é a sua conexão, e não adianta tentar de novo: este app é o mais recente que existe.",
    SemPacoteParaEsteSistema:
      "HÁ VERSÃO NOVA, MAS NÃO PARA ESTE SISTEMA NEM PARA ESTE PROCESSADOR.",
    AssinaturaRecusada:
      "O PACOTE BAIXADO NÃO FOI ASSINADO POR ESTE PROJETO.\n" +
      "Ele foi jogado fora, e não é para tentar de novo: baixe da página de releases.",
    NaoInstalei:
      "A TROCA DOS ARQUIVOS FALHOU, E NÃO HÁ MEIA INSTALAÇÃO.\n" +
      "Feche outras cópias do SEELE e tente de novo.",
    NadaEscolhido:
      "NÃO HÁ VERSÃO NOVA ESCOLHIDA PARA INSTALAR.\n" + "Procure de novo antes de instalar.",
    // Spec 0.16, D-linha-1: esta versão foi aberta pelo launcher de outra
    // versão do servidor, e o atualizador dela trocaria o app guardado lá. Sem
    // «linha» na frase: o ADR 0033 tirou a palavra da tela.
    AbertaPeloLancador:
      "ESTA VERSÃO FOI ABERTA POR OUTRO SEELE, QUE CUIDA DELA.\n" +
      "Quem a atualiza é ele: não há nada a fazer aqui.",
};

/**
 * Por que um arquivo não foi aceito, ou não vem. ADR 0027.
 *
 * Dez variantes e dez frases, e a divisão não é zelo: elas mandam coisas
 * diferentes de quem está esperando. Uma diz «tente de novo daqui a pouco»,
 * outra diz «este arquivo nunca vai caber», e uma terceira diz «os bytes já
 * foram embora, e o que sobrou é o nome». Escrever «não deu» nas dez faria
 * todo mundo tentar de novo, inclusive nos casos em que tentar de novo é a
 * coisa errada a fazer.
 */
const ANEXOS = {
  NotAllowed:
    "VOCÊ NÃO PODE ANEXAR ARQUIVO NESTE SERVIDOR.\n" +
    "Peça a permissão a quem hospeda.",
  TooLarge:
    "ESTE ARQUIVO É GRANDE DEMAIS PARA ESTE SERVIDOR.\n" +
    "Tentar de novo com o mesmo arquivo dá no mesmo.",
  NoRoom:
    "O DISCO DESTE SERVIDOR ESTÁ TOMADO POR TRANSFERÊNCIAS EM ANDAMENTO.\n" +
    "Tente de novo daqui a pouco.",
  SizeMismatch:
    "O ARQUIVO NÃO CHEGOU INTEIRO, E NADA FOI PUBLICADO NO CANAL.\n" +
    "Mandar de novo manda o arquivo inteiro outra vez, do começo.",
  HashDidNotMatch:
    "O QUE CHEGOU NÃO É O QUE SAIU DAQUI, E O SERVIDOR RECUSOU.\n" +
    "Nada foi publicado no canal.",
  RateLimited:
    "VOCÊ ESTÁ MANDANDO ARQUIVO MAIS RÁPIDO DO QUE ESTE SERVIDOR ACEITA.\n" +
    "Espere um pouco e mande de novo.",
  Unavailable: "ESTE SERVIDOR NÃO GUARDA ARQUIVO.",
  NotFound: "ESTE ARQUIVO NÃO EXISTE NESTE SERVIDOR, OU ESTÁ NUM CANAL QUE VOCÊ NÃO PODE LER.",
  Expired:
    "ESTE ARQUIVO EXPIROU, E O SERVIDOR APAGOU OS BYTES PARA ABRIR ESPAÇO.\n" +
    "Peça a quem mandou para mandar de novo.",
  Malformed:
    "O SERVIDOR NÃO ENTENDEU O PEDIDO DE ARQUIVO, E NADA FOI PUBLICADO NO CANAL.\n" +
    "Se acontecer de novo, as duas pontas podem estar em versões diferentes.",
};

/**
 * O que aconteceu com um arquivo que estava subindo.
 *
 * `Caiu` é a que só existe porque o ADR 0027 mandou dizê-la: **não há
 * retomada.** Uma transferência que cai recomeça do zero, e isso precisa ser
 * dito a quem está esperando em vez de descoberto pela barra voltando ao
 * começo.
 */
const TRANSFERENCIAS = {
  Sent: "ARQUIVO ENTREGUE",
  Refused: "O SERVIDOR RECUSOU O ARQUIVO",
  Fell:
    "A TRANSFERÊNCIA CAIU, E NÃO HÁ DE ONDE CONTINUAR.\n" +
    "Mandar de novo manda o arquivo inteiro outra vez, do começo.",
  Saved: "ARQUIVO SALVO",
};

/**
 * Por que um arquivo que chegou não foi salvo. O `Transfer::NotSaved` diz qual.
 *
 * Quatro, porque mandam coisas diferentes. Um nome recusado não adianta tentar
 * de novo: só quem mandou resolve, mandando com outro nome. Uma gravação que
 * falhou no meio adianta. Uma frase só para as quatro mandaria tentar de novo o
 * que vai ser recusado de novo.
 */
const NAO_SALVOS = {
  NomeRecusado:
    "ESTE ARQUIVO VEIO COM UM NOME QUE O SEELE NÃO GRAVA " +
    "NO WINDOWS, NO MAC NEM NO LINUX, E NADA FOI SALVO.\n" +
    "Peça a quem mandou que mande de novo com outro nome.",
  AnexoDesconhecido:
    "ESTE ARQUIVO NÃO ESTÁ NA CONVERSA QUE ESTA JANELA CARREGOU, E NADA FOI GRAVADO.",
  SemPasta:
    "O SEELE NÃO ACHOU ONDE GRAVAR — NEM A PASTA DE DOWNLOADS, NEM A SUA PASTA PESSOAL —, " +
    "E NADA FOI GRAVADO.",
  Falhou:
    "NÃO DEU PARA SALVAR O ARQUIVO.\n" +
    "O SEELE apaga o que começou a gravar e, se não conseguir, diz no seele.log; tente de novo.",
};

/**
 * A confirmação de salvar, com o lugar que o Rust escolheu.
 *
 * `onde` é o que `destino_do_anexo` devolveu: a pasta, o nome, e o nome que o
 * arquivo ganha ao lado se o primeiro estiver tomado. Esta janela não monta
 * caminho nenhum; ela só escreve o que o Rust disse que vai fazer — e na hora de
 * gravar o Rust deriva tudo de novo, sem ler nada daqui.
 *
 * As duas linhas do fim são as que o ADR 0027 manda dizer antes do ato, e não
 * numa página de ajuda: o SEELE não varre vírus, e nenhuma tela dele abre o
 * arquivo.
 */
function fraseDeSalvar(onde) {
  return (
    `Grava «${onde.file_name}» em ${onde.folder}.\n` +
    "Se já houver um arquivo com esse nome lá, o SEELE grava ao lado, como " +
    `«${onde.beside}», e não substitui nada.\n` +
    "O SEELE não confere se este arquivo é seguro: ele não varre vírus e não " +
    "vai varrer. Ele confere só que o arquivo chegou inteiro. O arquivo é " +
    "marcado com a quarentena do sistema ao ser gravado, e é o seu sistema " +
    "operacional que vai parar você na frente dele se for o caso.\n" +
    "Nenhuma tela do SEELE abre este arquivo. Abrir é um ato seu, fora daqui."
  );
}

/**
 * A frase de um arquivo que não foi, ou não seria, salvo.
 *
 * Recebe o motivo sozinho, como ele chega no `Transfer::NotSaved`, ou a recusa
 * inteira de `destino_do_anexo`, que traz também o nome que veio e a pasta. Com
 * os dois, um nome recusado é citado: a pessoa vê o que veio — com o caractere
 * invisível escrito por extenso, que o Rust já fez — e onde ele teria gravado.
 * Em duas linhas, como as outras compostas deste arquivo.
 *
 * A tela não sabe qual regra pegou o nome (isso vai para o `seele.log`), então
 * a frase cobre todas sem afirmar o que só uma delas faz. Nomes comuns caem
 * aqui também — «Notas 04:10.txt», que é como o Finder grava uma barra digitada
 * no nome, ou «Por quê?.pdf», que o Mac e o Linux aceitam —, e a frase não pode
 * descrevê-los como ataque.
 */
function fraseDeNaoSalvo(recusa) {
  const motivo = typeof recusa === "string" ? recusa : recusa?.reason;
  if (motivo === "NomeRecusado" && recusa?.claimed && recusa?.folder) {
    return (
      "ESTE ARQUIVO VEIO COM UM NOME QUE O SEELE NÃO GRAVA " +
      "NO WINDOWS, NO MAC NEM NO LINUX, E NADA FOI SALVO.\n" +
      `«${recusa.claimed}» tem um caminho, um caractere ou nome que o Windows ` +
      "não aceita, um caractere invisível que disfarça o nome, ou é comprido " +
      `demais. Para gravar em ${recusa.folder}, peça outro nome a quem mandou.`
    );
  }
  return NAO_SALVOS[motivo] ?? `FALHA NÃO IDENTIFICADA (${motivo})`;
}

/**
 * Por que um arquivo não foi desenhado. ADR 0027.
 *
 * Quatro, e a primeira é a que este caminho inteiro existe para poder dizer.
 * As `NOTAS-DE-RELEASE` deste projeto separam duas perguntas — «o arquivo
 * chegou inteiro?» e «como sei que ele é o que diz ser?» — e um anexo só
 * alcançava a primeira. O hash respondeu sim a ela; a segunda tem resposta
 * agora, e quando a resposta é não ela merece a frase própria em vez de virar
 * um silêncio que se lê como defeito.
 *
 * **Não desenhar não é esconder.** Em todas as quatro o arquivo continua na
 * tela, com nome, tamanho e o botão de salvar. O que ele perde é a figura.
 */
const PREVIAS = {
  TooBig:
    "ESTE ARQUIVO É GRANDE DEMAIS PARA UMA PRÉVIA.\n" +
    "Salve-o e abra no seu sistema, fora daqui.",
  NotAPicture:
    "ESTE ARQUIVO NÃO É UMA DAS IMAGENS QUE ESTA JANELA DESENHA.\n" +
    "Ele continua aqui, com nome e tamanho, para salvar.",
  DidNotArrive: "A PRÉVIA NÃO VEIO.",
};

/**
 * A frase de uma prévia que não virou figura.
 *
 * `Disagrees` é montada e não fixa, porque as duas metades da discordância são
 * o conteúdo: o que o arquivo **disse** que era e o que os primeiros bytes dele
 * **são**. Uma frase genérica aqui mandaria a pessoa tentar de novo, e tentar
 * de novo é a coisa errada a fazer com um arquivo que se apresentou como uma
 * coisa e é outra.
 */
function fraseDePrevia(previa) {
  const razao = previa?.refusal;
  const nome = razao?.kind;
  if (nome === "Disagrees") {
    const achado = previa.found
      ? `os primeiros bytes dele são de «${previa.found}»`
      : "os primeiros bytes dele não são de imagem nenhuma";
    return (
      "ESTE ARQUIVO NÃO É O QUE DIZ SER.\n" +
      `Ele chegou inteiro, se apresentou como «${previa.claimed}», e ${achado} ` +
      "— continua aqui, para salvar."
    );
  }
  // O número entra no título, e não numa linha nova. Uma terceira linha é a
  // redação que esta tela acabou de perder, e o máximo é justamente o que
  // qualifica o «grande demais» — ele pertence à frase que o diz.
  if (nome === "TooBig" && typeof razao.limit === "number") {
    return PREVIAS.TooBig.replace(".\n", `: O MÁXIMO É ${emBytes(razao.limit)}.\n`);
  }
  return PREVIAS[nome] ?? `FALHA NÃO IDENTIFICADA (${nome})`;
}

/**
 * A frase de uma recusa de anexo.
 *
 * `TooLarge` chega com o limite dentro, porque «grande demais» sem número manda
 * a pessoa tentar de novo com um arquivo que também é grande demais. Ele entra
 * no título, e não numa linha nova: é o que qualifica o «demais».
 */
function fraseDeAnexo(razao) {
  if (razao && typeof razao === "object") {
    const nome = Object.keys(razao)[0];
    const base = ANEXOS[nome];
    if (!base) return `FALHA NÃO IDENTIFICADA (${nome})`;
    if (nome === "TooLarge" && typeof razao[nome]?.limit === "number") {
      return base.replace(".\n", `: O LIMITE É ${emBytes(razao[nome].limit)} POR ARQUIVO.\n`);
    }
    return base;
  }
  return ANEXOS[razao] ?? `FALHA NÃO IDENTIFICADA (${razao})`;
}

/**
 * Uma taxa em bits por segundo do jeito que alguém lê uma: em kbps, com o
 * ponto dos milhares e, se pedidas, `casas` decimais depois da vírgula.
 * `6480000` vira `6.480 kbps`; com uma casa, `4000400` vira `4.000,4 kbps`.
 *
 * Os separadores escritos à mão, e não `toLocaleString`: a frase é desta casa,
 * e o número não pode mudar de forma com a língua do sistema de quem lê. `null`
 * quando o valor não é um número — e quem chama escreve a frase sem ele.
 */
function emKbps(bps, casas = 0) {
  const numero = Number(bps);
  if (!Number.isFinite(numero) || numero <= 0) return null;
  const [inteira, fracao] = (numero / 1000).toFixed(casas).split(".");
  const milhares = inteira.replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return fracao ? `${milhares},${fracao} kbps` : `${milhares} kbps`;
}

/**
 * Duas taxas em kbps, para pôr lado a lado, que **nunca saem iguais quando são
 * diferentes** (M-6 da rodada 1 do lote I-b).
 *
 * Inteiros quando bastam. Dois valores a menos de 500 bps um do outro
 * arredondavam para o mesmo número, e a frase dizia «pede 4.000 kbps, e envia
 * cerca de 4.000 kbps» sobre números que não eram iguais. Então ganham casas
 * decimais, uma de cada vez, até se separarem — com três, o kbps é o bps
 * exato. Iguais de verdade saem iguais.
 */
function doisEmKbps(a, b) {
  let par = [emKbps(a), emKbps(b)];
  for (let casas = 1; casas <= 3; casas += 1) {
    if (par[0] === null || par[1] === null || par[0] !== par[1] || Number(a) === Number(b)) {
      break;
    }
    par = [emKbps(a, casas), emKbps(b, casas)];
  }
  return par;
}

/**
 * Um número de bytes do jeito que alguém lê um.
 *
 * Binário, como o teto que quem hospeda escolheu: dizer «1 GB» para 2^30 seria
 * mentir sobre o número que a pessoa digitou.
 */
function emBytes(bytes) {
  const GIB = 1024 * 1024 * 1024;
  const MIB = 1024 * 1024;
  const KIB = 1024;
  if (bytes >= GIB) return `${(bytes / GIB).toFixed(1)} GB`;
  if (bytes >= MIB) return `${(bytes / MIB).toFixed(1)} MB`;
  if (bytes >= KIB) return `${Math.round(bytes / KIB)} KB`;
  return `${bytes} B`;
}

/**
 * As mensagens de apps mais novos que o servidor desta janela pulou (a P12 do
 * plano F1b), para a seção ESTE SERVIDOR das configurações.
 *
 * **O zero também é dito**, e não escondido: uma linha que só aparece quando
 * há problema deixa quem hospeda sem saber se o número existe. E o número
 * vem com o que fazer, porque saber sem dizer o remédio é contar pela metade.
 * Sem «linha» e sem «protocolo»: para pessoas, é a versão do SEELE.
 */
function fraseDasMensagensDeAppsMaisNovos(mensagens, sessoes) {
  const titulo = "MENSAGENS DE APPS MAIS NOVOS QUE ESTE SERVIDOR";
  if (!mensagens) return `${titulo}: 0`;
  const emSessoes = `${sessoes} ${sessoes === 1 ? "SESSÃO" : "SESSÕES"}`;
  return (
    `${titulo}: ${mensagens}, EM ${emSessoes}.\n` +
    "Quem usa um SEELE mais novo está mandando o que esta versão não entende; " +
    "atualizar o SEELE daqui resolve."
  );
}

// ------------------------------------------- a sala pessoal (F7, 23 a 29)
//
// As frases de quem recebe ligações pelo SEU CÓDIGO (spec de 23/09, «Erros e
// frases», «Quem recebe»). O Rust manda enums (`sala::EstadoDaSala`,
// `sala::Registro`, `sala::FalhaDaSala`, `sala::FalhaAoAtender`,
// `sala::FalhaNaAgenda`) e o desfecho do toque numa palavra; a frase é daqui.
// O guarda `toda_variante_da_sala_pessoal_tem_frase` confere cada nome contra o
// Rust. As que não têm nada a dizer estão com `null`, de propósito: a sala
// subindo, o código esperando a primeira resposta do ponto, o código achável.
//
// Só o desktop lê isto: o navegador não tem sala, e as peças dela levam
// `data-sala-pessoal`, que as duas cascas do navegador escondem.

const SALA_PESSOAL = Object.freeze({
  /** O nome que a pessoa lê, o mesmo da lista SÓ NO APP DO SEELE da F8. */
  nome: "RECEBER LIGAÇÕES PELO SEU CÓDIGO",

  /** `EstadoDaSala`: o que a linha da sala diz; `NoAr` diz pelo registro. */
  estado: Object.freeze({
    Subindo: null,
    Desligada: "VOCÊ NÃO ESTÁ RECEBENDO LIGAÇÕES",
    NoAr: null,
    Falhou: null,
  }),

  /** `FalhaDaSala`: por que a sala não subiu. */
  falhou: Object.freeze({
    PortaOcupada:
      "NINGUÉM CONSEGUE TE LIGAR AGORA: a porta 8385 está em uso, talvez por outro SEELE aberto neste computador.",
    IdentidadeIndisponivel:
      "NINGUÉM CONSEGUE TE LIGAR AGORA: a chave deste computador não abriu.",
    NaoSubiu:
      "NINGUÉM CONSEGUE TE LIGAR AGORA: a sala não subiu. O motivo está no seele.log.",
  }),

  /**
   * `Registro`: se quem liga pelo código acha este computador. A frase é
   * «SEU CÓDIGO NÃO ESTÁ ACHÁVEL AGORA» e o motivo (`fraseDaSala`).
   */
  naoAchavel: "SEU CÓDIGO NÃO ESTÁ ACHÁVEL AGORA",
  registro: Object.freeze({
    SemPonto:
      "o ponto de encontro está desligado neste computador, e só quem está na mesma rede chega até você",
    Esperando: null,
    Registrado: null,
    // O sinal diz para que lado: positivo, o relógio daqui está adiantado.
    RelogioTorto: (diferencaS) => {
      const minutos = Math.max(1, Math.round(Math.abs(diferencaS) / 60));
      const lado = diferencaS > 0 ? "adiantado" : "atrasado";
      return `o relógio deste computador está ${minutos} min ${lado}. Acerte o relógio, e o código volta a ser achado`;
    },
    // O motivo como o fio o escreve (`Motivo::texto`).
    Recusado: Object.freeze({
      velho: "outro computador com a sua chave registrou o seu código depois deste",
      cheio: "o ponto de encontro está cheio agora",
      taxa: "esta rede pediu demais ao ponto de encontro, e ele volta a aceitar em instantes",
      relogio: "o relógio deste computador está fora da hora",
    }),
    CookieRecusado:
      "esta rede sai para a internet por mais de um endereço, e o ponto de encontro não consegue confirmá-la",
    SemResposta:
      "o ponto de encontro não respondeu. Ele pode estar fora do ar, ou ainda não conhecer este jeito de registrar",
  }),

  /**
   * O firewall do Windows sem a regra deste SEELE (D-sala-1): a frase do Rust
   * vem antes desta, que diz para que serve a 8385.
   */
  firewall:
    "A porta 8385 é a da sua sala: é por ela que quem liga pelo seu código entra. Sem a regra, quem liga de fora da sua rede pode não chegar.",
  abrirAPorta: "ABRIR A PORTA AGORA",

  /** A faixa `#toque`: o apelido entre aspas, porque é o que a pessoa diz. */
  ligando: ({ apelido, codigo, nome_na_agenda: nome }) => {
    const quem = nome ? `${nome} («${apelido}»)` : `«${apelido}»`;
    return `${quem} · ${codigo} está ligando`;
  },
  semSom: "SEM SOM: nenhuma saída de áudio abriu, e este aviso é o único.",
  atenderSaiDe: (daqui) => `ATENDER SAI DE ${daqui}`,
  naoEntrou: "VOCÊ ATENDEU, MAS NÃO CONSEGUIU ENTRAR NA SUA SALA",
  tentarDeNovo: "TENTAR DE NOVO",
  encerrada: "A LIGAÇÃO FOI ENCERRADA: você não conseguiu entrar na sua sala, e ninguém ficou lá sozinho.",

  /** `FalhaAoAtender`. */
  aoAtender: Object.freeze({
    SemSala: "A SUA SALA NÃO ESTÁ NO AR",
    JaParou: "ESSA LIGAÇÃO JÁ PAROU DE TOCAR",
  }),

  /** `FalhaNaAgenda`. */
  naAgenda: Object.freeze({
    CodigoInvalido: "ESSE CÓDIGO TEM UM ERRO DE DIGITAÇÃO",
    NaoGravou: "A AGENDA NÃO GRAVOU. O motivo está no seele.log.",
  }),

  /** O desfecho de um toque, dito quando a faixa some. */
  desfecho: Object.freeze({
    atendida: "LIGAÇÃO ATENDIDA",
    recusada: "LIGAÇÃO RECUSADA",
    ninguem_atendeu: "CHAMADA PERDIDA",
    caiu_antes: "QUEM LIGAVA DESISTIU",
    cheia: "A LIGAÇÃO ESTÁ CHEIA",
    sala_fechou: "A SUA SALA FECHOU",
  }),
});

/**
 * A frase da linha da sala, ou `null` quando não há o que dizer.
 *
 * Um estado que a casca não conhece também é `null`: a peça fica escondida, e
 * não mostra um estado que esta versão não sabe ler.
 */
function fraseDaSala(estado) {
  switch (estado?.estado) {
    case "Desligada":
      return SALA_PESSOAL.estado.Desligada;
    case "Falhou":
      return SALA_PESSOAL.falhou[estado.motivo] ?? null;
    case "NoAr": {
      const registro = estado.registro ?? {};
      let motivo = SALA_PESSOAL.registro[registro.registro] ?? null;
      if (typeof motivo === "function") motivo = motivo(registro.diferenca_s ?? 0);
      if (motivo && typeof motivo === "object") motivo = motivo[registro.motivo] ?? null;
      return motivo ? `${SALA_PESSOAL.naoAchavel}: ${motivo}.` : null;
    }
    default:
      return null;
  }
}

// ---------------------------------------------- as cascas do navegador (P10)
//
// As frases da casca do celular e da casca larga (F8; o spec da F8, §13, e a
// matriz de paridade). Os scripts delas não escrevem frase nenhuma: o
// `tests/celular.rs` confere. O desktop não lê nada daqui.

/** Para onde vai quem quer o app do SEELE (decisão da orquestradora, 09/10). */
const PAGINA_DO_APP = "https://seele.app.br/#baixar";

/**
 * O aviso de fone no navegador (P9; matriz, achado E-6).
 *
 * O do desktop diz que o SEELE não cancela eco, o que no navegador conta só
 * metade: a voz pede o `echoCancellation` do navegador. **Mas pedir não é
 * cancelar**, e isso não foi medido (F4, 09/10): a voz do SEELE toca pelo Web
 * Audio, e não por um `RTCPeerConnection`, e se o cancelador do navegador usa
 * essa saída como referência varia por motor. Dizer «cancela» sem a medida
 * convenceria a tirar o fone justamente quem faz o outro lado ouvir a própria
 * voz. Até a medida da F4 (a 3.3 dela), a frase pede o fone sem afirmar o que o
 * navegador faz.
 */
const AVISO_DE_FONE_CELULAR =
  "USE FONE. Sem fone, o som do alto-falante pode voltar pelo microfone, e quem fala com você se ouve de volta.";

/**
 * O que só o app do SEELE faz, a lista canônica da matriz: a mesma nas duas
 * cascas, e só o aparelho que não repassa a tela muda de nome. O terceiro campo
 * põe o link para baixar o app ao lado da frase.
 */
const SO_NO_APP_DO_SEELE = (aparelho) => Object.freeze({
  hospedar: ["HOSPEDAR UM SERVIDOR", "Pôr um servidor no ar, o link de convite, a porta e o firewall ficam no app do SEELE, instalado num computador.", true],
  portaria: ["A PORTARIA DO SEU SERVIDOR", "Aprovar quem bate, a senha e os convites ficam no app do SEELE que hospeda o servidor."],
  mods: ["INSTALAR E LIGAR MODS", "Os MODs deste servidor rodam aqui. Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE."],
  pacotes: ["PACOTES DE MOD", "O espaço que os pacotes ocupam, as revisões guardadas e o APAGAR ficam no app do SEELE."],
  codigo: ["LIGAR PELO CÓDIGO", "Ligar pelo código é pelo app do SEELE: o navegador não abre o caminho que a ligação usa."],
  receber: ["RECEBER LIGAÇÕES PELO SEU CÓDIGO", "Receber ligações pelo SEU CÓDIGO é pelo app do SEELE: quem recebe a ligação a hospeda, e o navegador não hospeda."],
  versoes: ["VERSÕES LADO A LADO", "O navegador abre a página da versão do servidor que o link pede. Guardar e abrir outras versões é do app do SEELE."],
  telaEntrePessoas: ["A TELA ENTRE PESSOAS", `No navegador, a tela de quem transmite vem sempre pelo servidor, e ${aparelho} não a repassa a ninguém.`],
});

/** O que é pelo SEELE no computador: a outra metade da lista do celular. */
const NO_COMPUTADOR = Object.freeze({
  moderar: ["MODERAR", "Expulsar, banir, mover pessoas e remover mensagens de outros é pelo SEELE no computador."],
  salasECanais: ["SALAS E CANAIS", "Criar, renomear e apagar salas de voz e canais é pelo SEELE no computador."],
  nomeEImagem: ["NOME E IMAGEM DO SERVIDOR", "Pelo SEELE no computador. A aparência do servidor se edita aqui, em ESTE SERVIDOR."],
  compartilhar: ["COMPARTILHAR A TELA", "O navegador do celular não grava a tela. Compartilhar é pelo SEELE no computador, e a tela de quem compartilhar aparece na chamada."],
  testeDoMicrofone: ["TESTAR O MICROFONE FORA DE SALA", "O microfone do celular só abre numa sala de voz. Testar e ouvir a própria voz fora de sala é pelo SEELE no computador."],
  saidaDeSom: ["ESCOLHER A SAÍDA DE SOM", "O navegador do celular não escolhe a saída: troque entre fone, Bluetooth e alto-falante pelo controle de som do celular."],
});

const FRASES_DO_CELULAR = Object.freeze({
  voz: Object.freeze({
    abas: { servidor: "SERVIDOR", conversa: "CONVERSA", pessoas: "PESSOAS", ajustes: "AJUSTES" },
    secoes: "seções",
    mic: "MIC",
    mudo: "MUDO",
    som: "SOM",
    surdo: "SURDO",
    sair: "SAIR",
    salaDeVoz: (nome) => `Sala de voz ${nome}`,
    abrirChamada: (nome) => `${nome}: abrir a chamada`,
    sairDaSala: "Sair da sala de voz",
    micAberto: "Microfone aberto. Tocar fecha.",
    micFechado: "Microfone fechado. Tocar abre.",
    somLigado: "Som ligado. Tocar desliga.",
    somDesligado: "Som desligado. Tocar liga.",
    segure: "SEGURE",
    noAr: "NO AR",
    segurarParaFalar: "Segurar para falar",
    fala: (quem) => `${quem} fala`,
    naSala: (n) => `${n} na sala`,
    sinal: (n) => `sinal ${n}`,
    // Com o enlace caído (spec §2.5), no lugar de quem fala.
    reconectando: "SEM SOM · RECONECTANDO",
    recolher: "RECOLHER",
    recolherChamada: "Recolher a chamada e voltar à sessão",
    telemetria: "telemetria da chamada",
    medidas: [["ATRASO", "tel-rtt"], ["JITTER", "tel-jit"], ["PERDA", "tel-loss"], ["CODEC", "tel-codec"]],
  }),
  servidor: Object.freeze({
    ajustes: "AJUSTES",
    aqui: "VOCÊ ESTÁ AQUI",
    comSenha: "COM SENHA",
    lotada: "LOTADA",
    mudo: "MUDO",
    entrarMudo: (sala) => `Entrar mudo em ${sala}`,
    salasECanais: NO_COMPUTADOR.salasECanais[0],
    foraSalas: NO_COMPUTADOR.salasECanais[1],
    mudoNaoRespondeu: "O MUDO não respondeu, e a entrada na sala não foi feita.",
  }),
  conversa: Object.freeze({
    escrever: (canal) => `escrever em #${canal}`,
    anexar: "Anexar uma foto, um vídeo ou um arquivo",
    renomearTitulo: "RENOMEAR O CANAL",
    salasECanais: NO_COMPUTADOR.salasECanais[0],
    foraSalas: NO_COMPUTADOR.salasECanais[1],
  }),
  folhas: Object.freeze({
    // A prévia do tema (D-estilo-1): o diálogo recolhido num selo do núcleo.
    verNaSessao: "VER NA SESSÃO",
    seloDaPrevia: "PRÉVIA NÃO PUBLICADA · SÓ NESTE APARELHO",
    voltarAoDialogo: "VOLTAR",
    senhaDaSala: "SENHA DA SALA",
    pedeSenha: (sala) => `${sala} pede senha para entrar.`,
    senha: "SENHA",
    entrar: "ENTRAR",
    agoraNao: "AGORA NÃO",
    fechar: "FECHAR",
    mensagemDe: (quem, hora) => `${quem} · ${hora}`,
    copiar: "COPIAR O TEXTO",
    apagarMinha: "APAGAR A MINHA MENSAGEM",
    remover: "REMOVER",
    foraRemover: "Remover a mensagem de outra pessoa é pelo SEELE no computador.",
    sinal: "SINAL",
    volume: (n) => `VOLUME DESTA PESSOA · ${n}%`,
    volumeDe: (quem) => `volume de ${quem}`,
    moderar: "MODERAR",
    foraModerar: "Expulsar, banir e mover pessoas é pelo SEELE no computador.",
    anexar: "ANEXAR",
    midia: "FOTO OU VÍDEO",
    arquivo: "ARQUIVO",
  }),
  ajustes: Object.freeze({
    ajustes: "AJUSTES",
    voltarAosAjustes: "Voltar à lista de AJUSTES",
    seuPerfil: "SEU PERFIL",
    editarPerfil: "EDITAR PERFIL",
    audio: "ÁUDIO",
    esteServidor: "ESTE SERVIDOR",
    modsDesteServidor: "MODS DESTE SERVIDOR",
    conexao: "CONEXÃO",
    conexaoSub: "As medidas do enlace com este servidor, também fora de sala.",
    medidas: Object.freeze({
      "tel-rtt": "ATRASO",
      "tel-jit": "JITTER",
      "tel-loss": "PERDA",
      "tel-codec": "CODEC",
      "tel-caminho": "CAMINHO",
      "tel-uptime": "UPTIME",
      "tel-sinal-valor": "SINAL DA SALA",
    }),
    foraDoCelular: "O QUE O CELULAR NÃO FAZ",
    foraDoCelularSub: "O que fica no app do SEELE, e o que é pelo SEELE no computador.",
    baixar: "BAIXAR O APP DO SEELE",
    grupos: Object.freeze([["SÓ NO APP DO SEELE", SO_NO_APP_DO_SEELE("este aparelho")], ["NO COMPUTADOR", NO_COMPUTADOR]]),
    comoAbre: "COMO ESTA PÁGINA ABRE",
    comoAbreSub: "Como celular ou como computador, na próxima vez que ela abrir.",
    abriuComoCelular: "Esta página abriu como celular.",
    abrirComoComputador: "ABRIR COMO COMPUTADOR",
    pelaTela: "PELO TAMANHO DA TELA",
    valeNaProxima: "Vale na próxima vez que a página abrir. Abrir de novo agora sai da sessão.",
    naoGuardou: "Este navegador não deixou guardar a escolha: a página continua abrindo pelo tamanho da tela.",
    sobre: "SOBRE",
    sairDoServidor: "SAIR DO SERVIDOR",
    segurarParaFalar: "SEGURAR PARA FALAR",
    segurarNota: "fala só enquanto o dedo está no SEGURE",
    // Na faixa de contexto de CONFIGURAÇÕES, no lugar do «segura ESPAÇO».
    modoSegurar: "microfone abre só enquanto o dedo está no SEGURE",
  }),
  // A entrada (Tarefa 10; spec §2.1): a tela de início, a «ENTRAR EM», a de
  // conectar e os desfechos. Os estados e os desfechos são os nomes da
  // `identidade-web.js` (F4); as frases vêm do protótipo aprovado e, no que ele
  // não cobria, da matriz (N-10, N-11, N-12 e a PA-53).
  entrada: Object.freeze({
    inicio: Object.freeze({
      titulo: "SERVIDORES DESTE APARELHO",
      vazio: "Nenhum servidor ainda. Abra um link de convite, ou cole um aqui.",
      entrar: "ENTRAR",
      servidor: (versao) => `Servidor ${versao}`,
      // No lugar do ENTRAR, quando os hashes guardados do servidor venceram.
      linkVencido: "LINK VENCIDO",
      colar: "COLAR UM LINK",
      campoDoLink: "O LINK QUE ALGUÉM TE MANDOU",
      continuar: "CONTINUAR",
      identidade: "Sua identidade mora neste navegador, neste aparelho.",
      hospedarRotulo: "HOSPEDAR",
      hospedar: "Hospedar um servidor é pelo app do SEELE, instalado num computador.",
    }),
    // O que o COLAR UM LINK recusa antes de abrir a «ENTRAR EM».
    colado: Object.freeze({
      soEndereco: Object.freeze(["UM ENDEREÇO SOZINHO NÃO ABRE CONEXÃO NO NAVEGADOR",
        "O link traz a chave do servidor, e sem ela o navegador não conecta. Peça o link a quem hospeda."]),
      doApp: Object.freeze(["ESTE LINK É PARA O APP DO SEELE",
        "No navegador ele não traz o que confere o servidor. Peça a quem hospeda o link para o celular."]),
    }),
    entrarEm: Object.freeze({
      titulo: "ENTRAR EM",
      sobre: "Nada saiu deste aparelho ainda: a conexão só abre quando você tocar em ENTRAR.",
      endereco: "ENDEREÇO",
      impressao: "IMPRESSÃO DO SERVIDOR",
      impressaoNota: "É a mesma que o SEELE de quem hospeda mostra. Quem atender com outra é recusado.",
      versao: "VERSÃO",
      vale: "O LINK VALE ATÉ",
      apelido: "COMO OS OUTROS VÃO TE CHAMAR",
      primeiraEntrada: "Ao entrar, este aparelho cria a identidade dele. Ela fica neste navegador: sem cópia e sem troca de aparelho.",
      entrar: "ENTRAR",
      agoraNao: "AGORA NÃO",
      servidores: "SERVIDORES DESTE APARELHO",
    }),
    // O botão do aviso da resposta na sessão: o mesmo dos avisos do pé ao lado
    // dele (o de fone e o do áudio, do desktop).
    dispensar: "dispensar",
    // A resposta do `navigator.storage.persist()` (ADR 0057), dita.
    persistencia: Object.freeze({
      concedida: "Este navegador guarda a sua identidade neste aparelho.",
      negada: "Este navegador pode apagar a sua identidade se faltar espaço. Instalado na Tela de Início, ele guarda.",
      "sem-suporte": "Este navegador não diz se guarda a sua identidade neste aparelho.",
    }),
    // Os estados da tentativa, antes do ENTRAR: [título, corpo].
    estados: Object.freeze({
      "numa-aba": Object.freeze({
        titulo: "INSTALE ANTES DE ENTRAR",
        corpo: "Você está numa aba do navegador. A identidade nasce onde você entra, e a aba e o ícone da Tela de Início não se enxergam: quem entra pela aba e depois instala vira outra pessoa, já sem convite.",
        passos: Object.freeze(["Toque em Compartilhar.", "Toque em Adicionar à Tela de Início.", "Abra o SEELE pelo ícone e cole este link."]),
        copiar: "COPIAR O LINK",
        copiado: "LINK COPIADO. Cole na tela de início do SEELE instalado.",
        naoCopiou: "O navegador não deixou copiar. Copie o link da mensagem em que ele chegou.",
        instalar: "INSTALAR",
      }),
      vencido: Object.freeze({
        titulo: (data) => `ESTE LINK VENCEU EM ${data}`,
        corpo: "O navegador só abre a conexão com o certificado que o link anuncia, e ele expirou. Peça um link novo a quem hospeda.",
      }),
      "servidor-0.15": Object.freeze({
        titulo: "ESTE LINK É DE UM SERVIDOR 0.15",
        corpo: "O celular entra só no Servidor 1.0. Abra o link no app do SEELE, ou peça a quem hospeda o Servidor 1.0.",
      }),
      incompleto: Object.freeze({
        titulo: "LINK INCOMPLETO",
        corpo: "Falta a impressão do servidor ou o certificado do navegador. Sem os dois não há como conferir quem responde, e o navegador não abre a conexão. Peça o link de novo, copiado inteiro.",
      }),
      "outra-origem": Object.freeze({
        titulo: "ESTE LINK É DE OUTRA PÁGINA DO SEELE",
        corpo: "A sua identidade mora nesta página. Entrar pela outra cria outra pessoa, sem o que você tem aqui.",
      }),
      "ja-conhecido": Object.freeze({
        titulo: "ESTE APARELHO JÁ ENTRA NESTE SERVIDOR",
        corpo: "Você entra com a identidade que já tem aqui, e o convite não é usado.",
      }),
    }),
    // Os desfechos do ENTRAR que não abrem a sessão: [título, corpo].
    desfechos: Object.freeze({
      "apelido-tomado": Object.freeze(["ESTE APELIDO JÁ É DE OUTRA CHAVE NESTE SERVIDOR",
        "Se for outra pessoa, escolha outro. Se for você, este aparelho perdeu a identidade, e quem hospeda libera o nome. O convite continua valendo."]),
      "convite-gasto": Object.freeze(["ESTE CONVITE JÁ FOI USADO",
        "Cada convite entra uma vez. Peça um link novo a quem hospeda."]),
      "convite-vencido": Object.freeze(["ESTE CONVITE VENCEU",
        "Peça um link novo a quem hospeda."]),
      // A PA-53: o navegador não separa servidor fora do ar de chave trocada.
      "sem-rede": Object.freeze(["O SERVIDOR NÃO ABRIU A CONEXÃO",
        "Ele pode estar fora do ar, ou este link não é mais dele. O convite continua valendo: tente de novo."]),
      recusado: Object.freeze(["O SERVIDOR RECUSOU A ENTRADA", "O motivo que ele deu:"]),
      // Os hashes guardados de um servidor deste aparelho venceram (os
      // certificados do navegador são curtos), e a sessão não trouxe novos.
      "link-vencido": Object.freeze(["O LINK DESTE SERVIDOR VENCEU",
        "Este aparelho só entra com um link em dia. Peça um link novo a quem hospeda, e cole aqui."]),
      "sem-tentativa": Object.freeze(["ESTE LINK NÃO ESTÁ MAIS ABERTO",
        "Abra o link de novo, ou cole-o na tela de início."]),
      // Enquanto a página não cria a identidade dela (a 3.4 e a 2.6 da F4), um
      // link colado não tem onde abrir.
      "sem-identidade": Object.freeze(["ESTA PÁGINA AINDA NÃO ENTRA EM SERVIDOR",
        "A entrada pelo navegador ainda não está pronta nesta versão. Use o app do SEELE."]),
    }),
    conectando: "CONECTANDO A",
    // Na espera da portaria, o que o celular faz com ela (spec §2.1).
    esperaPausa: "Com esta página fora da tela, em outro app ou com a tela apagada, a verificação para, e volta quando você volta a ela.",
    // O registro e o anúncio da entrada sem o aperto (D-celular-3, P11).
    entradaDireta: Object.freeze({
      registro: "IMPRESSÃO CONFERIDA NA ENTRADA — ENTRANDO",
      anuncio: "Impressão conferida. Entrando no servidor.",
    }),
  }),
});

const FRASES_DA_LARGA = (() => {
  const soNoApp = SO_NO_APP_DO_SEELE("este computador");
  return Object.freeze({
      proposito: "Voz, vídeo e texto entre pessoas, sem servidor de ninguém no meio. Entre no servidor de alguém pelo link que a pessoa mandou.",
      hospedar: "Hospedar um servidor é pelo app do SEELE. ",
      baixar: "BAIXAR O APP DO SEELE",
      atualizacao: "A página vem da origem: uma versão nova chega ao abrir a página de novo.",
      versoes: soNoApp.versoes[1],
      malha: soNoApp.telaEntrePessoas[1],
      tecla: "No navegador, a tecla de falar só vale com esta página em foco: o navegador não tem atalho para o computador inteiro.",
      comoAbre: "COMO ESTA PÁGINA ABRE",
      comoAbreSub: "Como computador ou como celular, na próxima vez que ela abrir.",
      abriuComoComputador: "Esta página abriu como computador.",
      abrirComoCelular: "ABRIR COMO CELULAR",
      pelaTela: "PELO TAMANHO DA TELA",
      valeNaProxima: "Vale na próxima vez que a página abrir. Abrir de novo agora sai da sessão.",
      naoGuardou: "Este navegador não deixou guardar a escolha: a página continua abrindo pelo tamanho da tela.",
      foraTitulo: "O QUE O NAVEGADOR NÃO FAZ",
      foraSub: "O que fica no app do SEELE, instalado num computador.",
      grupo: "SÓ NO APP DO SEELE",
      itens: soNoApp,
  });
})();
