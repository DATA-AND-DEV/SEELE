// A faixa de um pedido que o servidor não conseguiu fazer
// (`AlertReason::PedidoRecusado`), pelo caminho de verdade: o aviso no quadro,
// a faixa da sessão, e a frase de `fraseDoPedidoRecusado`.
//
// **Reprova** se a faixa não disser o que se pediu, por que não deu e o que
// fazer — a provisória de 09/10 dizia só «NÃO DEU: O SERVIDOR FALHOU» —, ou se
// um pedido que esta versão não sabe nomear abrir um buraco no meio da frase.
await entrarNaSessao();
const faixa = () => document.getElementById("banner-texto").textContent;
for (const [pedido, causa, esperada] of [
  ["SetNickname", "NomeTomado",
    "NÃO DEU PARA TROCAR O SEU APELIDO: ESSE NOME JÁ É DE OUTRA PESSOA.\nEscolha outro e tente de novo."],
  ["DeleteChannel", "NaoExiste",
    "NÃO DEU PARA APAGAR O CANAL: ISSO JÁ NÃO EXISTE.\nSumiu antes de o seu pedido chegar; não há o que refazer."],
  ["BanPerson", "FalhaDoServidor",
    "NÃO DEU PARA BANIR ESSA PESSOA: O SERVIDOR NÃO CONSEGUIU.\nTente de novo. Se repetir, quem hospeda acha o motivo no registro do servidor."],
  // A ponte manda «?» para um ordinal que esta versão não conhece.
  ["?", "FalhaDoServidor",
    "NÃO DEU: O SERVIDOR NÃO CONSEGUIU.\nTente de novo. Se repetir, quem hospeda acha o motivo no registro do servidor."],
]) {
  SEELE_QUADRO.notice = { severity: "Warning", reason: { PedidoRecusado: { pedido, causa } }, operator_text: null };
  window.__SEELE_EMITIR("RosterChanged");
  const chegou = await ate(() => faixa() === esperada, 1500);
  relatar(`${pedido}/${causa}: ${faixa().split("\n")[0]}`);
  exigir(chegou, `a faixa de ${pedido}/${causa} disse «${faixa()}», e não «${esperada}»`);
}
SEELE_QUADRO.notice = null;
