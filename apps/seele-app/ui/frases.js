// SEELE · Entry Plug — enum → frase, para toda tela.
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
 * (`specs/02-protocolo.md`). Este é o mesmo conjunto de frases do `plug`, em
 * português, porque é o mesmo produto.
 */
const MOTIVOS = {
  Incompatible: "VERSÃO INCOMPATÍVEL COM ESTE SERVIDOR",
  CredentialRejected: "CREDENCIAL RECUSADA",
  HandshakeTimeout: "TEMPO ESGOTADO NA SINCRONIZAÇÃO INICIAL",
  Kicked: "DESCONECTADO POR UM OPERADOR",
  Banned: "ACESSO BARRADO POR UM OPERADOR",
  DogmaFull: "SERVIDOR LOTADO",
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
    "QUEM HOSPEDA AINDA NÃO DECIDIU SOBRE VOCÊ.\n" +
    "O pedido ficou guardado e não vence: tente entrar de novo mais tarde.",
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

  LinkLost: "ENLACE PERDIDO",
};

const AVISOS = {
  Mentioned: "VOCÊ FOI CHAMADO",
  SubsystemChanged: "UM SUBSISTEMA MUDOU DE ESTADO",
  SyncDegraded: "SINAL EM QUEDA",
  CageEntryRefused: "ENTRADA NA SALA DE VOZ RECUSADA",
  PermissionDenied: "PERMISSÃO NEGADA",
  CageFull: "SALA DE VOZ LOTADA",
  OperatorNotice: "AVISO DO OPERADOR",
  // O aviso que o Dogma manda **antes** de derrubar. É o único da lista que
  // pede uma mudança de comportamento de quem o lê, e por isso ele existe:
  // derrubar sem ter avisado é o que faz o produto parecer quebrado.
  RateLimited: "VOCÊ ESTÁ FALANDO RÁPIDO DEMAIS PARA O SERVIDOR",

  // Uma transmissão de tela por sala de voz. Chega como aviso a quem tentou
  // começar a segunda, e a frase é a mesma da recusa em `FRASES` sem a segunda
  // linha: aqui ninguém apertou nada agora, então não há o que fazer em
  // seguida além de ler.
  ScreenShareTaken: "ALGUÉM JÁ ESTÁ COMPARTILHANDO A TELA NESTA SALA",

  // O Dogma parou a transmissão desta pessoa porque a sala cresceu além da
  // subida de quem hospeda (§5.1: o teto é o caminho do anfitrião ÷ quem
  // assiste). A frase diz **de quem** é o caminho que faltou, e é para isso que
  // ela não é `SyncDegraded`: «sinal em queda» mandaria quem lê conferir a
  // própria conexão, que está boa. Não manda tentar de novo — o que mudaria a
  // resposta é a sala esvaziar, e isso não está na mão de quem lê.
  ScreenShareOverHostUplink:
    "A TELA PAROU: A SALA CRESCEU ALÉM DA CONEXÃO DE QUEM HOSPEDA O SERVIDOR",

  // ---- uma sala deixou de existir ----
  //
  // Três frases e não uma, porque pedem coisas diferentes de quem lê. As duas
  // primeiras chegam a quem estava dentro: o plug já saiu e a conversa já saiu
  // da tela quando isto aparece, e sem a frase o que sobra é uma sala que sumiu
  // sozinha — indistinguível de uma janela que perdeu a conta de onde estava.
  //
  // A terceira não é sobre uma sala que foi: é uma recusa, e a única desta
  // lista que ensina o que fazer em seguida.
  CageDeleted: "A SALA DE VOZ EM QUE VOCÊ ESTAVA FOI APAGADA",
  LineDeleted: "O CANAL DE TEXTO QUE VOCÊ LIA FOI APAGADO, COM TUDO QUE HAVIA NELE",
  LastCage:
    "ESTA É A ÚNICA SALA DE VOZ DO SERVIDOR, E ELA FICA.\nFaça outra sala antes de apagar esta.",
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
const ETAPAS = {
  Parada: "LENDO O CONVITE",
  Avisando: "AVISANDO O PONTO DE ENCONTRO",
  Tentando: "TENTANDO UM ENDEREÇO DO CONVITE",
  // Diz o que abriu e não o que vem: quem lê isto ainda pode não entrar.
  CaminhoAberto: "O CAMINHO ATÉ AQUI ABRIU",
  Dentro: "DENTRO",
  // Neutra de propósito. `Desistiu` carrega o `ConnectError` inteiro — o núcleo
  // o guardou assim para não achatar `PinChanged` e `InviteMismatch`, os dois
  // erros que **não são de rede** (ADR 0003) — e afirmar aqui que nenhum
  // endereço atendeu apagaria esse alarme na tela justamente quando ele é a
  // coisa mais importante escrita nela. Esta linha diz onde a chegada parou; a
  // causa é de quem a tem, que é `fraseDeErro`.
  Desistiu: "A CHEGADA PAROU AQUI",
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
function fraseDeErro(erro) {
  if (typeof erro === "string") return FRASES[erro] ?? erro;
  if (erro && typeof erro === "object") {
    if (erro.PinChanged) {
      return (
        "A CHAVE DO SERVIDOR MUDOU.\n" +
        `fixada:   ${erro.PinChanged.pinned}\n` +
        `ofertada: ${erro.PinChanged.offered}\n` +
        "Confirme por outro canal antes de continuar."
      );
    }
    // O convite prometeu uma chave e o Dogma ofertou outra. Não é troca de
    // chave — nada estava fixado aqui — então a frase acusa o link, e não a
    // continuidade do servidor. A conexão já caiu quando isto chega: o core
    // derruba e desfaz o pin, e é por isso que este caso é `#boot-erro` e não
    // o veredito laranja da sessão.
    if (erro.InviteMismatch) {
      return (
        "ESTE NÃO É O SERVIDOR DO CONVITE.\n" +
        `esperada: ${erro.InviteMismatch.expected}\n` +
        `ofertada: ${erro.InviteMismatch.offered}\n` +
        "Confirme o link com quem o mandou."
      );
    }
    if (erro.Refused) {
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
    detalhe = typeof erro === "object" ? JSON.stringify(erro) : String(erro);
  } catch {
    // Um objeto com ciclo. Raro, e ainda assim melhor dizer o tipo que nada.
    detalhe = Object.prototype.toString.call(erro);
  }
  return `FALHA QUE ESTA TELA NÃO SABE NOMEAR:\n${detalhe}`;
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
    HandshakeTimeout: "TEMPO ESGOTADO NA SINCRONIZAÇÃO INICIAL",
    IdentityUnavailable: "NÃO CONSEGUI LER OU GRAVAR A IDENTIDADE EM DISCO",
    NoAudioDevice: "SEM DISPOSITIVO DE ÁUDIO",
    UnknownPilot: "NÃO CONHEÇO ESSA PESSOA",
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

    // Separada da de cima porque as duas mandam a pessoa fazer coisas
    // diferentes, e enquanto foram uma só quem lia parava de tentar: aquela diz
    // que o recurso não existe, e esta diz que falta um arquivo.
    //
    // O módulo do OpenH264 não vem no pacote porque a licença não deixa. A
    // frase diz o tamanho porque um megabyte é uma decisão fácil de tomar, e
    // «baixar algo da internet» sem número é uma difícil.
    ScreenModuleRefused:
      "O MÓDULO DE VÍDEO NÃO INSTALOU.\n" +
      "Ou o download veio quebrado, ou a pasta de configuração não aceitou o arquivo. " +
      "Tente de novo; se insistir, é a pasta.",

    ScreenModuleMissing:
      "FALTA O MÓDULO DE VÍDEO.\n" +
      "Este app captura a tela; o codec que comprime a imagem não vem junto, " +
      "porque a licença dele não deixa. É cerca de 1 MB, uma vez só.",

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
    // do link continua bom — o que se perde é o furo de NAT, não o Dogma.
    BilheteInvalido:
      "ESTE CONVITE VEIO CORTADO NA PARTE QUE FURA O NAT.",
    ImpressaoDigitalInvalida: "ESTE CONVITE CHEGOU CORTADO OU ADULTERADO",
    TokenInvalido: "O CONVITE DENTRO DESTE LINK NÃO É UM CONVITE",
    CageInvalido: "A SALA DE VOZ DESTE CONVITE NÃO É UM NÚMERO",

    // Hospedar aqui dentro.
    JaHospedando: "JÁ ESTOU HOSPEDANDO NESTA JANELA",
    PortaOcupada:
      "A PORTA 8383 JÁ ESTÁ EM USO.\nQuase sempre é outro SEELE aberto — feche o outro e tente de novo.",
    NaoSubiu: "NÃO CONSEGUI SUBIR O SERVIDOR AQUI",

    // A portaria — ADR 0030. As duas falam de uma porta que não existe daqui.
    //
    // `NaoEstaHospedando` não é engano de quem apertou: os comandos da portaria
    // mexem no Dogma **desta** máquina, e quem está no Dogma de outra pessoa não
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
    Ipv6Direto:
      "ESTE LINK LEVA UM ENDEREÇO IPv6.",
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

    // Escolher microfone, no Terminal Dogma. Duas frases e não uma porque pedem
    // coisas diferentes de quem lê: a primeira não tem conserto na tela, e a
    // segunda tem — a lista está logo acima, e o que sumiu entre desenhá-la e
    // clicar nela pode ser trocado por outro sem sair daqui.
    NaoGravei: "NÃO CONSEGUI GRAVAR ESSE AJUSTE NESTA MÁQUINA",
    DispositivoSumiu:
      "ESSE MICROFONE NÃO ESTÁ MAIS AQUI.",

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
  NotSaved: "NÃO DEU PARA SALVAR O ARQUIVO.\nNada foi gravado pela metade; tente de novo.",
};

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
