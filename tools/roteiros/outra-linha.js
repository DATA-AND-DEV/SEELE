// O servidor de outra versão (P8 do plano F1b; ADR 0058). O núcleo lê o
// primeiro byte da resposta, e um número que este app não fala chega como
// `ConnectionError::OutraLinha { numero, servidor }`. Até a 0.15 era NADA
// RESPONDEU NESSE ENDEREÇO, de um servidor que tinha respondido.
//
// **Reprova** se a frase não disser a versão do servidor (ou que ele é mais
// novo, ou antigo), se cair no ALGO FALHOU com o JSON cru, ou se usar a palavra
// «linha», que neste produto é o canal de texto.
const ALVO = "203.0.113.9:8383";
const falha = (numero, servidor, maisNova) => ({
  error: { OutraLinha: { numero, servidor, mais_nova: maisNova } },
  trail: [],
  origem_da_impressao: null,
  a_lista_guardou: false,
});
// Como a FFI os manda (`classify_connect_failure`): a versão só quando o
// launcher a abre, e se o número está acima do deste app.
for (const [numero, servidor, maisNova, esperada] of [
  // O que a v0.15.1 publicada responde: o 8.
  [8, "0.15", false, "ESTE SERVIDOR É O SERVIDOR 0.15."],
  // Um número à frente, que recusou o deste app: sem versão, e mais novo.
  [30, null, true, "ESTE SERVIDOR É DE UMA VERSÃO MAIS NOVA DO SEELE."],
  // Uma versão de antes da 0.15, que o launcher não oferece.
  [7, null, false, "ESTE SERVIDOR É DE UMA VERSÃO ANTIGA DO SEELE."],
]) {
  SEELE_RESPOSTAS.connect = { __recusa: falha(numero, servidor, maisNova) };
  await conectar(ALVO);
  await espera(200);
  const dita = document.getElementById("boot-erro").textContent;
  relatar(`${numero}: ${dita.split("\n")[0]}`);
  exigir(dita.startsWith(esperada), `o ${numero} disse «${dita}», e não «${esperada}»`);
  exigir(!/linha/i.test(dita), `a frase do ${numero} usa a palavra «linha»: ${dita}`);
}
