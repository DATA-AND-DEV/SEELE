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
    ImpressaoDigitalInvalida: "ESTE CONVITE CHEGOU CORTADO OU ADULTERADO",
    TokenInvalido: "O CONVITE DENTRO DESTE LINK NÃO É UM CONVITE",
    CageInvalido: "O CAGE DESTE CONVITE NÃO É UM NÚMERO",

    // Hospedar aqui dentro.
    JaHospedando: "JÁ ESTOU HOSPEDANDO NESTA JANELA",
    PortaOcupada:
      "A PORTA 8383 JÁ ESTÁ EM USO.\nQuase sempre é outro SEELE aberto — feche o outro e tente de novo.",
    NaoSubiu: "NÃO CONSEGUI SUBIR O DOGMA AQUI",

    // Escolher microfone, no Terminal Dogma. Duas frases e não uma porque pedem
    // coisas diferentes de quem lê: a primeira não tem conserto na tela, e a
    // segunda tem — a lista está logo acima, e o que sumiu entre desenhá-la e
    // clicar nela pode ser trocado por outro sem sair daqui.
    NaoGravei: "NÃO CONSEGUI GRAVAR ESSE AJUSTE NESTA MÁQUINA",
    DispositivoSumiu:
      "ESSE MICROFONE NÃO ESTÁ MAIS AQUI.\nA escolha ficou gravada; escolha outro para agora.",
};
