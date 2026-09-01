// SEELE — a tela de sessão encerrada (`#tela-fim`).
//
// Diz por quê; um app que fecha calado vira suporte. Chega aqui por duas
// portas: um `ended` dentro do snapshot que `tela-sessao.js` projeta, e o
// evento `Ended` que o ouvinte de lá recebe. As duas chamam `mostrarFim`.
//
// ---- duas saídas, e a principal é voltar ----
//
// A tela oferecia só EJETAR, que levava para a entrada com o endereço para
// redigitar. Mas quem acabou de cair de um servidor quase sempre quer aquele
// servidor de volta — a queda não foi uma escolha —, e a entrada é um desvio
// para chegar onde já se estava. Então o botão principal é RECONECTAR, com o
// endereço escrito nele para não prometer um lugar que não é o certo, e SAIR
// fica como saída secundária.
//
// O endereço vem de `alvoDoServer`, que a tela de autenticação guardou ao abrir
// a sessão; o campo de endereço da entrada é a segunda fonte. Sem nenhuma das
// duas não há para onde reconectar, e a tela volta a ter um botão só — um
// RECONECTAR sem destino é o botão que promete o lugar errado.

"use strict";

/** Para onde RECONECTAR volta, ou `null` quando não se sabe. */
let alvoDoFim = null;

/**
 * O SAIR secundário.
 *
 * Nasce aqui e não na marcação porque o botão principal desta tela já existe
 * lá com o id que o `data-foco` aponta; este é o par dele. Criado uma vez e
 * reaproveitado — uma sessão pode acabar muitas vezes na mesma janela.
 */
let botaoSair = null;

function saidaSecundaria() {
  if (botaoSair) return botaoSair;
  botaoSair = elemento("button", "botao-fantasma fim-sair", "SAIR");
  botaoSair.type = "button";
  botaoSair.addEventListener("click", sairParaAEntrada);
  return botaoSair;
}

function mostrarFim(motivo) {
  $("tela-sessao").hidden = true;
  $("tela-boot").hidden = true;
  // A autenticação também: uma sessão pode acabar com a conexão ainda por
  // fazer, e quem ficasse nela veria o servidor acabar por trás de um botão que
  // promete entrar nele.
  $("tela-auth").hidden = true;
  // E a configuração, que abre por cima da sessão. Toda `.tela` tem a altura da
  // janela, então duas visíveis não se sobrepõem: empilham, e a segunda fica
  // abaixo da dobra onde ninguém a encontra.
  abandonarServer();
  // A chamada pela mesma razão, e ela é a mais provável das duas: um operador
  // que derruba alguém derruba quem está numa sala de voz, que é exatamente
  // quem está olhando esta tela.
  abandonarChamada();
  // E a moderação, que é camada e não tela: escondida junto com a sessão ela
  // não empilha, mas continuaria aberta — e reapareceria sobre a **próxima**
  // sessão, armada com um ato sobre alguém de um servidor que já ficou para
  // trás.
  abandonarModeracao();
  // E a caixa de compartilhar, pela mesma razão e com um agravante: ela guarda
  // qual fonte desta máquina estava armada, e reaparecer sobre a próxima sessão
  // ofereceria um identificador de janela de uma sessão que já acabou.
  abandonarCompartilhar();
  $("tela-fim").hidden = false;

  // Lido antes de qualquer coisa desta tela mexer no estado da anterior: é o
  // endereço da sessão que acabou, e é ele que dá destino ao RECONECTAR.
  alvoDoFim = (typeof alvoDoServer === "string" && alvoDoServer) ||
    ultimoAlvo || null;
  desenharSaidas();

  const frase = MOTIVOS[motivo] ?? null;
  $("fim-motivo").textContent = frase ?? "ENLACE ENCERRADO";
  // A única troca de tela que ninguém pediu, e a única cujo anúncio carrega o
  // motivo: quem não vê a tela precisa do porquê junto, ou fica com um botão
  // RECONECTAR focado e nenhuma explicação de por que ele apareceu. Sem motivo
  // nomeado, o `data-anuncio` da tela já diz o que há para dizer — repeti-lo
  // aqui seria anunciar «enlace encerrado» duas vezes na mesma frase.
  abrirTela("tela-fim", frase ? `Enlace encerrado. ${frase}` : undefined);
}

