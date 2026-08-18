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
  Incompatible: "VERSÃO INCOMPATÍVEL COM ESTE DOGMA",
  CredentialRejected: "CREDENCIAL RECUSADA",
  HandshakeTimeout: "TEMPO ESGOTADO NA SINCRONIZAÇÃO INICIAL",
  Kicked: "DESCONECTADO POR UM OPERADOR",
  Banned: "ACESSO BARRADO POR UM OPERADOR",
  DogmaFull: "DOGMA LOTADO",
  ScheduledMaintenance: "MANUTENÇÃO PROGRAMADA",
  ServerShuttingDown: "O DOGMA ESTÁ ENCERRANDO",
  Timeout: "ENLACE PERDIDO",
  ProtocolViolation: "PROTOCOLO VIOLADO",
  RateLimited: "LIMITE DE MENSAGENS EXCEDIDO",
  LinkLost: "ENLACE PERDIDO",
};

const AVISOS = {
  Mentioned: "VOCÊ FOI CHAMADO",
  SubsystemChanged: "UM SUBSISTEMA MUDOU DE ESTADO",
  SyncDegraded: "TAXA DE SINCRONIZAÇÃO EM QUEDA",
  CageEntryRefused: "ENTRADA NO CAGE RECUSADA",
  PermissionDenied: "PERMISSÃO NEGADA",
  CageFull: "CAGE LOTADO",
  OperatorNotice: "AVISO DO OPERADOR",
  // O aviso que o Dogma manda **antes** de derrubar. É o único da lista que
  // pede uma mudança de comportamento de quem o lê, e por isso ele existe:
  // derrubar sem ter avisado é o que faz o produto parecer quebrado.
  RateLimited: "VOCÊ ESTÁ FALANDO RÁPIDO DEMAIS PARA O DOGMA",

  // ---- uma sala deixou de existir ----
  //
  // Três frases e não uma, porque pedem coisas diferentes de quem lê. As duas
  // primeiras chegam a quem estava dentro: o plug já saiu e a conversa já saiu
  // da tela quando isto aparece, e sem a frase o que sobra é uma sala que sumiu
  // sozinha — indistinguível de uma janela que perdeu a conta de onde estava.
  //
  // A terceira não é sobre uma sala que foi: é uma recusa, e a única desta
  // lista que ensina o que fazer em seguida.
  CageDeleted: "A JAULA EM QUE VOCÊ ESTAVA FOI APAGADA",
  LineDeleted: "A LINHA QUE VOCÊ LIA FOI APAGADA, COM TUDO QUE HAVIA NELA",
  LastCage:
    "ESTE É O ÚNICO CAGE DO DOGMA, E ELE FICA.\nUm Dogma sem Cage nenhum não tem onde falar. Faça outra sala antes de apagar esta.",
};

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
        "ESTE NÃO É O DOGMA DO CONVITE.\n" +
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
    UnknownPilot: "NÃO CONHEÇO ESSE PILOTO",
    UnknownChannel: "NÃO CONHEÇO ESSE CANAL",
    LinkLost: "ENLACE PERDIDO",

    // Por que um texto colado não é um convite. O Rust devolve o nome da
    // falha; a frase é daqui, como todas as outras.
    EsquemaDesconhecido: "ISTO NÃO PARECE UM CONVITE SEELE",
    SemEndereco: "ESTE CONVITE NÃO TRAZ ENDEREÇO NENHUM",
    EnderecoInvalido: "O ENDEREÇO DENTRO DESTE CONVITE NÃO É UM ENDEREÇO",
    // Frase própria, e não o `EnderecoInvalido` acima, porque esta falha tem
    // conserto na mão de quem lê: falta pontuação, não falta endereço. Mandar
    // procurar um caractere errado seria mandar procurar o que não existe.
    EnderecoIpv6SemColchetes:
      "FALTAM OS COLCHETES NESTE ENDEREÇO IPV6.\nO «:» que separa a porta é o mesmo que separa um endereço IPv6, então o endereço vai entre colchetes: seele://[2001:db8::1]:8383",
    // Degrau 4 do ADR 0022: o `enc` do link veio pela metade. Frase própria e
    // não `EnderecoInvalido` porque o que falta é outra coisa, e porque o resto
    // do link continua bom — o que se perde é o furo de NAT, não o Dogma.
    BilheteInvalido:
      "A PARTE DESTE CONVITE QUE SERVE PARA FURAR O NAT VEIO PELA METADE.\nO resto do link pode estar bom: tente de novo com o link inteiro, ou peça outro a quem hospeda.",
    ImpressaoDigitalInvalida: "ESTE CONVITE CHEGOU CORTADO OU ADULTERADO",
    TokenInvalido: "O CONVITE DENTRO DESTE LINK NÃO É UM CONVITE",
    CageInvalido: "O CAGE DESTE CONVITE NÃO É UM NÚMERO",

    // Hospedar aqui dentro.
    JaHospedando: "JÁ ESTOU HOSPEDANDO NESTA JANELA",
    PortaOcupada:
      "A PORTA 8383 JÁ ESTÁ EM USO.\nQuase sempre é outro SEELE aberto — feche o outro e tente de novo.",
    NaoSubiu: "NÃO CONSEGUI SUBIR O DOGMA AQUI",

    // Até onde o convite chega — a escada do ADR 0022. Vai junto do link, e não
    // numa tela de diagnóstico, porque é aí que a informação vale: um link que
    // só funciona na rede de casa e um link que funciona pela internet são o
    // **mesmo texto**. Sem estas frases o anfitrião manda o primeiro achando
    // que mandou o segundo, e quem descobre é o amigo, como "não conecta".
    //
    // Nenhuma promete alcance. Mesmo com a porta aberta o firewall do outro
    // lado pode recusar, e "deve funcionar" é o que dá para prometer.
    PortaNoRoteador:
      "O ROTEADOR ABRIU A PORTA.\nEste link deve funcionar pela internet, e também para quem estiver na sua rede — ele leva os dois endereços.",
    // Degrau 4, o que faz «manda o link e funciona» valer numa casa com CGNAT
    // ou com o UPnP desligado. «Deve funcionar» como as outras, e por um motivo
    // a mais: com NAT simétrico dos dois lados o furo não abre, e a saída
    // continua sendo encaminhar a porta à mão — o ADR 0022 deixou retransmissão
    // fora de escopo por decisão, então esta frase não pode prometer o que
    // nenhum código nosso entrega.
    //
    // A segunda linha diz o que o ponto de encontro aprende. Um produto que se
    // vende como «sem serviço no meio» acabou de ganhar um serviço no meio,
    // opcional — e dizer isso aqui, na tela em que o link aparece, é a
    // diferença entre honestidade e propaganda.
    FuroDeNat:
      "UM PONTO DE ENCONTRO APRESENTOU ESTA MÁQUINA: O LINK DEVE FUNCIONAR PELA INTERNET, SEM MEXER NO ROTEADOR.\nSe não funcionar, as duas redes são do tipo que não deixa furar — aí encaminhe a porta 8383 no roteador à mão. O endereço da sua rede vai junto, para quem estiver perto.\nO ponto de encontro fica sabendo que endereço falou com o seu, e quando; nunca o que foi dito. Para usar o seu, ou nenhum: docs/ponto-de-encontro.md.",
    Ipv6Direto:
      "ESTE LINK LEVA UM ENDEREÇO IPv6.\nAlcança de qualquer lugar, se o firewall do seu roteador deixar entrar, mas só quem também tiver IPv6. O endereço da sua rede vai junto, para quem estiver perto.",
    // O degrau que nasceu de um defeito de campo: um Windows com Cloudflare
    // WARP tinha IPv6 global — do túnel —, e a escada declarava «alcança de
    // qualquer lugar» embaixo de um link que não aceita entrada nenhuma. Frase
    // própria porque o que se faz a respeito é diferente das outras três: aqui
    // a saída é desligar a VPN, e não mexer no roteador.
    RedeLocalOuVpn:
      "ESTE LINK FUNCIONA NA SUA REDE, OU PARA QUEM ESTIVER NA MESMA VPN QUE VOCÊ.\nO único endereço que sai desta máquina vem de uma VPN, e VPN de navegação (WARP, Proton, Nord) não aceita conexão de entrada. Para alcançar de fora: desligue a VPN e encaminhe a porta 8383 no roteador à mão.",
    SoRedeLocal:
      "ESTE LINK SÓ FUNCIONA NA SUA REDE.\nNão consegui abrir a porta no roteador. Para alcançar de fora: encaminhe a porta 8383 no roteador à mão, ou ponha os dois lados na mesma VPN de rede (Tailscale, WireGuard).",

    // Escolher microfone, no Terminal Dogma. Duas frases e não uma porque pedem
    // coisas diferentes de quem lê: a primeira não tem conserto na tela, e a
    // segunda tem — a lista está logo acima, e o que sumiu entre desenhá-la e
    // clicar nela pode ser trocado por outro sem sair daqui.
    NaoGravei: "NÃO CONSEGUI GRAVAR ESSE AJUSTE NESTA MÁQUINA",
    DispositivoSumiu:
      "ESSE MICROFONE NÃO ESTÁ MAIS AQUI.\nA escolha ficou gravada; escolha outro para agora.",

    // ---- atualizar (ADR 0026) ----
    //
    // Seis variantes e seis frases, e a divisão não é zelo: elas pedem coisas
    // diferentes de quem está na frente da tela. Duas delas mandam **não**
    // tentar de novo — uma porque não há o que tentar neste executável, outra
    // porque tentar de novo é justamente o que não se faz com um pacote que
    // chegou assinado por outra pessoa. Escrever «não deu» nas seis mandaria
    // todo mundo apertar o botão de novo, inclusive nesses dois casos.
    //
    // As seis dizem, cada uma à sua maneira, que **esta máquina continua como
    // estava**. Não é consolo: o pacote é conferido inteiro antes de qualquer
    // arquivo instalado ser tocado, então não existe meia instalação nesses
    // caminhos, e quem lê um erro de atualizador precisa saber disso antes de
    // sair procurando o que ficou quebrado.
    NaoConfigurado:
      "ESTE SEELE SAIU SEM CHAVE DE ATUALIZAÇÃO.\n" +
      "Não é defeito: é um executável feito antes da chave existir, ou compilado do código-fonte. " +
      "Não adianta tentar de novo — baixe a versão nova da página de releases, como sempre.",
    NaoAlcancei:
      "NÃO CONSEGUI PERGUNTAR SE HÁ VERSÃO NOVA.\n" +
      "A página de releases não respondeu, ou respondeu algo que não entendi. " +
      "Nada foi baixado e nada mudou nesta máquina; tente de novo daqui a pouco.",
    SemPacoteParaEsteSistema:
      "HÁ VERSÃO NOVA, MAS NÃO PARA ESTE SISTEMA.\n" +
      "O release não traz pacote para este sistema operacional ou para este processador. " +
      "Nada foi tocado aqui.",
    AssinaturaRecusada:
      "O PACOTE BAIXADO NÃO FOI ASSINADO POR ESTE PROJETO.\n" +
      "Ele foi jogado fora sem tocar em nada instalado, e o SEELE continua o de antes. " +
      "Esta é a única falha desta lista que não é para tentar de novo: baixe da página de releases " +
      "e confira com quem hospeda de onde veio o que você estava atualizando.",
    NaoInstalei:
      "O PACOTE CHEGOU INTEIRO E CONFERIDO, E A TROCA DOS ARQUIVOS FALHOU.\n" +
      "O SEELE continua o de antes, inteiro e utilizável — não há meia instalação. " +
      "Feche outras cópias abertas e tente de novo.",
    NadaEscolhido:
      "NÃO HÁ VERSÃO NOVA ESCOLHIDA PARA INSTALAR.\n" +
      "Procure de novo: instalar sempre instala o que a última procura mostrou na tela.",
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
    "VOCÊ NÃO PODE ANEXAR ARQUIVO NESTE DOGMA.\n" +
    "Escrever e anexar são permissões separadas: quem hospeda pode deixar você " +
    "falar sem deixar você pôr arquivo no disco dele. Peça a permissão a quem hospeda.",
  TooLarge:
    "ESTE ARQUIVO É GRANDE DEMAIS PARA ESTE DOGMA.\n" +
    "Cada Dogma tem um teto de disco escolhido por quem hospeda, e o limite por " +
    "arquivo é uma fração dele — para uma subida sozinha não poder esvaziar o " +
    "histórico de todo mundo. Tentar de novo com o mesmo arquivo dá no mesmo.",
  NoRoom:
    "O DOGMA ESTÁ COM O TETO INTEIRO OCUPADO POR TRANSFERÊNCIAS EM ANDAMENTO.\n" +
    "Nada foi perdido e nada foi apagado. Tente de novo daqui a pouco.",
  SizeMismatch:
    "O ARQUIVO NÃO CHEGOU INTEIRO, E O DOGMA NÃO O GUARDOU.\n" +
    "Nada foi publicado na Linha. Mandar de novo manda o arquivo inteiro outra vez, " +
    "do começo.",
  HashDidNotMatch:
    "O QUE CHEGOU NÃO É O QUE SAIU DAQUI, E O DOGMA RECUSOU.\n" +
    "É a única pergunta que um Dogma consegue responder sobre um arquivo — se ele " +
    "chegou inteiro — e a resposta foi não. Nada foi publicado na Linha.",
  RateLimited:
    "VOCÊ ESTÁ MANDANDO ARQUIVO MAIS RÁPIDO DO QUE ESTE DOGMA ACEITA.\n" +
    "Não é castigo e não é limite de tamanho: é ritmo. Espere um pouco e mande de novo.",
  Unavailable:
    "ESTE DOGMA NÃO GUARDA ARQUIVO.\n" +
    "Quem hospeda não deu a ele um lugar para guardar. Só texto e voz por aqui.",
  NotFound:
    "ESTE ARQUIVO NÃO EXISTE NESTE DOGMA.\n" +
    "Ou ele nunca existiu, ou está numa Linha que você não pode ler.",
  Expired:
    "ESTE ARQUIVO EXPIROU.\n" +
    "O Dogma guarda anexos até um teto de disco, e ao encher o mais antigo sai. " +
    "O texto da mensagem continua; os bytes não. Peça a quem mandou para mandar de novo.",
  Malformed:
    "O DOGMA NÃO ENTENDEU O PEDIDO DE ARQUIVO.\n" +
    "Nada foi publicado na Linha. Se acontecer de novo, as duas pontas podem estar " +
    "em versões diferentes do SEELE.",
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
  Refused: "O DOGMA RECUSOU O ARQUIVO",
  Fell:
    "A TRANSFERÊNCIA CAIU.\n" +
    "Nada foi publicado na Linha, e não há de onde continuar: mandar de novo manda " +
    "o arquivo inteiro outra vez, do começo.",
  Saved: "ARQUIVO SALVO",
  NotSaved:
    "NÃO DEU PARA SALVAR O ARQUIVO.\n" +
    "Nada foi gravado pela metade. Tente de novo.",
};

/**
 * A frase de uma recusa de anexo.
 *
 * `TooLarge` chega com o limite dentro, porque «grande demais» sem número manda
 * a pessoa tentar de novo com um arquivo que também é grande demais.
 */
function fraseDeAnexo(razao) {
  if (razao && typeof razao === "object") {
    const nome = Object.keys(razao)[0];
    const base = ANEXOS[nome];
    if (!base) return `FALHA NÃO IDENTIFICADA (${nome})`;
    if (nome === "TooLarge" && typeof razao[nome]?.limit === "number") {
      return `${base}\nO limite deste Dogma é ${emBytes(razao[nome].limit)} por arquivo.`;
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
