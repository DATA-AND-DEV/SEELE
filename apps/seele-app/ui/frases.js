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
      "O ROTEADOR ABRIU A PORTA.\nEste link deve funcionar pela internet.",
    Ipv6Direto:
      "ESTE LINK É IPv6.\nAlcança de qualquer lugar, mas só quem também tiver IPv6. Quem não tiver precisa estar na sua rede.",
    SoRedeLocal:
      "ESTE LINK SÓ FUNCIONA NA SUA REDE.\nNão consegui abrir a porta no roteador. Para alcançar de fora: encaminhe a porta 8383 no roteador à mão, ou use uma VPN.",

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