/**
 * Escreve os dois botões, ou o único que faz sentido.
 *
 * Com endereço, o principal diz para onde vai: um RECONECTAR sozinho não se
 * distingue de voltar para a entrada, e a diferença é justamente o destino.
 */
function desenharSaidas() {
  const principal = $("botao-voltar");
  const sair = saidaSecundaria();
  principal.disabled = false;

  if (alvoDoFim === null) {
    // Sem para onde voltar, o botão principal **é** a saída, e a secundária
    // sairia duas vezes pela mesma porta.
    principal.textContent = "SAIR";
    sair.remove();
    return;
  }

  principal.textContent = `RECONECTAR — ${alvoDoFim}`;
  principal.after(sair);
}

/**
 * Volta para o mesmo servidor.
 *
 * O `disconnect` primeiro: a sessão acabou do lado de lá, mas o estado dela
 * continua montado deste, e um `connect` por cima dele responde
 * `AlreadyConnected`. Depois é o caminho de sempre — o `conectar` da tela de
 * entrada, com o endereço no campo —, e é ele quem decide para onde isto vai:
 * a autenticação, se der certo; a própria entrada, com a linha de erro, se não.
 *
 * O convite colado **não** é limpo aqui, ao contrário da saída: o token vale
 * para este servidor, que é exatamente para onde se está voltando. Sem ele, uma
 * reconexão a um servidor com portaria bateria sem credencial.
 */
async function reconectar() {
  const botao = $("botao-voltar");
  botao.disabled = true;
  await limparSessaoEncerrada();
  $("tela-fim").hidden = true;
  $("tela-boot").hidden = false;
  // O alvo volta pela memória do `conectar`, e não por um campo: a tela de
  // entrada da 0.9.0 não tem mais nenhum. Ver `ultimoAlvo` em `tela-boot.js`.
  ultimoAlvo = alvoDoFim;
  await conectar();
  // Só se a conexão não levou ninguém a lugar nenhum: deu errado, o erro está
  // escrito na entrada, e o teclado precisa chegar até ele.
  if (!$("tela-boot").hidden) abrirTela("tela-boot");
}

/** Fecha o assunto: entrada limpa, sem tentar de novo. */
async function sairParaAEntrada() {
  await limparSessaoEncerrada();
  $("tela-fim").hidden = true;
  $("tela-boot").hidden = false;
  limparConvite();
  // Depois do redesenho, porque a lista de visitados acabou de mudar de
  // tamanho e o campo de endereço é o alvo desta tela.
  abrirTela("tela-boot");
}

/**
 * O que as duas saídas fazem igual: desmontar a sessão que acabou.
 *
 * A troca de tela em si fica com quem chama, e não aqui: o guarda de foco lê
 * cada trecho de topo inteiro, e um `hidden = false` sem o `abrirTela` do mesmo
 * trecho é exatamente a transição que deixa o teclado no `<body>`.
 */
async function limparSessaoEncerrada() {
  await invoke("disconnect");
  // O `disconnect` também derruba o servidor hospedado. A caixa some junto, ou
  // ficaria oferecendo um link que não leva mais a lugar nenhum.
  $("convite").hidden = true;
  mostrarVeredito(null);
  desenhado = null;
  linhaAberta = null;
  await encerrarBusca();
  subsistemas("", "·");
  await desenharVisitados();
}

// ------------------------------------------------------------------- ligação

$("botao-voltar").addEventListener("click", async () => {
  if (alvoDoFim === null) await sairParaAEntrada();
  else await reconectar();
});
