// Onde você já esteve: os servidores conhecidos, e um campo para outro.
//
// # O que esta camada muda de comportamento
//
// A comp da 0.9.0 tira o endereço da tela de entrada e o traz para aqui. O
// ganho é o caso comum: quem já usou o produto volta ao mesmo lugar, e voltar
// deixa de ser digitar um endereço para virar apertar uma linha.
//
// O campo continua embaixo porque o caso raro não pode sumir — e ele aceita
// tanto o endereço cru quanto um `seele://`, que é a forma em que alguém manda
// um servidor para outra pessoa.
//
// # O que ela não decide
//
// Como se conecta. Ela chama o que a tela de entrada já chamava; a diferença é
// só de onde vem o endereço.

"use strict";

/** Para onde o teclado volta quando esta camada fecha. */
let focoAntesDosServidores = null;

/**
 * Quando foi a última visita, em palavra.
 *
 * Relativo e não a data: «há 3 dias» responde a pergunta que se faz olhando
 * esta lista — *qual destes eu usei por último* —, e uma data obriga quem lê a
 * fazer a subtração de cabeça.
 */
function quandoFoi(segundos) {
  if (!segundos) return "";
  const agora = Math.floor(Date.now() / 1000);
  const passou = Math.max(0, agora - segundos);
  if (passou < 60) return "agora há pouco";
  if (passou < 3600) return `há ${Math.floor(passou / 60)} min`;
  if (passou < 86_400) return `há ${Math.floor(passou / 3600)} h`;
  return `há ${Math.floor(passou / 86_400)} d`;
}

/** Desenha a lista do que está gravado em disco. */
async function desenharServidores() {
  let conhecidos = [];
  try {
    conhecidos = await invoke("conhecidos");
  } catch (falha) {
    console.warn("conhecidos:", falha);
  }

  const lista = $("servidores-lista");
  // Vazio **e dizendo por quê**: uma lista sem linhas e sem explicação lê como
  // defeito, e este é o estado de quem abriu o produto pela primeira vez.
  $("servidores-vazio").hidden = conhecidos.length > 0;

  lista.replaceChildren(
    ...conhecidos.map((conhecido) => {
      const item = elemento("li");
      const botao = elemento("button", "servidor-linha");
      botao.type = "button";
      botao.dataset.alvo = conhecido.alvo;
      botao.dataset.apelido = conhecido.apelido ?? "";

      const nome = conhecido.nome || conhecido.alvo;
      botao.append(
        elemento("span", "servidor-sigla", siglaDoAlvo(conhecido.alvo)),
        (() => {
          const texto = elemento("span", "servidor-texto");
          texto.append(
            elemento("span", "servidor-nome", nome),
            elemento("span", "servidor-endereco", conhecido.alvo),
          );
          return texto;
        })(),
        elemento("span", "servidor-visto", quandoFoi(conhecido.visto_em)),
      );
      botao.title = `entrar em ${nome} como ${conhecido.apelido || "você mesmo"}`;
      return item.appendChild(botao).parentNode;
    }),
  );
}

/** Abre a camada. */
async function abrirServidores() {
  focoAntesDosServidores = document.activeElement;
  $("servidores-erro").hidden = true;
  $("servidores-endereco").value = "";
  await desenharServidores();
  $("servidores").hidden = false;
  $("servidores-endereco").focus();
  anunciar("Onde você já esteve. Escape fecha.");
}

/** Fecha, devolvendo o teclado. */
function fecharServidores() {
  $("servidores").hidden = true;
  if (focavel(focoAntesDosServidores)) focoAntesDosServidores.focus();
  focoAntesDosServidores = null;
}

/** Diz, onde se acabou de digitar, por que não deu. */
function recusarServidor(frase) {
  const onde = $("servidores-erro");
  onde.hidden = false;
  onde.textContent = frase;
}

// Uma linha da lista: entra naquele servidor, com o apelido daquela vez.
$("servidores-lista").addEventListener("click", (evento) => {
  const linha = evento.target.closest("button[data-alvo]");
  if (!linha) return;
  fecharServidores();
  irParaOServidor(linha.dataset.alvo, linha.dataset.apelido);
});

// O campo: um endereço cru ou um `seele://`.
//
// O link é resolvido **antes** de conectar, e não depois: ele carrega a
// impressão digital do certificado, e é ela que faz quem recebe não precisar
// conferi-la por outro canal. Tratá-lo como endereço perderia essa metade.
$("servidores-forma").addEventListener("submit", async (evento) => {
  evento.preventDefault();
  const escrito = $("servidores-endereco").value.trim();
  if (escrito === "") return;
  $("servidores-erro").hidden = true;

  let alvo = escrito;
  if (escrito.startsWith("seele://")) {
    try {
      const convite = await invoke("analisar_convite", { link: escrito });
      alvo = convite.alvo;
    } catch (falha) {
      console.warn("analisar_convite:", falha);
      recusarServidor(fraseDeErro(falha));
      return;
    }
  }
  fecharServidores();
  irParaOServidor(alvo, null);
});

$("servidores-fechar").addEventListener("click", fecharServidores);
fecharAoClicarFora("servidores", fecharServidores);

window.addEventListener(
  "keydown",
  (evento) => {
    if (evento.key !== "Escape" || $("servidores").hidden) return;
    evento.preventDefault();
    evento.stopPropagation();
    fecharServidores();
  },
  true,
);

/**
 * Vai para um servidor, venha-se de onde vier.
 *
 * Duas origens, e elas pedem coisas diferentes:
 *
 * - **da tela de entrada**, não há sessão a largar: preenche os campos que o
 *   `conectar` lê e conecta;
 * - **de dentro de uma sessão**, há: o `trocarDeServidor` pergunta antes, com a
 *   consequência escrita, porque sair de um servidor é largar a sala e a
 *   conversa. Perguntar é a regra desta janela para o que não se desfaz.
 *
 * O apelido vem junto quando se sabe — é o daquela visita, gravado com o
 * servidor. Sem ele, o campo da entrada decide.
 */
function irParaOServidor(alvo, apelido) {
  const naSessao = !$("tela-sessao").hidden;
  if (naSessao) {
    pedirTrocaDeServidor(alvo, apelido || undefined);
    return;
  }
  $("campo-servidor").value = alvo;
  if (apelido) $("campo-apelido").value = apelido;
  conectar().catch((falha) => console.warn("conectar:", falha));
}
